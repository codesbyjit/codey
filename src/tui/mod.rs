pub mod commands;
pub mod input;
pub mod ui;

use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::agent::skills::{self, Skill};
use crate::agent::subagent;
use crate::agent::{self, AgentEvent, Session};
use crate::config::{Config, ConfirmationMode};
use crate::mcp;
use crate::provider::Provider;
use crate::tools::{builtin_registry, ToolRegistry};

#[derive(Debug, Clone)]
pub enum Entry {
    User(String),
    Assistant(String),
    Tool {
        name: String,
        summary: String,
        output: String,
        is_error: bool,
    },
    System(String),
    Error(String),
}

pub struct PermissionPrompt {
    pub description: String,
    pub responder: tokio::sync::oneshot::Sender<bool>,
}

pub struct App {
    pub sessions: agent::SessionManager,
    pub provider: Arc<dyn Provider>,
    pub registry: Arc<ToolRegistry>,
    pub workspace: PathBuf,
    pub confirmation_mode: ConfirmationMode,
    pub skills: Vec<Skill>,
    pub instructions: String,

    pub entries: Vec<Entry>,
    pub input: String,
    pub cursor: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub scroll: u16,
    pub auto_scroll: bool,

    pub running: bool,
    pub should_quit: bool,
    pub status: String,
    pub show_help: bool,

    pub event_tx: UnboundedSender<AgentEvent>,
    pub event_rx: UnboundedReceiver<AgentEvent>,
    pub result_rx: Option<UnboundedReceiver<Session>>,
    pub agent_task: Option<JoinHandle<()>>,
    pub permission: Option<PermissionPrompt>,

    pub active_subagents: Arc<AtomicUsize>,
    pub last_tool_index: Option<usize>,
}

impl App {
    fn new(
        provider: Arc<dyn Provider>,
        registry: Arc<ToolRegistry>,
        confirmation_mode: ConfirmationMode,
        workspace: PathBuf,
    ) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let skills = skills::discover_skills(&workspace);
        let instructions = agent::instructions::discover_instructions(&workspace);

        let mut app = Self {
            sessions: agent::SessionManager::load_all(),
            provider,
            registry,
            workspace: workspace.clone(),
            confirmation_mode,
            skills: skills.clone(),
            instructions: instructions.clone(),
            entries: Vec::new(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            scroll: 0,
            auto_scroll: true,
            running: false,
            should_quit: false,
            status: "ready".into(),
            show_help: false,
            event_tx,
            event_rx,
            result_rx: None,
            agent_task: None,
            permission: None,
            active_subagents: Arc::new(AtomicUsize::new(0)),
            last_tool_index: None,
        };
        app.push_system(format!(
            "Welcome to Codey. Type /help for commands. Workspace: {}",
            workspace.display()
        ));
        app
    }

    pub fn push_system(&mut self, content: impl Into<String>) {
        self.entries.push(Entry::System(content.into()));
        self.scroll_to_bottom();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
    }
}

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load().or_else(|_| -> anyhow::Result<Config> {
        anyhow::bail!("Codey is not configured. Run `codey setup` first.");
    })?;

    let provider = crate::provider::create_provider(&config)?;
    let mut registry = builtin_registry();
    let mcp_count = mcp::register_mcp_tools(&mut registry);
    let registry = Arc::new(registry);

    let workspace = config.workspace_path();

    let mut app = App::new(provider, registry, config.confirmation_mode, workspace);
    if mcp_count > 0 {
        app.push_system(format!("Loaded {mcp_count} MCP server definition(s)."));
    }

    let mut terminal = setup_terminal()?;
    let result = app_loop(&mut terminal, &mut app).await;
    restore_terminal(&mut terminal)?;

    let _ = app.sessions.save_all();
    result
}

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn app_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if app.should_quit {
            break;
        }

        while let Ok(event) = app.event_rx.try_recv() {
            handle_event(app, event);
        }

        if let Some(rx) = app.result_rx.as_mut() {
            while let Ok(session) = rx.try_recv() {
                app.sessions.replace_current(session);
                let _ = app.sessions.current().save();
                app.running = false;
                app.status = "ready".into();
            }
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                let submit = input::handle_key(app, key);
                if submit && !app.running && app.permission.is_none() {
                    let input = app.input.clone();
                    submit_input(app, &input).await;
                }
            }
        }
    }
    Ok(())
}

