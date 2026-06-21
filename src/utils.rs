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
    let args: &[&str] = match scope {
        "local" => &["config", "--local", "--get", key],
        "global" => &["config", "--global", "--get", key],
        _ => &["config", "--get", key], // effective (local > global)
    };
    let output = Command::new("git").args(args).output().ok()?;
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

pub struct Remote {
    pub name: String,
    pub url: String,
}

pub fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

pub fn get_http_remotes() -> Vec<Remote> {
    let Ok(output) = Command::new("git")
        .args(["config", "--get-regexp", r"remote\..*\.url"])
        .output()
    else {
        return Vec::new();
    };

    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let key = parts.next()?;
            let url = parts.next()?;

            if key.starts_with("remote.") && key.ends_with(".url") && is_http_url(url) {
                let name = key
                    .trim_start_matches("remote.")
                    .trim_end_matches(".url")
                    .to_string();
                Some(Remote {
                    name,
                    url: url.to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn has_http_remotes() -> bool {
    !get_http_remotes().is_empty()
}

pub fn git_args_use_http_transport(args: &[String]) -> bool {
    args.iter().any(|arg| is_http_url(arg))
        || (git_args_may_use_configured_remote(args) && has_http_remotes())
}

pub fn git_ssh_command(ssh_key: &str) -> String {
    let normalized_path = ssh_key.replace('\\', "/");
    format!("ssh -i \"{}\" -o IdentitiesOnly=yes", normalized_path)
}

fn git_args_may_use_configured_remote(args: &[String]) -> bool {
    let Some(command) = git_subcommand(args) else {
        return false;
    };

    matches!(
        command,
        "archive" | "fetch" | "ls-remote" | "pull" | "push" | "remote" | "submodule"
    )
}

fn git_subcommand(args: &[String]) -> Option<&str> {
    let mut iter = args.iter().map(String::as_str);

    while let Some(arg) = iter.next() {
        if matches!(
            arg,
            "-C" | "-c" | "--exec-path" | "--git-dir" | "--work-tree"
        ) {
            iter.next();
            continue;
        }

        if matches!(
            arg,
            "--bare" | "--no-pager" | "--paginate" | "--literal-pathspecs"
        ) {
            continue;
        }

        if arg.starts_with("--exec-path=")
            || arg.starts_with("--git-dir=")
            || arg.starts_with("--work-tree=")
            || arg.starts_with("-c")
        {
            continue;
        }

        if arg.starts_with('-') {
            continue;
        }

        return Some(arg);
    }

    None
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

pub fn git_credential_approve(
    username: &str,
    token: &str,
    host: &str,
    url: Option<&str>,
) -> Result<(), String> {
    use std::io::Write;
    let input = if let Some(u) = url {
        format!("url={u}\nusername={username}\npassword={token}\n\n")
    } else {
        format!("protocol=https\nhost={host}\nusername={username}\npassword={token}\n\n")
    };
    let mut child = Command::new("git")
        .args(["credential", "approve"])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to execute git credential approve");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).ok();
    }
    let output = child
        .wait_with_output()
        .expect("Failed to wait for git credential approve");
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to approve git credential: {}", err.trim()))
    } else {
        Ok(())
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

pub fn scan_ssh_keys(
    target_username: &str,
    target_email: &str,
) -> (Vec<String>, Vec<std::path::PathBuf>, usize) {
    let mut display_items = Vec::new();
    let mut paths = Vec::new();
    let mut default_idx = 0;

    let Some(entries) = dirs::home_dir()
        .map(|h| h.join(".ssh"))
        .and_then(|d| std::fs::read_dir(d).ok())
    else {
        display_items.push("Enter ssh key path manually".to_string());
        return (display_items, paths, 0);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if filename.ends_with(".pub")
            || filename.ends_with(".old")
            || filename.ends_with(".bak")
            || filename == "config"
            || filename == "known_hosts"
            || filename == "authorized_keys"
        {
            continue;
        }

        let is_private_key = std::fs::File::open(&path)
            .and_then(|mut f| {
                let mut buffer = [0; 128];
                use std::io::Read;
                let n = f.read(&mut buffer)?;
                Ok(String::from_utf8_lossy(&buffer[..n]).starts_with("-----BEGIN"))
            })
            .unwrap_or(false);

        if !is_private_key {
            continue;
        }

        let pub_path = path.with_extension("pub");
        let comment = std::fs::read_to_string(&pub_path).ok().and_then(|content| {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 3 {
                Some(parts[2..].join(" "))
            } else {
                None
            }
        });

        let display_name = match &comment {
            Some(c) => format!("{} ({})", filename, c),
            None => filename.clone(),
        };

        let matches = comment.as_ref().is_some_and(|c| {
            let c_lower = c.to_lowercase();
            let u_lower = target_username.to_lowercase();
            let e_lower = target_email.to_lowercase();

            c_lower == e_lower
                || c_lower == u_lower
                || c_lower
                    .find('@')
                    .is_some_and(|idx| c_lower[..idx] == u_lower)
        });

        if matches {
            default_idx = paths.len();
        }

        display_items.push(display_name);
        paths.push(path);
    }

    display_items.push("Enter ssh key path manually".to_string());
    if default_idx >= display_items.len() {
        default_idx = display_items.len() - 1;
    }
    (display_items, paths, default_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_http_urls() {
        assert!(is_http_url("https://github.com/owner/repo.git"));
        assert!(is_http_url("http://github.com/owner/repo.git"));
        assert!(!is_http_url("git@github.com:owner/repo.git"));
        assert!(!is_http_url("ssh://git@github.com/owner/repo.git"));
    }

    #[test]
    fn finds_subcommand_after_global_options() {
        let args = vec![
            "-C".to_string(),
            "repo".to_string(),
            "-c".to_string(),
            "core.askpass=true".to_string(),
            "fetch".to_string(),
        ];

        assert_eq!(git_subcommand(&args), Some("fetch"));
    }

    #[test]
    fn formats_gitas_ssh_command() {
        assert_eq!(
            git_ssh_command(r"C:\Users\me\.ssh\id_ed25519"),
            r#"ssh -i "C:/Users/me/.ssh/id_ed25519" -o IdentitiesOnly=yes"#
        );
    }
}
