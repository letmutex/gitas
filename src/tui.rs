use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, BeginSynchronizedUpdate, ClearType, EndSynchronizedUpdate},
};
use std::io::{Write, stdout};
use std::thread;
use std::time::{Duration, Instant};

fn character_width(character: char) -> usize {
    match character as u32 {
        0x0000..=0x001f | 0x007f..=0x009f | 0x0300..=0x036f => 0,
        0x1100..=0x115f
        | 0x2329..=0x232a
        | 0x2e80..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xfe10..=0xfe19
        | 0xfe30..=0xfe6f
        | 0xff00..=0xff60
        | 0xffe0..=0xffe6
        | 0x1f300..=0x1faff
        | 0x20000..=0x3fffd => 2,
        _ => 1,
    }
}

pub(crate) fn visible_line_width(line: &str) -> usize {
    let mut in_escape = false;
    line.chars()
        .filter_map(|character| {
            if in_escape {
                if character.is_ascii_alphabetic() {
                    in_escape = false;
                }
                None
            } else if character == '\x1b' {
                in_escape = true;
                None
            } else {
                Some(character_width(character))
            }
        })
        .sum()
}

/// Keep a rendered row away from the terminal's last column. Writing into that
/// column can trigger an implicit wrap, which breaks logical-line cursor math.
pub(crate) fn truncate_rendered_line(line: &str, max_width: usize) -> String {
    if visible_line_width(line) <= max_width {
        return line.to_string();
    }
    if max_width == 0 {
        return String::new();
    }

    let content_width = max_width - 1;
    let mut result = String::new();
    let mut width = 0;
    let mut in_escape = false;

    for character in line.chars() {
        if in_escape {
            result.push(character);
            if character.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        if character == '\x1b' {
            in_escape = true;
            result.push(character);
            continue;
        }

        let character_width = character_width(character);
        if width + character_width > content_width {
            break;
        }
        result.push(character);
        width += character_width;
    }

    result.push('…');
    result.push_str("\x1b[0m");
    result
}

fn terminal_line_width() -> usize {
    terminal::size()
        .map(|(columns, _)| usize::from(columns).saturating_sub(1))
        .unwrap_or(79)
}

fn prev_char_boundary(value: &str, index: usize) -> usize {
    value[..index].char_indices().last().map_or(0, |(i, _)| i)
}

fn next_char_boundary(value: &str, index: usize) -> usize {
    if index >= value.len() {
        value.len()
    } else {
        value[index..]
            .chars()
            .next()
            .map_or(value.len(), |c| index + c.len_utf8())
    }
}

fn input_cursor_column(prompt: &str, value: &str, cursor_index: usize) -> u16 {
    let prefix = format!("  {}: ", prompt);
    let prefix_width = prefix.chars().count();
    let cursor_width = value[..cursor_index].chars().count();
    (prefix_width + cursor_width) as u16
}

fn status_display_duration_ms(line_count: usize, has_issue: bool) -> u64 {
    if has_issue {
        5000
    } else if line_count > 3 {
        2500
    } else {
        1500
    }
}

/// Enter raw mode and hide cursor.
pub fn enter_raw_mode() {
    terminal::enable_raw_mode().ok();
    execute!(stdout(), cursor::Hide).ok();
}

/// Exit raw mode and show cursor.
pub fn exit_raw_mode() {
    execute!(stdout(), cursor::Show).ok();
    terminal::disable_raw_mode().ok();
}

/// Print line in raw mode (handles \r\n).
pub fn raw_println(msg: &str) {
    let mut stdout = stdout();
    crossterm::queue!(
        stdout,
        crossterm::style::Print(msg),
        crossterm::style::Print("\r\n")
    )
    .ok();
    stdout.flush().ok();
}

/// Run blocking work off the terminal thread while displaying an animated loader.
pub fn raw_with_loader<T, F>(message: &str, work: F) -> thread::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    // Fast operations should complete without flashing a loader for a handful of
    // frames. Once shown, keep it around long enough to read as intentional UI.
    const SHOW_DELAY: Duration = Duration::from_millis(200);
    const MIN_VISIBLE_DURATION: Duration = Duration::from_millis(300);
    const FRAME_INTERVAL: Duration = Duration::from_millis(80);
    let handle = thread::spawn(work);
    let mut stdout = stdout();
    let mut frame = 0;
    let started_at = Instant::now();

    while !handle.is_finished() && started_at.elapsed() < SHOW_DELAY {
        thread::sleep(Duration::from_millis(10));
    }

    if handle.is_finished() {
        return handle.join();
    }

    let shown_at = Instant::now();
    let max_width = terminal_line_width();

    while !handle.is_finished() || shown_at.elapsed() < MIN_VISIBLE_DURATION {
        let line = format!("  {} {}", FRAMES[frame].cyan(), message);
        let line = truncate_rendered_line(&line, max_width);
        // Rewrite only the spinner row. Clearing it and printing a newline on
        // every frame caused flashes on terminals without sync support.
        crossterm::queue!(
            stdout,
            cursor::MoveToColumn(0),
            crossterm::style::Print(&line)
        )
        .ok();
        stdout.flush().ok();
        frame = (frame + 1) % FRAMES.len();
        thread::sleep(FRAME_INTERVAL);
    }

    // Leave the last frame in place. raw_show_status replaces it in the same
    // buffered update, avoiding a blank frame between loader and result.
    handle.join()
}

