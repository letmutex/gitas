mod commands;
mod github;
mod models;
mod tui;
mod utils;

use clap::{Parser, Subcommand};
use models::load_config;

#[derive(Parser)]
#[command(
    name = "gitas",
    about = "GitHub Account Switch — manage multiple git identities",
    version
)]
struct Cli {
    /// Account username or alias (skip interactive selection for git)
    #[arg(short = 'a', long, global = true)]
    account: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a new git account
    Add,
    /// Run any git command as a specific account
    #[command(trailing_var_arg = true)]
    Git {
        /// Arguments passed to git (e.g. clone, push, pull ...)
        #[arg(allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() {
    utils::check_git_installed();
    let cli = Cli::parse();
    let mut config = load_config();

    match cli.command {
        None => commands::list::run(&mut config),
        Some(Commands::Add) => commands::add::run(&mut config),
        Some(Commands::Git { args }) => commands::git::run(&config, cli.account, args),
    }
}
