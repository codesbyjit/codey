use std::io::{self, Write};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use tokio::sync::mpsc;

use codey::{
    agent::{instructions, skills, subagent, AgentEvent, Session},
    config::ConfirmationMode,
    provider::create_provider,
    tools::builtin_registry,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("setup") => {
            setup()?;
            return Ok(());
        }
        Some("--help") | Some("-h") | Some("help") => {
            print_help();
            return Ok(());
        }
        Some("--version") | Some("-v") => {
            println!("codey {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(other) if other.starts_with('-') => {
            eprintln!("Unknown flag: {other}");
            print_help();
            return Ok(());
        }
        _ => {}
    }

    if args.is_empty() {
        codey::tui::run().await?;
        return Ok(());
    }

    let prompt = args.join(" ");
    run_headless(&prompt).await?;

    Ok(())
}

async fn run_headless(prompt: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = codey::config::Config::load()?;
    let provider = create_provider(&config)?;
    let registry = builtin_registry();

    let workspace = config.workspace_path();
    let instructions = instructions::discover_instructions(&workspace);
    let discovered = skills::discover_skills(&workspace);
    let summaries = skills::summaries(&discovered);
    let selected = skills::select_for_task(&discovered, prompt);

    let mut tool_defs = registry.definitions();
    tool_defs.push(subagent::delegate_definition());

    let mut system_prompt =
        codey::agent::prompt::build_system_prompt(&tool_defs, &instructions, &summaries);
    for skill in &selected {
        system_prompt.push_str(&format!(
            "\n\nSKILL: {}\n{}",
            skill.name, skill.instructions
        ));
    }

    let mut session = Session::new();
    session.add_user_message(prompt);

    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

    let printer = tokio::spawn(async move {
        let mut pending = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::AssistantText(t) => {
                    pending.push_str(&t);
                }
                AgentEvent::ClearAssistant => {
                    pending.clear();
                }
                AgentEvent::ToolCall { name, .. } => {
                    pending.clear();
                    println!("\n[tool] {name}");
                }
                AgentEvent::ToolResult {
                    output, is_error, ..
                } => {
                    if is_error {
                        eprintln!("[tool error] {output}");
                    } else {
                        eprintln!("[tool result]\n{output}");
                    }
                }
                AgentEvent::Finished(text) => {
                    let clean = codey::provider::clean_answer(&text);
                    if !clean.is_empty() {
                        println!("{clean}");
                    }
                }
                AgentEvent::Error(e) => {
                    eprintln!("\nerror: {e}");
                }
                AgentEvent::Status(s) => {
                    eprintln!("[{s}]");
                }
                _ => {}
            }
        }
    });

    let result = codey::agent::run_agent(
        &mut session,
        &provider,
        &registry,
        &system_prompt,
        &tx,
        config.confirmation_mode,
        true,
        0,
        Arc::new(AtomicUsize::new(0)),
        None,
    )
    .await;

    drop(tx);
    let _ = printer.await;

    match result {
        Ok(_answer) => {
            let _ = session.save();
        }
        Err(e) => {
            eprintln!("\nCodey failed: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn setup() -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("Codey Setup");
    println!("───────────");
    println!();

    let provider = ask("Provider [openrouter]", "openrouter")?;
    let api_key = ask("API key", "")?;
    if api_key.is_empty() {
        return Err("API key cannot be empty.".into());
    }
    let model = ask("Model [openrouter/free]", "openrouter/free")?;
    let model = if model.is_empty() {
        "openrouter/free".to_string()
    } else {
        model
    };
    let base_url = ask("Base URL (blank for default)", "")?;
    let base_url = if base_url.is_empty() {
        codey::config::default_base_url_for(&provider).to_string()
    } else {
        base_url
    };
    let confirmation = ask("Confirmation mode [dangerous]", "dangerous")?;
    let confirmation_mode = match confirmation.to_lowercase().as_str() {
        "always" => ConfirmationMode::Always,
        "never" => ConfirmationMode::Never,
        _ => ConfirmationMode::Dangerous,
    };

    let config = codey::config::Config {
        provider,
        api_key,
        model,
        base_url,
        context_window: codey::config::DEFAULT_CONTEXT_WINDOW,
        workspace: None,
        confirmation_mode,
    };

    config.save()?;

    println!();
    println!(
        "✓ Configuration saved to {}",
        codey::config::config_display_path()
    );
    println!();
    println!("Codey is ready. Run `codey` to start, or `codey \"your task\"`.");
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
    Ok(if input.is_empty() {
        default.to_string()
    } else {
        input.to_string()
    })
}

fn print_help() {
    println!("Codey - a fast terminal coding agent");
    println!();
    println!("Usage:");
    println!("  codey               Launch the interactive TUI");
    println!("  codey \"your task\"    Run a single task and exit");
    println!("  codey setup         Configure providers and API keys");
    println!("  codey --help        Show this help");
    println!("  codey --version     Show version");
}
