use super::session::Session;

#[derive(Debug)]
pub struct SessionManager {
    sessions: Vec<Session>,
    current: usize,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: vec![Session::new()],
            current: 0,
        }
    }

    pub fn current(&self) -> &Session {
        &self.sessions[self.current]
    }

    pub fn current_mut(&mut self) -> &mut Session {
        &mut self.sessions[self.current]
    }

    pub fn new_session(&mut self) {
        self.sessions.push(Session::new());

        self.current = self.sessions.len() - 1;
    }

    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return false;
        }

        self.current = index;
        true
    }

    pub fn previous(&mut self) -> bool {
        if self.current == 0 {
            return false;
        }

        self.current -= 1;
        true
    }

    pub fn next(&mut self) -> bool {
        if self.current + 1 >= self.sessions.len() {
            return false;
        }

        self.current += 1;
        true
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }
}
