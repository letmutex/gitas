use crate::models::{Config, save_config};
use crate::utils::{
    git_config_get, git_config_set, git_config_unset, git_credential_approve, git_credential_reject,
};
use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{self, ClearType},
};
use std::cmp::min;
use std::io::{Write, stdout};

pub fn run(config: &mut Config) {
    let mut state = ListState::new(config);
    state.run_loop();
}

struct ListState<'a> {
    config: &'a mut Config,
    git: GitIdentity,
    cursor: usize,
    last_rendered_lines: usize,
}

impl<'a> ListState<'a> {
    fn new(config: &'a mut Config) -> Self {
        let git = GitIdentity::fetch();
        Self {
            config,
            git,
            cursor: 0,
            last_rendered_lines: 0,
        }
    }

    fn run_loop(&mut self) {
        // Setup raw mode
        terminal::enable_raw_mode().ok();
        execute!(stdout(), cursor::Hide).ok();

        self.render();

        loop {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.move_cursor(-1);
                        self.render();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.move_cursor(1);
                        self.render();
                    }
                    KeyCode::Enter => {
                        if self.handle_switch() {
                            self.refresh_git();
                        }
                        self.render();
                    }
                    KeyCode::Backspace | KeyCode::Delete => {
                        if self.handle_delete() {
                            self.refresh_git();
                        }
                        self.render();
                    }
                    KeyCode::Char('e') => {
                        if self.handle_edit() {
                            self.refresh_git();
                        }
                        self.render();
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        break;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                        break;
                    }
                    _ => {}
                }
            }
        }

        // Cleanup on exit
        self.exit_cleanup();
    }

    fn exit_cleanup(&mut self) {
        self.clear_frame();
        execute!(stdout(), cursor::Show).ok();
        terminal::disable_raw_mode().ok();
    }

    fn refresh_git(&mut self) {
        self.git = GitIdentity::fetch();
    }

    fn move_cursor(&mut self, delta: isize) {
        let unmanaged_len = self.get_unmanaged_accounts().len();
        let total_len = (self.config.accounts.len() + unmanaged_len) as isize;

        if total_len == 0 {
            self.cursor = 0;
            return;
        }

        let current = self.cursor as isize;
        let new_pos = (current + delta).rem_euclid(total_len);
        self.cursor = new_pos as usize;
    }

    fn render(&mut self) {
        let unmanaged = self.get_unmanaged_accounts();
        let frame = self.build_frame(&unmanaged);
        let mut stdout = stdout();

        // Move to start of previous frame
        if self.last_rendered_lines > 0 {
            crossterm::queue!(stdout, cursor::MoveUp(self.last_rendered_lines as u16)).ok();
        }

        // Overwrite each line in-place (prevents flash on Windows)
        for line in &frame {
            crossterm::queue!(
                stdout,
                terminal::Clear(ClearType::CurrentLine),
                crossterm::style::Print(line),
                crossterm::style::Print("\r\n")
            )
            .ok();
        }

        // If previous frame was taller, clear leftover lines
        if frame.len() < self.last_rendered_lines {
            let extra = self.last_rendered_lines - frame.len();
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

        // Clear any stale content below the frame (leftover from submenus)
        crossterm::queue!(stdout, terminal::Clear(ClearType::FromCursorDown)).ok();

        stdout.flush().ok();
        self.last_rendered_lines = frame.len();
    }

    fn clear_frame(&mut self) {
        if self.last_rendered_lines > 0 {
            execute!(
                stdout(),
                cursor::MoveUp(self.last_rendered_lines as u16),
                terminal::Clear(ClearType::FromCursorDown)
            )
            .ok();
            self.last_rendered_lines = 0;
        }
    }

    fn get_unmanaged_accounts(&self) -> Vec<(String, String, String)> {
        let mut unmanaged = Vec::new();

        if let (Some(name), Some(email)) = (&self.git.global_name, &self.git.global_email)
            && !self
                .config
                .accounts
                .iter()
                .any(|a| &a.username == name && &a.email == email)
        {
            unmanaged.push((name.clone(), email.clone(), "global".to_string()));
        }

        if let (Some(name), Some(email)) = (&self.git.local_name, &self.git.local_email) {
            let is_known = self
                .config
                .accounts
                .iter()
                .any(|a| &a.username == name && &a.email == email);
            let is_already_listed = unmanaged.iter().any(|(n, e, _)| n == name && e == email);

            if !is_known && !is_already_listed {
                unmanaged.push((name.clone(), email.clone(), "local".to_string()));
            }
        }
        unmanaged
    }

    fn build_frame(&self, unmanaged: &[(String, String, String)]) -> Vec<String> {
        let mut frame = Vec::new();
        frame.push(String::new());
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        frame.push(format!(
            "  {} {} {}",
            "GITAS".bold(),
            "(GitHub Account Switch)".dimmed(),
            format!("v{}", VERSION).dimmed()
        ));
        frame.push(format!(
            "  {}",
            "↑↓ select · Enter switch · e edit · Backspace remove · q quit".dimmed()
        ));
        frame.push(String::new());

        // Calculate maximum available width to prevent wrapping
        let (term_cols, _) = terminal::size().unwrap_or((80, 24));
        let max_width = (term_cols as usize).saturating_sub(4); // buffer

        let name_len_fn = |name: &str, alias: Option<&String>| -> usize {
            name.len() + alias.map(|a| a.len() + 1).unwrap_or(0)
        };

        let max_name_len = self
            .config
            .accounts
            .iter()
            .map(|a| name_len_fn(&a.username, a.alias.as_ref()))
            .chain(unmanaged.iter().map(|(n, _, _)| n.len()))
            .max()
            .unwrap_or(0);

        // Ensure minimum width
        let name_width = "Username".len().max(max_name_len);

        let max_email_len = self
            .config
            .accounts
            .iter()
            .map(|a| a.email.len() + 2) // <email>
            .chain(unmanaged.iter().map(|(_, e, _)| e.len() + 2))
            .max()
            .unwrap_or(0);

        let email_width = "Email".len().max(max_email_len);

        // Header
        frame.push(format!(
            "    {:<nw$}  {:<ew$}  {}",
            "Username".dimmed(),
            "Email".dimmed(),
            "Scope".dimmed(),
            nw = name_width,
            ew = email_width
        ));

        let sep_len = name_width + email_width + 10;
        let safe_sep_len = min(sep_len, max_width);
        frame.push(format!("  {}", "─".repeat(safe_sep_len).dimmed()));

        // List Accounts
        if self.config.accounts.is_empty() && unmanaged.is_empty() {
            frame.push(format!("  {}", "No accounts found.".italic().dimmed()));
        } else {
            for (i, account) in self.config.accounts.iter().enumerate() {
                frame.push(self.format_account_line(i, account, name_width, email_width));
            }

            // Unmanaged
            let accounts_len = self.config.accounts.len();
            for (i, unmanaged_acc) in unmanaged.iter().enumerate() {
                frame.push(self.format_unmanaged_line(
                    i,
                    accounts_len,
                    unmanaged_acc,
                    name_width,
                    email_width,
                ));
            }
        }

        frame.push(format!("  {}", "─".repeat(safe_sep_len).dimmed()));
        frame.push(String::new());
        frame
    }

    fn format_account_line(
        &self,
        index: usize,
        account: &crate::models::Account,
        name_width: usize,
        email_width: usize,
    ) -> String {
        let is_current = index == self.cursor;

        let is_global = self.git.global_name.as_deref() == Some(&account.username)
            && self.git.global_email.as_deref() == Some(&account.email)
            && self.git.global_alias.as_deref() == account.alias.as_deref();

        let is_local = self.git.has_local()
            && self.git.local_name.as_deref() == Some(&account.username)
            && self.git.local_email.as_deref() == Some(&account.email)
            && self.git.local_alias.as_deref() == account.alias.as_deref();

        let pointer = if is_current {
            ">".yellow().bold().to_string()
        } else {
            " ".to_string()
        };

        let marker = if is_local {
            "●".green().bold()
        } else if is_global {
            "●".cyan().bold()
        } else {
            "○".dimmed()
        };

        // Name with alias
        let alias_part = account
            .alias
            .as_ref()
            .map(|a| format!(":{}", a).dimmed().to_string())
            .unwrap_or_default();
        let display_name = match (is_local, is_global) {
            (true, _) => format!("{}{}", account.username.green().bold(), alias_part),
            (_, true) => format!("{}{}", account.username.cyan().bold(), alias_part),
            _ => format!("{}{}", account.username.white(), alias_part),
        };

        // Padding logic
        let raw_name_len =
            account.username.len() + account.alias.as_ref().map(|a| a.len() + 1).unwrap_or(0);
        let name_pad = " ".repeat(name_width.saturating_sub(raw_name_len));

        let email_str = format!("<{}>", account.email);
        let email_pad = " ".repeat(email_width.saturating_sub(email_str.len()));

        let scope_str = if is_local {
            "local".green().to_string()
        } else if is_global {
            "global".cyan().to_string()
        } else {
            String::new()
        };

        format!(
            "{} {} {}{}  {}{}  {}",
            pointer,
            marker,
            display_name,
            name_pad,
            email_str.dimmed(),
            email_pad,
            scope_str
        )
    }

    fn format_unmanaged_line(
        &self,
        index: usize,
        accounts_len: usize,
        unmanaged: &(String, String, String),
        name_width: usize,
        email_width: usize,
    ) -> String {
        let (name, email, scope) = unmanaged;
        let is_selected = (accounts_len + index) == self.cursor;
        let pointer = if is_selected {
            ">".yellow().bold().to_string()
        } else {
            " ".to_string()
        };

        let name_pad = " ".repeat(name_width.saturating_sub(name.len()));
        let email_str = format!("<{}>", email);
        let email_pad = " ".repeat(email_width.saturating_sub(email_str.len()));

        format!(
            "{} {} {}{}  {}{}  {} {}",
            pointer,
            "●".yellow().bold(), // marker
            name.yellow(),
            name_pad,
            email_str.dimmed(),
            email_pad,
            scope.yellow(),
            "(unmanaged)".dimmed().italic()
        )
    }

    fn handle_switch(&mut self) -> bool {
        if self.config.accounts.is_empty() {
            return false;
        }

        // Prevent accessing unmanaged accounts
        if self.cursor >= self.config.accounts.len() {
            return false;
        }

        let account = &self.config.accounts[self.cursor];
        let toplevel = crate::utils::git_toplevel();
        let local_label = if let Some(ref path) = toplevel {
            format!("local {}", format!("({})", path).dimmed())
        } else {
            "local".to_string()
        };

        let items = vec![
            "global".to_string(),
            local_label,
            "Cancel".dimmed().to_string(),
        ];

        let prompt = format!("Switch to '{}'. Apply to", account.username.cyan());
        let selection = raw_select(&prompt, &items, 0);

        match selection {
            Some(0) | Some(1) => {
                let scope = if selection == Some(0) {
                    "global"
                } else {
                    "local"
                };
                git_config_set("user.name", &account.username, scope);
                git_config_set("user.email", &account.email, scope);

                if let Some(alias) = &account.alias {
                    git_config_set("gitas.alias", alias, scope);
                } else {
                    git_config_unset("gitas.alias", scope);
                }

                // Enforce the correct username for the credential helper (fixes "sticky" tokens)
                let host = account.host.as_deref().unwrap_or("github.com");
                let cred_key = format!("credential.https://{}.username", host);
                git_config_set(&cred_key, &account.username, scope);

                let mut status_lines: Vec<String> = Vec::new();

                match crate::models::get_token(&account.username, account.alias.as_deref()) {
                    Some(token) if !token.is_empty() => {
                        let host = account.host.as_deref().unwrap_or("github.com");

                        let url = if scope == "local" {
                            git_config_get("remote.origin.url", "local")
                        } else {
                            None
                        };

                        if scope == "local" && url.is_some() {
                            git_config_set("credential.useHttpPath", "true", "local");
                        }

                        if let Some(warning) = crate::utils::check_credential_helper() {
                            status_lines.push(warning);
                        }

                        // Clear any potentially conflicting credentials
                        git_credential_reject(host);
                        git_credential_approve(&account.username, &token, host, url.as_deref());
                    }
                    _ => {
                        status_lines.push(format!(
                            "  {} No token found for {}. Git may prompt for authentication.",
                            "⚠".yellow(),
                            account.username.cyan()
                        ));
                    }
                }

                status_lines.push(String::new());
                status_lines.push(format!(
                    "{}   Switched to '{}' ({})",
                    "✔".green(),
                    account.username.cyan(),
                    scope.green()
                ));

                raw_show_status(
                    &status_lines,
                    if status_lines.len() > 3 { 2500 } else { 1500 },
                );
                true
            }
            _ => false,
        }
    }

    fn handle_delete(&mut self) -> bool {
        if self.config.accounts.is_empty() {
            return false;
        }

        // Prevent accessing unmanaged accounts
        if self.cursor >= self.config.accounts.len() {
            return false;
        }

        let account = &self.config.accounts[self.cursor];
        let prompt = format!("Remove account '{}'?", account.username.yellow());

        if let Some(true) = raw_confirm(&prompt, false) {
            let username = account.username.clone();
            let alias = account.alias.clone();
            crate::models::delete_token(&username, alias.as_deref());
            self.config.accounts.remove(self.cursor);
            save_config(self.config);

            if self.cursor >= self.config.accounts.len() && self.cursor > 0 {
                self.cursor -= 1;
            }
            true
        } else {
            false
        }
    }

    fn handle_edit(&mut self) -> bool {
        if self.config.accounts.is_empty() {
            return false;
        }

        // Prevent accessing unmanaged accounts
        if self.cursor >= self.config.accounts.len() {
            return false;
        }

        let mut temp_account = self.config.accounts[self.cursor].clone();
        let original_username = temp_account.username.clone();
        let original_alias = temp_account.alias.clone();

        let mut current_token =
            crate::models::get_token(&original_username, original_alias.as_deref());

        loop {
            let fields = [
                format!("{:<15} {}", "Username:".dimmed(), temp_account.username),
                format!("{:<15} {}", "Email:".dimmed(), temp_account.email),
                format!(
                    "{:<15} {}",
                    "Alias:".dimmed(),
                    temp_account.alias.as_deref().unwrap_or("none")
                ),
                format!(
                    "{:<15} {}",
                    "Host:".dimmed(),
                    temp_account.host.as_deref().unwrap_or("github.com")
                ),
                format!(
                    "{:<15} {}",
                    "Token:".dimmed(),
                    if current_token.is_some() {
                        "*******"
                    } else {
                        "none"
                    }
                ),
                "Save Changes".green().to_string(),
                "Cancel".dimmed().to_string(),
            ];

            let items: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
            let selection = raw_select("Edit Account", &items, 0);

            match selection {
                Some(0) => {
                    if let Some(val) = raw_input("New Username", &temp_account.username)
                        && !val.is_empty()
                    {
                        temp_account.username = val;
                    }
                }
                Some(1) => {
                    if let Some(val) = raw_input("New Email", &temp_account.email)
                        && !val.is_empty()
                    {
                        temp_account.email = val;
                    }
                }
                Some(2) => {
                    if let Some(val) =
                        raw_input("New Alias", temp_account.alias.as_deref().unwrap_or(""))
                    {
                        temp_account.alias = if val.is_empty() { None } else { Some(val) };
                    }
                }
                Some(3) => {
                    if let Some(val) = raw_input(
                        "New Host",
                        temp_account.host.as_deref().unwrap_or("github.com"),
                    ) {
                        temp_account.host = if val == "github.com" || val.is_empty() {
                            None
                        } else {
                            Some(val)
                        };
                    }
                }
                Some(4) => {
                    if let Some(val) =
                        raw_input("New Token/PAT", current_token.as_deref().unwrap_or(""))
                    {
                        current_token = if val.is_empty() { None } else { Some(val) };
                    }
                }
                Some(5) => {
                    if original_username != temp_account.username
                        || original_alias != temp_account.alias
                    {
                        crate::models::delete_token(&original_username, original_alias.as_deref());
                    }
                    if let Some(t) = &current_token {
                        crate::models::set_token(
                            &temp_account.username,
                            temp_account.alias.as_deref(),
                            t,
                        );
                    } else {
                        crate::models::delete_token(
                            &temp_account.username,
                            temp_account.alias.as_deref(),
                        );
                    }

                    self.config.accounts[self.cursor] = temp_account;
                    save_config(self.config);
                    return true;
                }
                Some(6) | None => return false,
                _ => {}
            }
        }
    }
}

