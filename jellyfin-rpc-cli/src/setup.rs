use crate::config::{Config, DiscordBuilder, Username};
use colored::Colorize;
use std::io::{self, Write};

/// Interactive first-run setup. Writes a fresh config to `path`, creating
/// parent directories as needed. Called either because no config exists yet
/// or because the user passed `--configure`.
pub fn run(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "Welcome to Jellyfin-RPC!".cyan().bold());
    println!("Let's get you set up. This needs a few things from your Jellyfin server");
    println!("so it can figure out what you're watching and show it on Discord.\n");

    println!("{}", "Server details".cyan().bold());
    let (url, api_key, username) = loop {
        let url = prompt_required("Jellyfin URL (e.g. https://jellyfin.example.com):");
        let api_key = prompt_required(
            "Jellyfin API key\n  Get one from: Jellyfin dashboard > Advanced > API Keys > + (Add)\nAPI key:",
        );
        let username = prompt_required(
            "Jellyfin username (the account whose watch status should be shown):",
        );

        print!("Checking that against your server... ");
        io::stdout().flush().ok();

        match validate(&url, &api_key, &username) {
            Ok(()) => {
                println!("{}", "looks good.".green());
                break (url, api_key, username);
            }
            Err(reason) => {
                println!("{}", "failed.".red());
                eprintln!("  {}", reason.red());
                if prompt_yes_no("Save it anyway and sort it out later?", false) {
                    break (url, api_key, username);
                }
                println!("\n{}\n", "Let's try again.".yellow());
            }
        }
    };

    println!("\n{}", "Discord (optional)".cyan().bold());
    let discord_app_id = prompt_optional(
        "Custom Discord Application ID, only needed if you want your own bot identity\n\
         (leave blank to use Jellyfin-RPC's default app):",
    );

    let mut builder = Config::builder();
    builder.jellyfin.url = url;
    builder.jellyfin.api_key = api_key;
    builder.jellyfin.username = Username::String(username);
    if let Some(application_id) = discord_app_id {
        builder.discord = Some(DiscordBuilder {
            application_id: Some(application_id),
            buttons: None,
            show_paused: None,
        });
    }

    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&builder)?)?;

    println!("\nConfig saved to {}", path.green());
    println!("Run with {} any time you want to redo this.\n", "--configure".bold());

    Ok(())
}

/// Hits the server the same way the actual client does (Users endpoint, same
/// auth headers as `jellyfin_rpc::Client`) so a bad URL/key/username gets caught
/// now instead of as a cryptic error mid-poll-loop later.
fn validate(url: &str, api_key: &str, username: &str) -> Result<(), String> {
    let users_url = format!("{}/Users", url.trim_end_matches('/'));

    let response = reqwest::blocking::Client::new()
        .get(&users_url)
        .header("X-Emby-Token", api_key)
        .header("Authorization", format!("MediaBrowser Token=\"{api_key}\""))
        .send()
        .map_err(|err| format!("Couldn't reach {url}: {err}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Server responded with {} — double check the URL and API key",
            response.status()
        ));
    }

    let users: Vec<serde_json::Value> = response
        .json()
        .map_err(|err| format!("Got a response but couldn't parse it as Jellyfin's /Users: {err}"))?;

    let known = users
        .iter()
        .any(|user| user.get("Name").and_then(|n| n.as_str()) == Some(username));

    if !known {
        return Err(format!("No user named '{username}' found on that server"));
    }

    Ok(())
}

fn prompt(question: &str) -> String {
    print!("{question} ");
    io::stdout().flush().ok();

    let mut input = String::new();
    // Ok(0) means stdin closed (e.g. piped/non-interactive invocation) — without
    // this check a closed stdin makes prompt_required loop forever asking for input.
    match io::stdin().read_line(&mut input) {
        Ok(0) => {
            eprintln!("\n{}", "No input available, aborting setup.".red());
            std::process::exit(1);
        }
        _ => input.trim().to_string(),
    }
}

fn prompt_required(question: &str) -> String {
    loop {
        let value = prompt(question);
        if !value.is_empty() {
            return value;
        }
        println!("{}", "This one's required, try again.".yellow());
    }
}

fn prompt_optional(question: &str) -> Option<String> {
    let value = prompt(question);
    (!value.is_empty()).then_some(value)
}

fn prompt_yes_no(question: &str, default_yes: bool) -> bool {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    match prompt(&format!("{question} {suffix}")).to_lowercase().as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        _ => false,
    }
}
