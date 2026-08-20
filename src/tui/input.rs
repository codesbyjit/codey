use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::{cancel_run, App};

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if let Some(prompt) = app.permission.take() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                let _ = prompt.responder.send(true);
                app.status = "running…".into();
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                let _ = prompt.responder.send(false);
                app.status = "ready".into();
            }
            _ => app.permission = Some(prompt),
        }
        return false;
    }

    if app.show_help {
        if key.code == KeyCode::Esc {
            app.show_help = false;
        }
        return false;
    }

    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if app.running {
                cancel_run(app);
            } else {
                app.should_quit = true;
            }
            false
        }

        KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input.clear();
            app.cursor = 0;
            false
        }

        KeyCode::Esc => {
            app.input.clear();
            app.cursor = 0;
            false
        }

        KeyCode::PageUp => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_sub(5);
            false
        }

        KeyCode::PageDown => {
            app.auto_scroll = false;
            app.scroll = app.scroll.saturating_add(5);
            false
        }

        KeyCode::Enter => true,

        KeyCode::Backspace => {
            delete_previous_char(app);
            false
        }

        KeyCode::Delete => {
            delete_next_char(app);
            false
        }

        KeyCode::Left => {
            move_cursor_left(app);
            false
        }

        KeyCode::Right => {
            move_cursor_right(app);
            false
        }

        KeyCode::Home => {
            app.cursor = 0;
            false
        }

        KeyCode::End => {
            app.cursor = app.input.len();
            false
        }

        KeyCode::Up => {
            history_prev(app);
            false
        }

        KeyCode::Down => {
            history_next(app);
            false
        }

        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                return false;
            }
            insert_char(app, c);
            false
        }

        _ => false,
    }
}

fn insert_char(app: &mut App, character: char) {
    app.input.insert(app.cursor, character);
    app.cursor += character.len_utf8();
    app.auto_scroll = true;
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

fn delete_next_char(app: &mut App) {
    if app.cursor >= app.input.len() {
        return;
    }
    if let Some(character) = app.input[app.cursor..].chars().next() {
        let end = app.cursor + character.len_utf8();
        app.input.drain(app.cursor..end);
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

fn history_prev(app: &mut App) {
    if app.history.is_empty() {
        return;
    }
    let next = match app.history_index {
        None => app.history.len() - 1,
        Some(i) => i.saturating_sub(1),
    };
    app.history_index = Some(next);
    app.input = app.history[next].clone();
    app.cursor = app.input.len();
}

fn history_next(app: &mut App) {
    if app.history.is_empty() {
        return;
    }
    match app.history_index {
        None => {}
        Some(i) if i + 1 >= app.history.len() => {
            app.history_index = None;
            app.input.clear();
            app.cursor = 0;
        }
        Some(i) => {
            app.history_index = Some(i + 1);
            app.input = app.history[i + 1].clone();
            app.cursor = app.input.len();
        }
    }
}
