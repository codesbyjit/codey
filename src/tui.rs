use std::io::{self, Stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use unicode_width::UnicodeWidthStr;

use crate::agent;
use crate::agent::session_manager::SessionManager;

type AppResult<T> = Result<T, Box<dyn std::error::Error>>;

pub async fn run() -> AppResult<()> {
    let mut terminal = setup_terminal()?;

    let result = app_loop(&mut terminal).await;

    restore_terminal(&mut terminal)?;

    result
}

fn setup_terminal() -> AppResult<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;

    let stdout = io::stdout();

    let mut stdout = stdout;

    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    terminal.clear()?;

    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> AppResult<()> {
    disable_raw_mode()?;

    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    terminal.show_cursor()?;

    Ok(())
}

struct App {
    sessions: SessionManager,
    ui_messages: Vec<UiMessage>,
    input: String,
    cursor: usize,
    should_quit: bool,
    thinking: bool,
    pending_task: Option<tokio::task::JoinHandle<Result<String, String>>>,
}

struct UiMessage {
    role: UiRole,
    content: String,
}

enum UiRole {
    System,
}

impl App {
    fn new() -> Self {
        let mut app = Self {
            sessions: SessionManager::new(),

            ui_messages: Vec::new(),

            input: String::new(),
            cursor: 0,

            should_quit: false,
            thinking: false,
            pending_task: None,
        };

        app.add_system_message("Welcome to Codey. Type /help for commands.");

        app
    }

    fn add_system_message(&mut self, content: impl Into<String>) {
        self.ui_messages.push(UiMessage {
            role: UiRole::System,
            content: content.into(),
        });
    }

    fn clear_ui_messages(&mut self) {
        self.ui_messages.clear();
    }

    fn reset_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    fn current_session_number(&self) -> usize {
        self.sessions.current_index() + 1
    }

    /*
     * Rebuild the visible conversation from
     * the currently selected Session.
     */
    fn visible_messages(&self) -> Vec<DisplayMessage> {
        let session = self.sessions.current();

        let mut messages = Vec::new();

        for message in session.messages() {
            /*
             * Don't show the system prompt in the
             * conversation UI.
             */
            if message.role == "system" {
                continue;
            }

            let role = match message.role.as_str() {
                "user" => DisplayRole::User,

                "assistant" => DisplayRole::Assistant,

                "tool" => DisplayRole::Tool,

                _ => DisplayRole::System,
            };

            messages.push(DisplayMessage {
                role,
                content: message.content.clone(),
            });
        }

        /*
         * UI-only messages such as /help,
         * /model and errors.
         */
        for message in &self.ui_messages {
            messages.push(DisplayMessage {
                role: DisplayRole::System,
                content: message.content.clone(),
            });
        }

        messages
    }
}

struct DisplayMessage {
    role: DisplayRole,
    content: String,
}

enum DisplayRole {
    User,
    Assistant,
    Tool,
    System,
}

async fn app_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> AppResult<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|frame| {
            draw(frame, &app);
        })?;

        if app.should_quit {
            break;
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                let submit = handle_key(&mut app, key);

                if submit && !app.thinking {
                    submit_input(&mut app).await?;
                }
            }
        }
    }

    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        KeyCode::Esc => {
            app.reset_input();
        }

        KeyCode::Enter => {
            return true;
        }

        KeyCode::Backspace => {
            delete_previous_char(app);
        }

        KeyCode::Left => {
            move_cursor_left(app);
        }

        KeyCode::Right => {
            move_cursor_right(app);
        }

        KeyCode::Home => {
            app.cursor = 0;
        }

        KeyCode::End => {
            app.cursor = app.input.len();
        }

        KeyCode::Char(c) => {
            insert_char(app, c);
        }

        _ => {}
    }

    false
}

fn insert_char(app: &mut App, character: char) {
    app.input.insert(app.cursor, character);
    app.cursor += character.len_utf8();
}

fn delete_previous_char(app: &mut App) {
    if app.cursor == 0 {
        return;
    }

    let previous = app.input[..app.cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index);

    if let Some(index) = previous {
        app.input.drain(index..app.cursor);
        app.cursor = index;
    }
}

fn move_cursor_left(app: &mut App) {
    if app.cursor == 0 {
        return;
    }

    app.cursor = app.input[..app.cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0);
}

fn move_cursor_right(app: &mut App) {
    if app.cursor >= app.input.len() {
        return;
    }

    if let Some(character) = app.input[app.cursor..].chars().next() {
        app.cursor += character.len_utf8();
    }
}

async fn submit_input(app: &mut App) -> AppResult<()> {
    let input = app.input.trim().to_string();

    if input.is_empty() {
        return Ok(());
    }

    app.reset_input();

    /*
     * Commands are handled locally.
     */
    if input.starts_with('/') {
        handle_command(app, &input)?;

        return Ok(());
    }

    /*
     * The actual conversation is stored
     * inside the current Session.
     */
    let result = agent::run_session(app.sessions.current_mut(), &input).await;

    match result {
        Ok(_) => {}

        Err(error) => {
            app.add_system_message(format!("Error: {error}"));
        }
    }

    Ok(())
}

