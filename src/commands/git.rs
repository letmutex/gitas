use crate::models::Config;
use crate::utils::resolve_account;
use colored::Colorize;
use std::process::Command;

pub fn run(config: &Config, account_id: Option<String>, args: Vec<String>) {
    if args.is_empty() {
        eprintln!(
            "\n  {} No git command provided. Usage: {}\n",
            "✗".red().bold(),
            "gitas git <args...>".cyan()
        );
        std::process::exit(1);
    }

    let account = resolve_account(config, account_id, "  Run as");

    // Build: git -c user.name=X -c user.email=Y <args...>
    let mut cmd = Command::new("git");
    cmd.arg("-c").arg(format!("user.name={}", account.username));
    cmd.arg("-c").arg(format!("user.email={}", account.email));

    // Inject inline credential helper if token is available
    match crate::models::get_token(&account.username, account.alias.as_deref()) {
        Some(token) if !token.is_empty() => {
            cmd.arg("-c").arg("credential.helper=");
            cmd.arg("-c").arg(format!(
                "credential.helper=!f() {{ echo \"username={}\"; echo \"password={}\"; }}; f",
                account.username, token
            ));
        }
        _ => {
            println!(
                "  {} No token found for {}. Git may prompt for authentication.",
                "⚠".yellow(),
                account.username.cyan()
            );
        }
    }

    cmd.args(&args);

    println!(
        "  {} git {} {}",
        "\u{21b7}".dimmed(),
        args.join(" "),
        format!("as {} <{}>", account.username, account.email).dimmed(),
    );
    println!();

    let status = cmd.status().expect("Failed to execute git");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
