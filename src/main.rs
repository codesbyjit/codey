mod agent;
mod tools;
mod tui;

use std::env;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("setup") => {
            setup()?;
            return Ok(());
        }

        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }

        _ => {}
    }

    // No prompt = interactive TUI.
    if args.is_empty() {
        tui::run().await?;
        return Ok(());
    }

    // Prompt supplied directly.
    let prompt = args.join(" ");

    let result = agent::run(&prompt).await?;

    println!("\nCodey: {result}");

    Ok(())
}

fn setup() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("Codey Setup");
    println!("───────────");
    println!();

    let provider = ask("Provider", "openrouter")?;

    let api_key = ask("API key", "")?;

    if api_key.is_empty() {
        return Err("API key cannot be empty.".into());
    }

    let model = ask("Model", agent::config::default_model())?;

    let model = if model.is_empty() {
        agent::config::default_model().to_string()
    } else {
        model
    };

    let config = agent::config::Config {
        provider,
        api_key,
        model,
    };

    config.save()?;

    println!();
    println!("✓ Configuration saved.");
    println!();
    println!("Codey is ready.");
    println!();

    Ok(())
}

fn ask(name: &str, default: &str) -> Result<String, Box<dyn std::error::Error>> {
    if default.is_empty() {
        print!("{name}: ");
    } else {
        print!("{name} [{default}]: ");
    }

    io::stdout().flush()?;

    let mut input = String::new();

    io::stdin().read_line(&mut input)?;

    let input = input.trim();

    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input.to_string())
    }
}

fn print_help() {
    println!("Codey - terminal coding agent");
    println!();
    println!("Usage:");
    println!("  codey");
    println!("  codey \"your task\"");
    println!("  codey setup");
    println!("  codey help");
}