fn handle_command(app: &mut App, command: &str) -> AppResult<()> {
    let mut parts = command.split_whitespace();

    let command = parts.next().unwrap_or("");

    match command {
        "/help" => {
            app.add_system_message(
                [
                    "/help          Show commands",
                    "/new           Create new session",
                    "/sessions      List sessions",
                    "/session <n>   Switch session",
                    "/prev          Previous session",
                    "/next          Next session",
                    "/model         Show current model",
                    "/model <name>  Change current model",
                    "/context       Show context info",
                    "/clear         Clear current session",
                    "/quit           Exit Codey",
                ]
                .join("\n"),
            );
        }

        "/new" => {
            app.sessions.new_session();

            app.clear_ui_messages();

            app.add_system_message(format!("Started session {}.", app.current_session_number()));
        }

        "/sessions" => {
            let current = app.sessions.current_index();

            let total = app.sessions.len();

            let mut output = String::from("Sessions:\n\n");

            for index in 0..total {
                let marker = if index == current { "*" } else { " " };

                output.push_str(&format!("{marker} Session {}\n", index + 1));
            }

            app.add_system_message(output);
        }

        "/session" => {
            let Some(value) = parts.next() else {
                app.add_system_message("Usage: /session <number>");

                return Ok(());
            };

            let Ok(number) = value.parse::<usize>() else {
                app.add_system_message("Session number must be a number.");

                return Ok(());
            };

            if number == 0 {
                app.add_system_message("Session numbers start at 1.");

                return Ok(());
            }

            if app.sessions.switch_to(number - 1) {
                app.clear_ui_messages();

                app.add_system_message(format!("Switched to session {number}."));
            } else {
                app.add_system_message(format!("Session {number} does not exist."));
            }
        }

        "/prev" => {
            if app.sessions.previous() {
                app.clear_ui_messages();

                app.add_system_message(format!(
                    "Switched to session {}.",
                    app.current_session_number()
                ));
            } else {
                app.add_system_message("Already at the first session.");
            }
        }

        "/next" => {
            if app.sessions.next() {
                app.clear_ui_messages();

                app.add_system_message(format!(
                    "Switched to session {}.",
                    app.current_session_number()
                ));
            } else {
                app.add_system_message("Already at the latest session.");
            }
        }

        "/model" => {
            let model = app.sessions.current().model().to_string();

            app.add_system_message(format!("Current model: {model}"));
        }

        "/context" => {
            let session = app.sessions.current();

            let context = session.context();

            let messages = context.len();

            let tokens = context.estimated_tokens();

            app.add_system_message(format!(
                "Session: {}\n\
                     Messages: {messages}\n\
                     Estimated tokens: {tokens}",
                app.current_session_number()
            ));
        }

        "/clear" => {
            app.sessions.current_mut().clear();

            app.clear_ui_messages();

            app.add_system_message("Context cleared.");
        }

        "/quit" | "/exit" => {
            app.should_quit = true;
        }

        _ => {
            app.add_system_message(format!(
                "Unknown command: {command}\n\
                     Type /help to see available commands."
            ));
        }
    }

    Ok(())
}

fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_messages(frame, app, chunks[0]);

    draw_input(frame, app, chunks[1]);

    draw_status(frame, app, chunks[2]);
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect) {
    let messages = app.visible_messages();

    let mut lines = Vec::<Line>::new();

    for message in messages {
        let prefix = match message.role {
            DisplayRole::User => "You",
            DisplayRole::Assistant => "Codey",
            DisplayRole::Tool => "Tool",
            DisplayRole::System => "System",
        };

        lines.push(Line::from(format!("{prefix}:")));

        for line in message.content.lines() {
            lines.push(Line::from(format!("  {line}")));
        }

        lines.push(Line::from(""));
    }

    // Space available inside the border.
    let height = area.height.saturating_sub(2) as usize;

    // Keep the newest lines visible.
    let start = lines.len().saturating_sub(height);

    let visible = lines.into_iter().skip(start).collect::<Vec<_>>();

    let paragraph = Paragraph::new(visible)
        .block(
            Block::default()
                .title(format!(
                    " Codey | Session {} ",
                    app.current_session_number()
                ))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let input = Paragraph::new(app.input.as_str())
        .block(Block::default().title(" Prompt ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    frame.render_widget(input, area);

    let cursor_width = UnicodeWidthStr::width(&app.input[..app.cursor]);

    let max_width = area.width.saturating_sub(2) as usize;

    let cursor_x = area.x + 1 + cursor_width.min(max_width) as u16;

    let cursor_y = area.y + 1;

    frame.set_cursor_position((cursor_x, cursor_y));
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let session = app.sessions.current();

    let context = session.context();

    let tokens = context.estimated_tokens();

    let usage = session.context_usage_percent();

    let max_tokens = crate::agent::config::DEFAULT_CONTEXT_WINDOW;

    let percentage = if max_tokens == 0 {
        0.0
    } else {
        (tokens as f64 / max_tokens as f64) * 100.0
    };

    let state = if app.thinking { "Thinking..." } else { "Ready" };

    let text = format!(
        " Session {} │ {} │ {} msgs │ ~{} / {} tokens {}% │ {:.1}% context │ /help │ /quit | {}",
        app.current_session_number(),
        session.model(),
        context.len(),
        tokens,
        max_tokens,
        percentage,
        usage,
        state,
    );

    let status = Paragraph::new(text).style(Style::default().add_modifier(Modifier::DIM));

    frame.render_widget(status, area);
}