/// Render lines at current position using per-line clear (flicker-free).
fn raw_render_lines(stdout: &mut impl Write, lines: &[String], prev_count: usize) {
    let max_width = terminal_line_width();
    crossterm::queue!(stdout, BeginSynchronizedUpdate).ok();
    if prev_count > 0 {
        crossterm::queue!(stdout, cursor::MoveUp(prev_count as u16)).ok();
    }
    for line in lines {
        let line = truncate_rendered_line(line, max_width);
        crossterm::queue!(
            stdout,
            cursor::MoveToColumn(0),
            crossterm::style::Print(&line),
            terminal::Clear(ClearType::UntilNewLine),
            crossterm::style::Print("\r\n")
        )
        .ok();
    }
    if lines.len() < prev_count {
        let extra = prev_count - lines.len();
        for _ in 0..extra {
            crossterm::queue!(
                stdout,
                terminal::Clear(ClearType::CurrentLine),
                crossterm::style::Print("\r\n")
            )
            .ok();
        }
        crossterm::queue!(stdout, cursor::MoveUp(extra as u16)).ok();
    }
    crossterm::queue!(stdout, EndSynchronizedUpdate).ok();
    stdout.flush().ok();
}

/// Clear N lines above cursor.
pub fn raw_clear_lines(stdout: &mut impl Write, count: usize) {
    if count == 0 {
        return;
    }
    crossterm::queue!(
        stdout,
        BeginSynchronizedUpdate,
        cursor::MoveUp(count as u16)
    )
    .ok();
    for _ in 0..count {
        crossterm::queue!(
            stdout,
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print("\r\n")
        )
        .ok();
    }
    crossterm::queue!(stdout, cursor::MoveUp(count as u16), EndSynchronizedUpdate).ok();
    stdout.flush().ok();
}

/// Arrow-key select menu. Returns selected index or None on Esc.
pub fn raw_select(prompt: &str, items: &[String], default: usize) -> Option<usize> {
    let mut stdout = stdout();
    let mut pos = default;
    let mut prev_lines = 0;

    loop {
        let mut lines = Vec::new();
        lines.push(format!("  {}", prompt));
        for (i, item) in items.iter().enumerate() {
            if i == pos {
                lines.push(format!("  {} {}", ">".yellow().bold(), item));
            } else {
                lines.push(format!("    {}", item));
            }
        }

        raw_render_lines(&mut stdout, &lines, prev_lines);
        prev_lines = lines.len();

        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                pos = if pos == 0 { items.len() - 1 } else { pos - 1 };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                pos = (pos + 1) % items.len();
            }
            KeyCode::Enter => {
                raw_clear_lines(&mut stdout, prev_lines);
                return Some(pos);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                raw_clear_lines(&mut stdout, prev_lines);
                return None;
            }
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                raw_clear_lines(&mut stdout, prev_lines);
                return None;
            }
            _ => {}
        }
    }
}

/// y/n confirmation. Returns Some(bool) or None on Esc.
pub fn raw_confirm(prompt: &str, default: bool) -> Option<bool> {
    let mut stdout = stdout();
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    let line = format!("  {} {}", prompt, hint.dimmed());

    crossterm::queue!(
        stdout,
        crossterm::style::Print(&line),
        crossterm::style::Print("\r\n")
    )
    .ok();
    stdout.flush().ok();

    loop {
        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                raw_clear_lines(&mut stdout, 1);
                return Some(true);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                raw_clear_lines(&mut stdout, 1);
                return Some(false);
            }
            KeyCode::Enter => {
                raw_clear_lines(&mut stdout, 1);
                return Some(default);
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                raw_clear_lines(&mut stdout, 1);
                return None;
            }
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                raw_clear_lines(&mut stdout, 1);
                return None;
            }
            _ => {}
        }
    }
}

