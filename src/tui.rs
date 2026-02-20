use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, ClearType},
};
use std::io::{Write, stdout};

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

/// Render lines at current position using per-line clear (flicker-free).
fn raw_render_lines(stdout: &mut impl Write, lines: &[String], prev_count: usize) {
    if prev_count > 0 {
        crossterm::queue!(stdout, cursor::MoveUp(prev_count as u16)).ok();
    }
    for line in lines {
        crossterm::queue!(
            stdout,
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print(line),
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
    stdout.flush().ok();
}

/// Clear N lines above cursor.
pub fn raw_clear_lines(stdout: &mut impl Write, count: usize) {
    if count == 0 {
        return;
    }
    crossterm::queue!(stdout, cursor::MoveUp(count as u16)).ok();
    for _ in 0..count {
        crossterm::queue!(
            stdout,
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print("\r\n")
        )
        .ok();
    }
    crossterm::queue!(stdout, cursor::MoveUp(count as u16)).ok();
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

    // Show cursor while typing
    execute!(stdout, cursor::Show).ok();

    loop {
        let display = format!("  {}: {}", prompt, value);
        crossterm::queue!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print(&display),
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
                value.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                value.clear();
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
                value.push(c);
            }
            _ => {}
        }
    }
}

/// Password input (masked). Returns Some(value) or None.
pub fn raw_password(prompt: &str) -> Option<String> {
    let mut stdout = stdout();
    let mut value = String::new();

    execute!(stdout, cursor::Show).ok();

    loop {
        let mask = "*".repeat(value.len());
        let display = format!("  {}: {}", prompt, mask);
        crossterm::queue!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            crossterm::style::Print(&display),
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
                value.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                value.clear();
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
                value.push(c);
            }
            _ => {}
        }
    }
}

/// Show status message lines, sleep, then clear them.
pub fn raw_show_status(lines: &[String], duration_ms: u64) {
    let mut stdout = stdout();

    for line in lines {
        crossterm::queue!(
            stdout,
            crossterm::style::Print(line),
            crossterm::style::Print("\r\n")
        )
        .ok();
    }
    stdout.flush().ok();

    std::thread::sleep(std::time::Duration::from_millis(duration_ms));
    raw_clear_lines(&mut stdout, lines.len());
}
