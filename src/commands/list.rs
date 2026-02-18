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
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
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
                        self.cleanup_terminal();
                        if self.handle_switch() {
                            self.refresh_git();
                        }
                        self.setup_terminal();
                        self.render();
                    }
                    KeyCode::Backspace | KeyCode::Delete => {
                        self.cleanup_terminal();
                        if self.handle_delete() {
                            self.refresh_git();
                        }
                        self.setup_terminal();
                        self.render();
                    }
                    KeyCode::Char('e') => {
                        self.cleanup_terminal();
                        if self.handle_edit() {
                            self.refresh_git();
                        }
                        self.setup_terminal();
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

    fn setup_terminal(&self) {
        terminal::enable_raw_mode().ok();
        execute!(stdout(), cursor::Hide).ok();
    }

    fn cleanup_terminal(&mut self) {
        // Only disable raw mode/show cursor to allow dialoguer to print below the list
        execute!(stdout(), cursor::Show).ok();
        terminal::disable_raw_mode().ok();
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

        // 1. Move up if we have rendered before
        if self.last_rendered_lines > 0 {
            crossterm::queue!(
                stdout,
                cursor::MoveUp(self.last_rendered_lines as u16),
                terminal::Clear(ClearType::FromCursorDown)
            )
            .ok();
        }

        // 2. Render new frame
        for line in &frame {
            crossterm::queue!(
                stdout,
                crossterm::style::Print(line),
                crossterm::style::Print("\r\n")
            )
            .ok();
        }

        // 3. Flush
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

        // Calculate widths
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

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "  Switch to '{}'. Apply to",
                account.username.cyan()
            ))
            .items(&items)
            .default(0)
            .clear(true)
            .interact_opt()
            .unwrap_or(None);

        execute!(stdout(), cursor::MoveUp(1)).ok();

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

                let mut lines_printed = 0;

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
                            println!("{}", warning);
                            lines_printed += 1;
                        }

                        // Clear any potentially conflicting credentials
                        git_credential_reject(host);
                        git_credential_approve(&account.username, &token, host, url.as_deref());
                    }
                    _ => {
                        println!(
                            "  {} No token found for {}. Git may prompt for authentication.",
                            "⚠".yellow(),
                            account.username.cyan()
                        );
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        execute!(
                            stdout(),
                            cursor::MoveUp(1),
                            terminal::Clear(ClearType::CurrentLine)
                        )
                        .ok();
                    }
                }

                println!(
                    "\n{}   Switched to '{}' ({})",
                    "✔".green(),
                    account.username.cyan(),
                    scope.green()
                );
                lines_printed += 2;

                let sleep_ms = if lines_printed > 2 { 2500 } else { 1500 };
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                execute!(
                    stdout(),
                    cursor::MoveUp(lines_printed as u16),
                    terminal::Clear(ClearType::FromCursorDown)
                )
                .ok();
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

        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("  Remove account '{}'?", account.username.yellow()))
            .default(false)
            .interact_opt()
            .unwrap_or(None);

        execute!(
            stdout(),
            cursor::MoveUp(1),
            terminal::Clear(ClearType::CurrentLine)
        )
        .ok();

        if let Some(true) = confirmed {
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

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("  Edit Account")
                .items(&fields)
                .default(0)
                .clear(true)
                .interact_opt()
                .unwrap_or(None);

            execute!(stdout(), cursor::MoveUp(1)).ok();

            match selection {
                Some(0) => {
                    temp_account.username = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("  New Username")
                        .default(temp_account.username)
                        .interact_text()
                        .unwrap();
                    execute!(
                        stdout(),
                        cursor::MoveUp(1),
                        terminal::Clear(ClearType::CurrentLine)
                    )
                    .ok();
                }
                Some(1) => {
                    temp_account.email = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("  New Email")
                        .default(temp_account.email)
                        .interact_text()
                        .unwrap();
                    execute!(
                        stdout(),
                        cursor::MoveUp(1),
                        terminal::Clear(ClearType::CurrentLine)
                    )
                    .ok();
                }
                Some(2) => {
                    let alias: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("  New Alias (optional)")
                        .default(temp_account.alias.clone().unwrap_or_default())
                        .interact_text()
                        .unwrap();
                    execute!(
                        stdout(),
                        cursor::MoveUp(1),
                        terminal::Clear(ClearType::CurrentLine)
                    )
                    .ok();
                    temp_account.alias = if alias.is_empty() { None } else { Some(alias) };
                }
                Some(3) => {
                    let host: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("  New Host")
                        .default(
                            temp_account
                                .host
                                .clone()
                                .unwrap_or_else(|| "github.com".to_string()),
                        )
                        .interact_text()
                        .unwrap();
                    execute!(
                        stdout(),
                        cursor::MoveUp(1),
                        terminal::Clear(ClearType::CurrentLine)
                    )
                    .ok();
                    temp_account.host = if host == "github.com" || host.is_empty() {
                        None
                    } else {
                        Some(host)
                    };
                }
                Some(4) => {
                    let token: String = Input::with_theme(&ColorfulTheme::default())
                        .with_prompt("  New Token/PAT (optional)")
                        .default(current_token.clone().unwrap_or_default())
                        .interact_text()
                        .unwrap();
                    execute!(
                        stdout(),
                        cursor::MoveUp(1),
                        terminal::Clear(ClearType::CurrentLine)
                    )
                    .ok();
                    current_token = if token.is_empty() { None } else { Some(token) };
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