/// Text input with default. Returns Some(value) on Enter, None on Esc.
pub fn raw_input(prompt: &str, default: &str) -> Option<String> {
    let mut stdout = stdout();
    let mut value = default.to_string();
    let mut cursor_index = value.len();

    // Show cursor while typing
    execute!(stdout, cursor::Show).ok();

    loop {
        let display = format!("  {}: {}", prompt, value);
        crossterm::queue!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print(&display),
            cursor::MoveToColumn(input_cursor_column(prompt, &value, cursor_index)),
        )
        .ok();
        stdout.flush().ok();

        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Enter => {
                crossterm::queue!(
                    stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(ClearType::CurrentLine),
                )
                .ok();
                execute!(stdout, cursor::Hide).ok();
                return Some(value);
            }
            KeyCode::Esc => {
                crossterm::queue!(
                    stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(ClearType::CurrentLine),
                )
                .ok();
                execute!(stdout, cursor::Hide).ok();
                return None;
            }
            KeyCode::Backspace => {
                if cursor_index > 0 {
                    let prev_index = prev_char_boundary(&value, cursor_index);
                    value.drain(prev_index..cursor_index);
                    cursor_index = prev_index;
                }
            }
            KeyCode::Delete => {
                if cursor_index < value.len() {
                    let next_index = next_char_boundary(&value, cursor_index);
                    value.drain(cursor_index..next_index);
                }
            }
            KeyCode::Left => {
                if cursor_index > 0 {
                    cursor_index = prev_char_boundary(&value, cursor_index);
                }
            }
            KeyCode::Right => {
                if cursor_index < value.len() {
                    cursor_index = next_char_boundary(&value, cursor_index);
                }
            }
            KeyCode::Home => {
                cursor_index = 0;
            }
            KeyCode::End => {
                cursor_index = value.len();
            }
            KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                value.clear();
                cursor_index = 0;
            }
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                crossterm::queue!(
                    stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(ClearType::CurrentLine),
                )
                .ok();
                execute!(stdout, cursor::Hide).ok();
                return None;
            }
            KeyCode::Char(c) => {
                value.insert(cursor_index, c);
                cursor_index += c.len_utf8();
            }
            _ => {}
        }
    }
}

/// Password input (masked). Returns Some(value) or None.
pub fn raw_password(prompt: &str) -> Option<String> {
    let mut stdout = stdout();
    let mut value = String::new();
    let mut cursor_index = 0;

    execute!(stdout, cursor::Show).ok();

    loop {
        let mask = "*".repeat(value.len());
        let display = format!("  {}: {}", prompt, mask);
        crossterm::queue!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print(&display),
            cursor::MoveToColumn(input_cursor_column(prompt, &mask, cursor_index)),
        )
        .ok();
        stdout.flush().ok();

        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Enter => {
                crossterm::queue!(
                    stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(ClearType::CurrentLine),
                )
                .ok();
                execute!(stdout, cursor::Hide).ok();
                return Some(value);
            }
            KeyCode::Esc => {
                crossterm::queue!(
                    stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(ClearType::CurrentLine),
                )
                .ok();
                execute!(stdout, cursor::Hide).ok();
                return None;
            }
            KeyCode::Backspace => {
                if cursor_index > 0 {
                    let prev_index = prev_char_boundary(&value, cursor_index);
                    value.drain(prev_index..cursor_index);
                    cursor_index = prev_index;
                }
            }
            KeyCode::Delete => {
                if cursor_index < value.len() {
                    let next_index = next_char_boundary(&value, cursor_index);
                    value.drain(cursor_index..next_index);
                }
            }
            KeyCode::Left => {
                if cursor_index > 0 {
                    cursor_index = prev_char_boundary(&value, cursor_index);
                }
            }
            KeyCode::Right => {
                if cursor_index < value.len() {
                    cursor_index = next_char_boundary(&value, cursor_index);
                }
            }
            KeyCode::Home => {
                cursor_index = 0;
            }
            KeyCode::End => {
                cursor_index = value.len();
            }
            KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                value.clear();
                cursor_index = 0;
            }
            KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                crossterm::queue!(
                    stdout,
                    cursor::MoveToColumn(0),
                    terminal::Clear(ClearType::CurrentLine),
                )
                .ok();
                execute!(stdout, cursor::Hide).ok();
                return None;
            }
            KeyCode::Char(c) => {
                value.insert(cursor_index, c);
                cursor_index += c.len_utf8();
            }
            _ => {}
        }
    }
}

/// Show status message lines, sleep, then clear them.
pub fn raw_show_status(lines: &[String], has_issue: bool) {
    let mut stdout = stdout();
    let max_width = terminal_line_width();

    crossterm::queue!(stdout, BeginSynchronizedUpdate, cursor::MoveToColumn(0)).ok();
    for line in lines {
        let line = truncate_rendered_line(line, max_width);
        crossterm::queue!(
            stdout,
            crossterm::style::Print(&line),
            terminal::Clear(ClearType::UntilNewLine),
            crossterm::style::Print("\r\n")
        )
        .ok();
    }
    crossterm::queue!(stdout, EndSynchronizedUpdate).ok();
    stdout.flush().ok();

    let duration = Duration::from_millis(status_display_duration_ms(lines.len(), has_issue));
    let start = Instant::now();

    while start.elapsed() < duration {
        let remaining = duration.saturating_sub(start.elapsed());
        let poll_timeout = remaining.min(Duration::from_millis(100));

        let Ok(has_event) = event::poll(poll_timeout) else {
            continue;
        };
        if !has_event {
            continue;
        }

        let Ok(Event::Key(key)) = event::read() else {
            continue;
        };
        if key.kind == KeyEventKind::Press && key.code == KeyCode::Enter {
            break;
        }
    }

    raw_clear_lines(&mut stdout, lines.len());
}