// ─── Raw-mode UI helpers (no terminal mode transitions) ─────────────────────

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
fn raw_clear_lines(stdout: &mut impl Write, count: usize) {
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
fn raw_select(prompt: &str, items: &[String], default: usize) -> Option<usize> {
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

        if let Ok(Event::Key(key)) = event::read() {
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
                KeyCode::Esc => {
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
}

/// y/n confirmation. Returns Some(bool) or None on Esc.
fn raw_confirm(prompt: &str, default: bool) -> Option<bool> {
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
        if let Ok(Event::Key(key)) = event::read() {
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
                KeyCode::Esc => {
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
}

/// Text input with default. Returns Some(value) on Enter, None on Esc.
fn raw_input(prompt: &str, default: &str) -> Option<String> {
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

        if let Ok(Event::Key(key)) = event::read() {
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
}

/// Show status message lines, sleep, then clear them.
fn raw_show_status(lines: &[String], duration_ms: u64) {
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

// ─── Git identity ───────────────────────────────────────────────────────────

struct GitIdentity {
    global_name: Option<String>,
    global_email: Option<String>,
    global_alias: Option<String>,
    local_name: Option<String>,
    local_email: Option<String>,
    local_alias: Option<String>,
}

impl GitIdentity {
    fn fetch() -> Self {
        Self {
            global_name: git_config_get("user.name", "global"),
            global_email: git_config_get("user.email", "global"),
            global_alias: git_config_get("gitas.alias", "global"),
            local_name: git_config_get("user.name", "local"),
            local_email: git_config_get("user.email", "local"),
            local_alias: git_config_get("gitas.alias", "local"),
        }
    }

    fn has_local(&self) -> bool {
        self.local_name.is_some() || self.local_email.is_some()
    }
}
