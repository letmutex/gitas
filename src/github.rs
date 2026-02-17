use colored::Colorize;
use serde::Deserialize;
use std::thread;
use std::time::Duration;

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    // expires_in: u64, // unused but part of response
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct UserResponse {
    login: String,
    email: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct EmailResponse {
    email: String,
    primary: bool,
}

pub fn login() -> Option<(String, String, Option<String>, String)> {
    let client_id = "Ov23likbcGeD5f41YHUr";

    let config = ureq::config::Config::builder()
        .user_agent("gitas-cli")
        .http_status_as_error(false)
        .build();
    let agent = ureq::Agent::new_with_config(config);

    // Step 1: Request device code
    let res = agent
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .send_form([
            ("client_id", client_id),
            ("scope", "read:user user:email repo workflow"),
        ]);

    let device_res: DeviceCodeResponse = match res {
        Ok(mut r) if r.status().is_success() => match r.body_mut().read_json() {
            Ok(json) => json,
            Err(_) => {
                println!("  {}", "Failed to parse GitHub response.".red());
                return None;
            }
        },
        _ => {
            println!("  {}", "Failed to connect to GitHub.".red());
            return None;
        }
    };

    println!();
    println!(
        "  Please visit: {}",
        device_res.verification_uri.cyan().bold()
    );
    println!("  And enter code: {}", device_res.user_code.green().bold());
    println!();

    // Give user a moment to see the code before opening the browser
    thread::sleep(Duration::from_secs(1));

    if open::that(&device_res.verification_uri).is_err() {
        println!("  (Failed to open browser automatically)");
    }

    // Step 2: Poll for token
    println!("  Waiting for authentication...");
    let interval = Duration::from_secs(device_res.interval + 1);

    loop {
        thread::sleep(interval);

        let token_res = agent
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .send_form([
                ("client_id", client_id),
                ("device_code", device_res.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ]);

        let json_res: Option<TokenResponse> = match token_res {
            Ok(mut r) => r.body_mut().read_json().ok(),
            Err(_) => None,
        };

        if let Some(json) = json_res {
            if let Some(token) = json.access_token {
                // Success! Fetch user info
                let user_res = agent
                    .get("https://api.github.com/user")
                    .header("Authorization", format!("Bearer {}", token))
                    .call();

                if let Ok(mut ur) = user_res
                    && ur.status().is_success()
                    && let Ok(user) = ur.body_mut().read_json::<UserResponse>()
                {
                    // Always fetch emails to find the noreply one
                    let emails_res = agent
                        .get("https://api.github.com/user/emails")
                        .header("Authorization", format!("Bearer {}", token))
                        .call();

                    let email = if let Ok(mut er) = emails_res
                        && er.status().is_success()
                        && let Ok(emails) = er.body_mut().read_json::<Vec<EmailResponse>>()
                    {
                        // 1. Try to find a noreply address
                        // 2. Fallback to primary address
                        // 3. Fallback to the first one found
                        emails
                            .iter()
                            .find(|e| e.email.contains("noreply.github.com"))
                            .or_else(|| emails.iter().find(|e| e.primary))
                            .or_else(|| emails.first())
                            .map(|e| e.email.clone())
                            .unwrap_or_else(|| user.email.unwrap_or_default())
                    } else {
                        user.email.unwrap_or_default()
                    };

                    return Some((user.login, email, user.name, token));
                }
                println!("  {}", "Failed to fetch user info.".red());
                return None;
            }
            if let Some(error) = json.error
                && error != "authorization_pending"
                && error != "slow_down"
            {
                println!("  Error: {}", error.red());
                return None;
            }
        }
    }
}
