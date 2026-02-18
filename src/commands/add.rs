use crate::github;
use crate::models::{Account, Config, save_config, set_token};
use crate::tui::{
    enter_raw_mode, exit_raw_mode, raw_confirm, raw_input, raw_password, raw_println, raw_select,
};
use colored::Colorize;

pub fn run(config: &mut Config) {
    enter_raw_mode(); // Start raw mode immediately

    raw_println("");
    raw_println(&format!("  {}", "Add Git Account".bold()));
    raw_println(&format!("  {}", "─".repeat(48).dimmed()));
    raw_println("");

    let methods = vec![
        "Manual Input".to_string(),
        "GitHub Browser Login".to_string(),
    ];

    let selection = raw_select("Authentication Method", &methods, 0);

    match selection {
        Some(0) => {
            // Manual - stay in raw mode
            add_manual(config);
            exit_raw_mode();
        }
        Some(1) => {
            // GitHub - exit raw mode because github::login prints standard output and opens browser
            exit_raw_mode();
            add_github(config);
        }
        _ => {
            exit_raw_mode();
        }
    }
}

fn add_github(config: &mut Config) {
    // Normal terminal mode
    if let Some((username, email, _name, token)) = github::login() {
        println!(
            "  Authenticated as: {} <{}>",
            username.cyan(),
            email.dimmed()
        );

        // We could re-enter raw mode here for the alias input, but mixing modes is complex.
        // Let's stick to standard input for consistency within this flow since we already left raw mode.
        // We'll use dialoguer just for this part as fallback, or just manual input reading?
        // Dialoguer is still a dependency, so we can use it.
        // Or we can use raw_input by re-entering raw mode. Re-entering is cleaner for UI consistency.

        enter_raw_mode();

        let alias = raw_input("Alias (optional)", "").unwrap_or_default();
        let alias = if alias.is_empty() { None } else { Some(alias) };

        // Check for duplicate
        let existing_idx = config
            .accounts
            .iter()
            .position(|a| a.username == username && a.alias == alias);

        if existing_idx.is_some() {
            let prompt = format!(
                "Account '{}' (alias: {}) already exists. Overwrite?",
                username.yellow(),
                alias.as_deref().unwrap_or("none").yellow()
            );

            match raw_confirm(&prompt, false) {
                Some(true) => {}
                _ => {
                    raw_println(&format!("\n  {}\n", "Cancelled.".dimmed()));
                    exit_raw_mode();
                    return;
                }
            }
        }

        let account = Account {
            username: username.clone(),
            email,
            alias: alias.clone(),
            host: None,
        };

        set_token(&username, alias.as_deref(), &token);

        if let Some(idx) = existing_idx {
            upsert_account_raw(config, account, Some(idx));
        } else {
            upsert_account_raw(config, account, None);
        }
        exit_raw_mode();
    }
}

fn add_manual(config: &mut Config) {
    let username = match raw_input("Username", "") {
        Some(u) if !u.is_empty() => u,
        _ => return,
    };

    let email = match raw_input("Email", "") {
        Some(e) if !e.is_empty() => e,
        _ => return,
    };

    let alias = raw_input("Alias (optional)", "").unwrap_or_default();
    let alias = if alias.is_empty() { None } else { Some(alias) };

    // Check duplicate
    let existing_idx = config
        .accounts
        .iter()
        .position(|a| a.username == username && a.alias == alias);

    if existing_idx.is_some() {
        let prompt = format!(
            "Account '{}' (alias: {}) already exists. Overwrite?",
            username.yellow(),
            alias.as_deref().unwrap_or("none").yellow()
        );

        match raw_confirm(&prompt, false) {
            Some(true) => {}
            _ => {
                raw_println(&format!("\n  {}\n", "Cancelled.".dimmed()));
                return;
            }
        }
    }

    let token = raw_password("Token/PAT (optional)").unwrap_or_default();
    let host_in = raw_input("Host", "github.com").unwrap_or_else(|| "github.com".to_string());

    let host = if host_in == "github.com" || host_in.is_empty() {
        None
    } else {
        Some(host_in)
    };

    let account = Account {
        username: username.clone(),
        email,
        alias: alias.clone(),
        host,
    };

    if !token.is_empty() {
        set_token(&username, alias.as_deref(), &token);
    } else {
        crate::models::delete_token(&username, alias.as_deref());
    }

    if let Some(idx) = existing_idx {
        upsert_account_raw(config, account, Some(idx));
    } else {
        upsert_account_raw(config, account, None);
    }
}

fn upsert_account_raw(config: &mut Config, account: Account, index: Option<usize>) {
    let username = account.username.clone();
    if let Some(idx) = index {
        config.accounts[idx] = account;
        raw_println(&format!(
            "\n  {} Account '{}' updated successfully.\n",
            "✓".green().bold(),
            username.cyan()
        ));
    } else {
        config.accounts.push(account);
        raw_println(&format!(
            "\n  {} Account '{}' added successfully.\n",
            "✓".green().bold(),
            username.cyan()
        ));
    }
    save_config(config);
}
