use std::io::{self, Stdout};

use crossterm::{
    event::{
        self,
        Event,
        KeyCode,
        KeyEvent,
        KeyModifiers,
    },
    execute,
    terminal::{
        disable_raw_mode,
        enable_raw_mode,
        EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

use ratatui::{
    backend::CrosstermBackend,
    layout::{
        Constraint,
        Direction,
        Layout,
        Rect,
    },
    style::{
        Modifier,
        Style,
    },
    text::Line,
    widgets::{
        Block,
        Borders,
        Paragraph,
        Wrap,
    },
    Frame,
    Terminal,
};

use crate::agent;
use crate::agent::session::Session;

type AppResult<T> =
    Result<T, Box<dyn std::error::Error>>;

pub async fn run() -> AppResult<()> {
    let mut terminal = setup_terminal()?;

    let result =
        app_loop(&mut terminal).await;

    restore_terminal(&mut terminal)?;

    result
}

fn setup_terminal(
) -> AppResult<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    execute!(
        stdout,
        EnterAlternateScreen
    )?;

    let backend =
        CrosstermBackend::new(stdout);

    let mut terminal =
        Terminal::new(backend)?;

    terminal.clear()?;

    Ok(terminal)
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> AppResult<()> {
    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;

    terminal.show_cursor()?;

    Ok(())
}

struct App {
    session: Session,

    messages: Vec<Message>,

    input: String,
    cursor: usize,

    should_quit: bool,
}

struct Message {
    role: Role,
    content: String,
}

enum Role {
    User,
    Assistant,
    System,
}

impl App {
    fn new() -> Self {
        let mut session =
            Session::new();

        session.initialize();

        Self {
            session,

            messages: vec![
                Message {
                    role: Role::System,
                    content:
                        "Welcome to Codey. Type /help for commands."
                            .to_string(),
                }
            ],

            input: String::new(),

            cursor: 0,

            should_quit: false,
        }
    }

    fn add_user_message(
        &mut self,
        content: String,
    ) {
        self.messages.push(
            Message {
                role: Role::User,
                content,
            },
        );
    }

    fn add_assistant_message(
        &mut self,
        content: String,
    ) {
        self.messages.push(
            Message {
                role: Role::Assistant,
                content,
            },
        );
    }

    fn add_system_message(
        &mut self,
        content: String,
    ) {
        self.messages.push(
            Message {
                role: Role::System,
                content,
            },
        );
    }

    fn clear(&mut self) {
        self.session.clear();

        self.messages.clear();

        self.add_system_message(
            "Context cleared.".to_string(),
        );
    }
}

async fn app_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> AppResult<()> {
    let mut app = App::new();

    loop {
        terminal.draw(|frame| {
            draw(frame, &app);
        })?;

        if app.should_quit {
            break;
        }

        if !event::poll(
            std::time::Duration::from_millis(100),
        )? {
            continue;
        }

        let event =
            event::read()?;

        if let Event::Key(key) = event {
            handle_key(
                &mut app,
                key,
            )
            .await?;
        }
    }

    Ok(())
}

async fn handle_key(
    app: &mut App,
    key: KeyEvent,
) -> AppResult<()> {
    match key.code {
        KeyCode::Char('c')
            if key
                .modifiers
                .contains(
                    KeyModifiers::CONTROL,
                ) =>
        {
            app.should_quit = true;
        }

        KeyCode::Esc => {
            app.input.clear();
            app.cursor = 0;
        }

        KeyCode::Enter => {
            submit_input(app).await?;
        }

        KeyCode::Backspace => {
            if app.cursor > 0 {
                app.input.remove(
                    app.cursor - 1,
                );

                app.cursor -= 1;
            }
        }

        KeyCode::Left => {
            if app.cursor > 0 {
                app.cursor -= 1;
            }
        }

        KeyCode::Right => {
            if app.cursor
                < app.input.len()
            {
                app.cursor += 1;
            }
        }

        KeyCode::Home => {
            app.cursor = 0;
        }

        KeyCode::End => {
            app.cursor =
                app.input.len();
        }

        KeyCode::Char(c) => {
            app.input.insert(
                app.cursor,
                c,
            );

            app.cursor += 1;
        }

        _ => {}
    }

    Ok(())
}

async fn submit_input(
    app: &mut App,
) -> AppResult<()> {
    let input =
        app.input.trim().to_string();

    if input.is_empty() {
        return Ok(());
    }

    app.input.clear();
    app.cursor = 0;

    /*
     * Commands are handled locally.
     */
    if input.starts_with('/') {
        handle_command(
            app,
            &input,
        )?;

        return Ok(());
    }

    /*
     * Display user message.
     */
    app.add_user_message(
        input.clone(),
    );

    /*
     * Display temporary response.
     */
    app.add_assistant_message(
        "Thinking...".to_string(),
    );

    /*
     * IMPORTANT:
     *
     * We use the persistent Session here.
     *
     * Do NOT use agent::run().
     *
     * agent::run() creates a new Session
     * for every request.
     */
    let result =
        agent::run_session(
            &mut app.session,
            &input,
        )
        .await;

    /*
     * Replace "Thinking..." with
     * the actual result.
     */
    if let Some(message) =
        app.messages.last_mut()
    {
        match result {
            Ok(response) => {
                message.content =
                    response;
            }

            Err(error) => {
                message.content =
                    format!(
                        "Error: {error}"
                    );
            }
        }
    }

    Ok(())
}

fn handle_command(
    app: &mut App,
    command: &str,
) -> AppResult<()> {
    let mut parts =
        command.split_whitespace();

    let command =
        parts.next().unwrap_or("");

    match command {
        "/help" => {
            app.add_system_message(
                [
                    "/help      Show commands",
                    "/model     Show current model",
                    "/context   Show context info",
                    "/clear     Clear conversation",
                    "/quit      Exit Codey",
                ]
                .join("\n"),
            );
        }

        "/model" => {
            app.add_system_message(
                format!(
                    "Current model: {}",
                    app.session.model()
                ),
            );
        }

        "/context" => {
            let context =
                app.session.context();

            let messages =
                context.len();

            let tokens =
                context
                    .estimated_tokens();

            app.add_system_message(
                format!(
                    "Messages: {messages}\n\
                     Estimated tokens: {tokens}"
                ),
            );
        }

        "/clear" => {
            app.clear();
        }

        "/quit" | "/exit" => {
            app.should_quit = true;
        }

        _ => {
            app.add_system_message(
                format!(
                    "Unknown command: {command}\n\
                     Type /help to see available commands."
                ),
            );
        }
    }

    Ok(())
}

fn draw(
    frame: &mut Frame,
    app: &App,
) {
    let size =
        frame.area();

    let chunks =
        Layout::default()
            .direction(
                Direction::Vertical,
            )
            .constraints([
                Constraint::Min(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(size);

    draw_messages(
        frame,
        app,
        chunks[0],
    );

    draw_input(
        frame,
        app,
        chunks[1],
    );

    draw_status(
        frame,
        app,
        chunks[2],
    );
}

fn draw_messages(
    frame: &mut Frame,
    app: &App,
    area: Rect,
) {
    let lines: Vec<Line> =
        app.messages
            .iter()
            .flat_map(|message| {
                let prefix =
                    match message.role {
                        Role::User =>
                            "You",

                        Role::Assistant =>
                            "Codey",

                        Role::System =>
                            "System",
                    };

                let mut lines =
                    Vec::new();

                /*
                 * Render the role on
                 * its own line.
                 */
                lines.push(
                    Line::from(
                        format!(
                            "{prefix}:"
                        ),
                    ),
                );

                /*
                 * Preserve multiline
                 * model output.
                 */
                for line in
                    message.content.lines()
                {
                    lines.push(
                        Line::from(
                            format!(
                                "  {line}"
                            ),
                        ),
                    );
                }

                /*
                 * Add spacing between
                 * messages.
                 */
                lines.push(
                    Line::from(""),
                );

                lines.into_iter()
            })
            .collect();

    let paragraph =
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Codey ")
                    .borders(
                        Borders::ALL,
                    ),
            )
            .wrap(
                Wrap {
                    trim: false,
                },
            );

    frame.render_widget(
        paragraph,
        area,
    );
}

fn draw_input(
    frame: &mut Frame,
    app: &App,
    area: Rect,
) {
    let input =
        Paragraph::new(
            app.input.as_str(),
        )
        .block(
            Block::default()
                .title(" Prompt ")
                .borders(
                    Borders::ALL,
                ),
        )
        .wrap(
            Wrap {
                trim: false,
            },
        );

    frame.render_widget(
        input,
        area,
    );

    /*
     * Prevent the cursor from
     * going outside the input box.
     */
    let cursor_x =
        area.x
            + 1
            + app.cursor
                .min(
                    area.width
                        .saturating_sub(2)
                        as usize,
                ) as u16;

    let cursor_y =
        area.y + 1;

    frame.set_cursor_position((
        cursor_x,
        cursor_y,
    ));
}

fn draw_status(
    frame: &mut Frame,
    app: &App,
    area: Rect,
) {
    let model =
        app.session.model();

    let context =
        app.session.context();

    let tokens =
        context
            .estimated_tokens();

    let messages =
        context.len();

    let text =
        format!(
            " {model} │ \
             context {tokens} tokens │ \
             {messages} messages │ \
             /help │ /quit "
        );

    let status =
        Paragraph::new(text)
            .style(
                Style::default()
                    .add_modifier(
                        Modifier::DIM,
                    ),
            );

    frame.render_widget(
        status,
        area,
    );
}