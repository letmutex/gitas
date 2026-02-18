use crate::models::{Account, Config};
use crate::tui::{enter_raw_mode, exit_raw_mode, raw_select};
use colored::Colorize;
use std::process::Command;

pub fn check_git_installed() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!(
            "{} Git is not installed or not in PATH.",
            "Error:".red().bold()
        );
        std::process::exit(1);
    }
}

pub fn git_config_set(key: &str, value: &str, scope: &str) {
    let scope_flag = if scope == "local" {
        "--local"
    } else {
        "--global"
    };
    let status = Command::new("git")
        .args(["config", scope_flag, key, value])
        .status()
        .expect("Failed to execute git");
    if !status.success() {
        eprintln!("{} Failed to set git config {key}", "error:".red().bold());
        std::process::exit(1);
    }
}

pub fn git_config_unset(key: &str, scope: &str) {
    let scope_flag = if scope == "local" {
        "--local"
    } else {
        "--global"
    };
    // --unset may fail if key doesn't exist; that's fine
    let _ = Command::new("git")
        .args(["config", scope_flag, "--unset", key])
        .status();
}

pub fn git_config_get(key: &str, scope: &str) -> Option<String> {
    let args = match scope {
        "local" => vec!["config", "--local", "--get", key],
        "global" => vec!["config", "--global", "--get", key],
        _ => vec!["config", "--get", key], // effective (local > global)
    };
    let output = Command::new("git").args(&args).output().ok()?;
    if output.status.success() {
        let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if val.is_empty() { None } else { Some(val) }
    } else {
        None
    }
}

pub fn git_toplevel() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if output.status.success() {
        let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if val.is_empty() { None } else { Some(val) }
    } else {
        None
    }
}

pub fn check_credential_helper() -> Option<String> {
    match git_config_get("credential.helper", "effective") {
        Some(helper) if helper.contains("cache") => Some(format!(
            "  {} credential.helper is set to '{}'. Tokens may not persist.",
            "⚠".yellow(),
            helper
        )),
        None => Some(format!(
            "  {} No credential.helper set. Git may not store your tokens.",
            "⚠".yellow()
        )),
        _ => None,
    }
}

pub fn git_credential_approve(username: &str, token: &str, host: &str, url: Option<&str>) {
    use std::io::Write;
    let input = if let Some(u) = url {
        format!("url={u}\nusername={username}\npassword={token}\n\n")
    } else {
        format!("protocol=https\nhost={host}\nusername={username}\npassword={token}\n\n")
    };
    let mut child = Command::new("git")
        .args(["credential", "approve"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute git credential approve");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).ok();
    }
    let status = child
        .wait()
        .expect("Failed to wait for git credential approve");
    if !status.success() {
        eprintln!("{} Failed to approve git credential", "error:".red().bold());
    }
}

pub fn git_credential_reject(host: &str) {
    use std::io::Write;
    let input = format!("protocol=https\nhost={host}\n\n");
    let mut child = Command::new("git")
        .args(["credential", "reject"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute git credential reject");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).ok();
    }
    let _ = child.wait();
}

pub fn format_account_label(account: &Account) -> String {
    match &account.alias {
        Some(alias) => format!("{}:{} <{}>", account.username, alias, account.email),
        None => format!("{} <{}>", account.username, account.email),
    }
}

/// Resolve an account by identifier (username or alias), or show interactive selection.
pub fn resolve_account(config: &Config, identifier: Option<String>, prompt: &str) -> Account {
    if config.accounts.is_empty() {
        println!("\n  {}\n", "No accounts configured.".dimmed());
        println!("  Run {} to add one.\n", "gitas add".cyan().bold());
        std::process::exit(1);
    }

    match identifier {
        Some(id) => {
            let found = config.accounts.iter().find(|a| {
                a.username == id
                    || a.alias.as_deref() == Some(&id)
                    || a.alias
                        .as_ref()
                        .is_some_and(|alias| id == format!("{}:{}", a.username, alias))
            });
            match found {
                Some(a) => a.clone(),
                None => {
                    eprintln!(
                        "\n  {} No account matching '{}'.\n",
                        "\u{2717}".red().bold(),
                        id.yellow()
                    );
                    std::process::exit(1);
                }
            }
        }
        None => {
            let labels: Vec<String> = config.accounts.iter().map(format_account_label).collect();

            enter_raw_mode();
            let selection = raw_select(prompt, &labels, 0);
            exit_raw_mode();

            match selection {
                Some(index) => config.accounts[index].clone(),
                None => {
                    std::process::exit(0);
                }
            }
        }
    }
}