fn handle_event(app: &mut App, event: AgentEvent) {
    match event {
        AgentEvent::Status(s) => {
            app.status = s;
        }
        AgentEvent::UserMessage(s) => {
            app.entries.push(Entry::User(s));
            app.scroll_to_bottom();
        }
        AgentEvent::AssistantText(t) => {
            if let Some(Entry::Assistant(existing)) = app.entries.last_mut() {
                existing.push_str(&t);
            } else {
                app.entries.push(Entry::Assistant(t));
            }
            app.scroll_to_bottom();
        }
        AgentEvent::ClearAssistant => {
            if let Some(Entry::Assistant(_)) = app.entries.last() {
                app.entries.pop();
                app.scroll_to_bottom();
            }
        }
        AgentEvent::ToolCall { name, summary } => {
            app.entries.push(Entry::Tool {
                name,
                summary,
                output: String::new(),
                is_error: false,
            });
            app.last_tool_index = Some(app.entries.len() - 1);
            app.scroll_to_bottom();
        }
        AgentEvent::ToolResult {
            name,
            output,
            is_error,
        } => {
            if let Some(index) = app.last_tool_index.take() {
                if let Some(Entry::Tool { name: n, .. }) = app.entries.get(index) {
                    let n = n.clone();
                    app.entries[index] = Entry::Tool {
                        name: n,
                        summary: String::new(),
                        output,
                        is_error,
                    };
                }
            } else {
                app.entries.push(Entry::Tool {
                    name,
                    summary: String::new(),
                    output,
                    is_error,
                });
            }
            app.scroll_to_bottom();
        }
        AgentEvent::Error(s) => {
            app.entries.push(Entry::Error(s));
            app.status = "error".into();
            app.scroll_to_bottom();
        }
        AgentEvent::Finished(_) => {
            app.status = "ready".into();
        }
        AgentEvent::Cancelled => {
            app.entries.push(Entry::System("Run cancelled.".into()));
            app.status = "ready".into();
            app.running = false;
            app.scroll_to_bottom();
        }
        AgentEvent::PermissionRequest {
            description,
            responder,
        } => {
            app.permission = Some(PermissionPrompt {
                description,
                responder,
            });
            app.status = "awaiting permission…".into();
        }
    }
}

async fn submit_input(app: &mut App, raw: &str) {
    let input = raw.trim();
    if input.is_empty() {
        return;
    }

    if input.starts_with('/') {
        commands::handle_command(app, input);
        app.input.clear();
        app.cursor = 0;
        return;
    }

    if app.running {
        app.push_system("Codey is busy. Wait or press Ctrl-C to cancel.");
        app.input.clear();
        app.cursor = 0;
        return;
    }

    app.history.push(input.to_string());
    app.history_index = None;

    app.entries.push(Entry::User(input.to_string()));
    app.scroll_to_bottom();
    app.input.clear();
    app.cursor = 0;

    let mut tool_defs = app.registry.definitions();
    tool_defs.push(subagent::delegate_definition());
    let summaries = skills::summaries(&app.skills);
    let selected = skills::select_for_task(&app.skills, input);
    let mut prompt = agent::prompt::build_system_prompt(&tool_defs, &app.instructions, &summaries);
    for skill in &selected {
        prompt.push_str(&format!(
            "\n\nSKILL: {}\n{}",
            skill.name, skill.instructions
        ));
    }

    let mut session = app.sessions.current().clone();
    session.add_user_message(input.to_string());
    let tx = app.event_tx.clone();
    let provider = app.provider.clone();
    let registry = app.registry.clone();
    let active = app.active_subagents.clone();
    let mode = app.confirmation_mode;

    let (res_tx, res_rx) = mpsc::unbounded_channel();
    app.result_rx = Some(res_rx);

    app.agent_task = Some(tokio::spawn(async move {
        let _ = agent::run_agent(
            &mut session,
            &provider,
            registry.as_ref(),
            &prompt,
            &tx,
            mode,
            false,
            0,
            active,
            None,
        )
        .await;
        let _ = res_tx.send(session);
    }));

    app.running = true;
    app.status = "thinking…".into();
}

pub fn cancel_run(app: &mut App) {
    if let Some(task) = app.agent_task.take() {
        task.abort();
    }
    app.running = false;
    app.permission = None;
    app.status = "ready".into();
}
