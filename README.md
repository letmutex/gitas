# gitas

Git Account Switch / Git As

## Installation

```bash
cargo install gitas
```

## Usage

```bash
# Open interactive TUI to switch, edit, or remove accounts
gitas

# Add a new account (Manual or GitHub Login)
gitas add

# Run any git command as a specific account
gitas git clone <url>
```

## Data

- **Config**: [`dirs::config_dir()`](https://docs.rs/dirs/latest/dirs/fn.config_dir.html)/`gitas/accounts.json`
- **Secrets**: System Keychain
