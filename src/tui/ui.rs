use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::{App, Entry};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_messages(frame, app, chunks[0]);
    draw_input(frame, app, chunks[1]);
    draw_status(frame, app, chunks[2]);

    if app.show_help {
        draw_help(frame);
    }

    if app.permission.is_some() {
        draw_permission(frame, app);
    }
}

fn draw_messages(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.entries {
        match entry {
            Entry::User(text) => {
                lines.push(Line::from(Span::styled(
                    "You",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Green),
                )));
                for line in text.lines() {
                    lines.push(Line::from(format!("  {line}")));
                }
            }
            Entry::Assistant(text) => {
                lines.push(Line::from(Span::styled(
                    "Codey",
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Cyan),
                )));
                for line in text.lines() {
                    lines.push(Line::from(format!("  {line}")));
                }
            }
            Entry::Tool {
                name,
                summary,
                output,
                is_error,
            } => {
                let color = if *is_error { Color::Red } else { Color::Yellow };
                lines.push(Line::from(Span::styled(
                    format!("Tool ▸ {name}"),
                    Style::default().add_modifier(Modifier::BOLD).fg(color),
                )));
                if !summary.is_empty() {
                    for line in summary.lines() {
                        lines.push(Line::from(Span::styled(
                            format!("  ↳ {line}"),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
                let out_lines: Vec<&str> = output.lines().collect();
                let cap = 80;
                let shown = if out_lines.len() > cap {
                    &out_lines[..cap]
                } else {
                    &out_lines[..]
                };
                for line in shown {
                    lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        Style::default().fg(if *is_error { Color::Red } else { Color::Gray }),
                    )));
                }
                if out_lines.len() > cap {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  … {} more lines (output truncated)",
                            out_lines.len() - cap
                        ),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
            Entry::System(text) => {
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("· {line}"),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
            Entry::Error(text) => {
                lines.push(Line::from(Span::styled(
                    format!("Error: {text}"),
                    Style::default().fg(Color::Red),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(2);
    let max_scroll = total.saturating_sub(visible);
    let scroll = if app.auto_scroll {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    };

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(format!(
                    " Codey │ Session {} ",
                    app.sessions.current_index() + 1
                ))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = if app.running {
        "▌ thinking… (Ctrl-C to cancel)"
    } else {
        "> "
    };
    let content = format!("{prompt}{}", app.input);
    let paragraph = Paragraph::new(content)
        .block(Block::default().title(" Prompt ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    let x = area.x + 1 + prompt.chars().count() as u16 + app.cursor as u16;
    let x = x.min(area.x + area.width.saturating_sub(2));
    frame.set_cursor_position((x, area.y + 1));
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let session = app.sessions.current();
    let tokens = session.context().estimated_tokens();
    let budget = session.context().budget();
    let percent = if budget == 0 {
        0.0
    } else {
        (tokens as f64 / budget as f64) * 100.0
    };

    let color = if percent >= 90.0 {
        Color::Red
    } else if percent >= 80.0 {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    let left = format!(
        "Session {} │ {} │ {} msgs │ ~{} / {} tk ({:.1}%)",
        app.sessions.current_index() + 1,
        session.model(),
        session.context().len(),
        tokens,
        budget,
        percent
    );
    let right = format!("{} │ /help │ Ctrl-C", app.status);

    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(color)),
        Span::raw("   "),
        Span::styled(right, Style::default().add_modifier(Modifier::DIM)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);
    let text = "\
CODEY COMMANDS

  /help            toggle this help
  /new             new session
  /sessions        list sessions
  /session <n>     switch session
  /prev /next      switch session
  /model           show model
  /model <name>    change model
  /context         context usage
  /clear           clear session
  /skills          discovered skills
  /agents          subagents
  /tools           available tools
  /mcp             MCP configuration
  /quit            exit

KEYS
  Enter       submit
  Shift+Enter newline (not in this build)
  Ctrl-C      cancel run / quit
  Ctrl-L      clear input
  PageUp/Dn   scroll
  y / n       answer permission prompts

Press /help or Esc to close.";
    let p = Paragraph::new(text)
        .block(Block::default().title(" Codey Help ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn draw_permission(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, frame.area());
    frame.render_widget(Clear, area);
    let description = app
        .permission
        .as_ref()
        .map(|p| p.description.clone())
        .unwrap_or_default();
    let text = format!("{description}\n\nAllow this action?\n\n  y = allow   n = deny");
    let p = Paragraph::new(text)
        .block(Block::default().title(" Confirm ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_w = area.width * percent_x / 100;
    let popup_h = area.height * percent_y / 100;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    Rect {
        x,
        y,
        width: popup_w,
        height: popup_h,
    }
}
