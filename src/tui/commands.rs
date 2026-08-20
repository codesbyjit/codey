use crate::agent::subagent;
use crate::mcp;
use crate::tui::App;

pub fn handle_command(app: &mut App, raw: &str) {
    let mut parts = raw.split_whitespace();
    let command = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();

    match command {
        "/help" => {
            app.show_help = !app.show_help;
        }

        "/new" => {
            let n = app.sessions.new_session() + 1;
            let _ = app.sessions.save_all();
            app.entries.clear();
            app.push_system(format!("Started session {n}."));
        }

        "/sessions" => {
            let current = app.sessions.current_index();
            let mut out = String::from("Sessions:\n");
            for (i, session) in app.sessions.sessions().iter().enumerate() {
                let marker = if i == current { "*" } else { " " };
                out.push_str(&format!(
                    "  {marker} {}  {}  ({})\n",
                    i + 1,
                    session.title(),
                    session.model()
                ));
            }
            app.push_system(out);
        }

        "/session" => {
            if let Some(arg) = rest.first() {
                if let Ok(num) = arg.parse::<usize>() {
                    if num == 0 {
                        app.push_system("Session numbers start at 1.".to_string());
                    } else if app.sessions.switch_to(num - 1) {
                        let _ = app.sessions.save_all();
                        app.entries.clear();
                        app.push_system(format!("Switched to session {num}."));
                    } else {
                        app.push_system(format!("Session {num} does not exist."));
                    }
                } else {
                    app.push_system("Usage: /session <number>".to_string());
                }
            } else {
                app.push_system("Usage: /session <number>".to_string());
            }
        }

        "/prev" => {
            if app.sessions.previous() {
                let _ = app.sessions.save_all();
                app.entries.clear();
                app.push_system(format!(
                    "Switched to session {}.",
                    app.sessions.current_index() + 1
                ));
            } else {
                app.push_system("Already at the first session.".to_string());
            }
        }

        "/next" => {
            if app.sessions.next() {
                let _ = app.sessions.save_all();
                app.entries.clear();
                app.push_system(format!(
                    "Switched to session {}.",
                    app.sessions.current_index() + 1
                ));
            } else {
                app.push_system("Already at the latest session.".to_string());
            }
        }

        "/model" => {
            if let Some(name) = rest.first() {
                app.sessions.current_mut().set_model((*name).to_string());
                let _ = app.sessions.current().save();
                app.push_system(format!("Model set to `{name}`."));
            } else {
                app.push_system(format!("Current model: {}", app.sessions.current().model()));
            }
        }

        "/context" => {
            let session = app.sessions.current();
            let ctx = session.context();
            let tokens = ctx.estimated_tokens();
            let budget = ctx.budget();
            let percent = if budget == 0 {
                0.0
            } else {
                (tokens as f64 / budget as f64) * 100.0
            };
            app.push_system(format!(
                "Session: {}\nMessages: {}\nEstimated tokens: ~{tokens}\nBudget: {budget}\nUsage: {percent:.1}%",
                app.sessions.current_index() + 1,
                ctx.len()
            ));
        }

        "/clear" => {
            app.sessions.current_mut().clear();
            app.entries.clear();
            let _ = app.sessions.current().save();
            app.push_system("Context cleared.".to_string());
        }

        "/skills" => {
            if app.skills.is_empty() {
                app.push_system("No skills discovered.".to_string());
            } else {
                let mut out = String::from("Available skills:\n");
                for skill in &app.skills {
                    out.push_str(&format!("  • {} — {}\n", skill.name, skill.description));
                }
                app.push_system(out);
            }
        }

        "/agents" => {
            let mut out = String::from("Subagents (use delegate_subagent tool):\n");
            for agent in subagent::builtin_subagents() {
                out.push_str(&format!("  • {} — {}\n", agent.name, agent.purpose));
            }
            app.push_system(out);
        }

        "/tools" => {
            let mut out = String::from("Available tools:\n");
            for def in app.registry.definitions() {
                out.push_str(&format!("  • {} — {}\n", def.name, def.description));
            }
            out.push_str("  • delegate_subagent — delegate to a subagent\n");
            app.push_system(out);
        }

        "/mcp" => match mcp::load_config() {
            Some(config) if !config.servers.is_empty() => {
                let mut out = String::from("MCP servers:\n");
                for server in &config.servers {
                    out.push_str(&format!(
                        "  • {} ({})\n",
                        server.name,
                        server.transport.transport_name()
                    ));
                }
                out.push_str("\nNote: MCP tool transport is a scaffold in this build.");
                app.push_system(out);
            }
            _ => app.push_system("No MCP servers configured.".to_string()),
        },

        "/quit" | "/exit" => {
            app.should_quit = true;
        }

        "/cancel" => {
            if app.running {
                crate::tui::cancel_run(app);
                app.push_system("Cancelled.".to_string());
            }
        }

        other => {
            app.push_system(format!(
                "Unknown command: {other}\nType /help to see available commands."
            ));
        }
    }
}

trait TransportName {
    fn transport_name(&self) -> String;
}
impl TransportName for mcp::McpTransport {
    fn transport_name(&self) -> String {
        match self {
            mcp::McpTransport::Stdio { .. } => "stdio".to_string(),
        }
    }
}

#[allow(dead_code)]
pub fn command_names() -> &'static [&'static str] {
    &[
        "/help",
        "/new",
        "/sessions",
        "/session",
        "/prev",
        "/next",
        "/model",
        "/context",
        "/clear",
        "/skills",
        "/agents",
        "/tools",
        "/mcp",
        "/quit",
    ]
}
