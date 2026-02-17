use colored::Colorize;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SERVICE_NAME: &str = "gitas";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub username: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub accounts: Vec<Account>,
}

fn config_path() -> PathBuf {
    let config_dir = dirs::config_dir()
        .expect("Could not determine config directory")
        .join("gitas");
    fs::create_dir_all(&config_dir).expect("Could not create config directory");
    config_dir.join("accounts.json")
}

pub fn load_config() -> Config {
    let path = config_path();
    if path.exists() {
        let data = fs::read_to_string(&path).expect("Could not read config file");
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        Config::default()
    }
}

pub fn save_config(config: &Config) {
    let path = config_path();
    let data = serde_json::to_string_pretty(config).expect("Could not serialize config");
    fs::write(&path, data).expect("Could not write config file");
}

/// Helper to construct the keychain entry key
fn make_key(username: &str, alias: Option<&str>) -> String {
    match alias {
        Some(a) => format!("{}::{}", username, a),
        None => username.to_string(),
    }
}

/// Securely store a token in the system keychain
pub fn set_token(username: &str, alias: Option<&str>, token: &str) {
    let key = make_key(username, alias);
    match Entry::new(SERVICE_NAME, &key) {
        Ok(entry) => {
            if let Err(e) = entry.set_password(token) {
                eprintln!("  {} Failed to store token in keychain: {}", "✗".red(), e);
            }
        }
        Err(e) => eprintln!("  {} Failed to create keychain entry: {}", "✗".red(), e),
    }
}

/// Retrieve a token from the system keychain
pub fn get_token(username: &str, alias: Option<&str>) -> Option<String> {
    let key = make_key(username, alias);
    match Entry::new(SERVICE_NAME, &key) {
        Ok(entry) => match entry.get_password() {
            Ok(password) => Some(password),
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                eprintln!(
                    "  {} Failed to retrieve token from keychain: {}",
                    "✗".red(),
                    e
                );
                None
            }
        },
        Err(e) => {
            eprintln!("  {} Failed to access keychain: {}", "✗".red(), e);
            None
        }
    }
}

/// Delete a token from the system keychain
pub fn delete_token(username: &str, alias: Option<&str>) {
    let key = make_key(username, alias);
    if let Ok(entry) = Entry::new(SERVICE_NAME, &key) {
        let _ = entry.delete_credential();
    }
}
