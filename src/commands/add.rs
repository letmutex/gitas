use crate::github;
use crate::models::{Account, Config, save_config, set_token};
use colored::Colorize;
use dialoguer::{Confirm, Input, Password, Select};

pub fn run(config: &mut Config) {
    println!();
    println!("  {}", "Add Git Account".bold());
    println!("  {}", "─".repeat(48).dimmed());
    println!();

    let methods = ["Manual Input", "GitHub Browser Login"];
    let selection = Select::new()
        .with_prompt("  Authentication Method")
        .items(&methods)
        .default(0)
        .interact_opt()
        .unwrap_or(None);

    match selection {
        Some(0) => add_manual(config),
        Some(1) => add_github(config),
        _ => {}
    }
}

fn add_github(config: &mut Config) {
    if let Some((username, email, _name, token)) = github::login() {
        println!(
            "  Authenticated as: {} <{}>",
            username.cyan(),
            email.dimmed()
        );

        let alias: String = Input::new()
            .with_prompt("  Alias (optional, press Enter to skip)")
            .default(String::new())
            .interact_text()
            .unwrap_or_default();

        let alias = if alias.is_empty() { None } else { Some(alias) };

        // Check for duplicate (username + alias)
        let existing_idx = config
            .accounts
            .iter()
            .position(|a| a.username == username && a.alias == alias);

        if existing_idx.is_some() {
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "  Account '{}' (alias: {}) already exists. Overwrite?",
                    username.yellow(),
                    alias.as_deref().unwrap_or("none").yellow()
                ))
                .default(false)
                .interact()
                .unwrap_or(false);

            if !confirmed {
                println!("\n  {}\n", "Cancelled.".dimmed());
                return;
            }
        }

        let account = Account {
            username: username.clone(),
            email,
            alias: alias.clone(),
            host: None, // Defaults to github.com
        };

        // Securely store the token
        set_token(&username, alias.as_deref(), &token);

        if let Some(idx) = existing_idx {
            upsert_account(config, account, Some(idx));
        } else {
            upsert_account(config, account, None);
        }
    }
}

fn add_manual(config: &mut Config) {
    let username: String = Input::new()
        .with_prompt("  Username")
        .interact_text()
        .expect("Failed to read input");

    let email: String = Input::new()
        .with_prompt("  Email")
        .interact_text()
        .expect("Failed to read input");

    let alias: String = Input::new()
        .with_prompt("  Alias (optional, press Enter to skip)")
        .default(String::new())
        .interact_text()
        .unwrap_or_default();

    let alias = if alias.is_empty() { None } else { Some(alias) };

    // Check for duplicate (username + alias)
    let existing_idx = config
        .accounts
        .iter()
        .position(|a| a.username == username && a.alias == alias);

    if existing_idx.is_some() {
        let confirmed = Confirm::new()
            .with_prompt(format!(
                "  Account '{}' (alias: {}) already exists. Overwrite?",
                username.yellow(),
                alias.as_deref().unwrap_or("none").yellow()
            ))
            .default(false)
            .interact()
            .unwrap_or(false);

        if !confirmed {
            println!("\n  {}\n", "Cancelled.".dimmed());
            return;
        }
    }

    let token: String = Password::new()
        .with_prompt("  Token/PAT (optional, press Enter to skip)")
        .allow_empty_password(true)
        .interact()
        .unwrap_or_default();

    let host: String = Input::new()
        .with_prompt("  Host")
        .default("github.com".to_string())
        .interact_text()
        .unwrap_or_else(|_| "github.com".to_string());

    let host = if host == "github.com" {
        None
    } else {
        Some(host)
    };

    let account = Account {
        username: username.clone(),
        email,
        alias: alias.clone(),
        host,
    };

    // Securely store the token if provided
    if !token.is_empty() {
        set_token(&username, alias.as_deref(), &token);
    } else {
        // Explicitly clear token if it was overwritten with empty
        crate::models::delete_token(&username, alias.as_deref());
    }

    if let Some(idx) = existing_idx {
        upsert_account(config, account, Some(idx));
    } else {
        upsert_account(config, account, None);
    }
}

fn upsert_account(config: &mut Config, account: Account, index: Option<usize>) {
    let username = account.username.clone();
    if let Some(idx) = index {
        config.accounts[idx] = account;
        println!(
            "\n  {} Account '{}' updated successfully.\n",
            "✓".green().bold(),
            username.cyan()
        );
    } else {
        config.accounts.push(account);
        println!(
            "\n  {} Account '{}' added successfully.\n",
            "✓".green().bold(),
            username.cyan()
        );
    }
    save_config(config);
}
