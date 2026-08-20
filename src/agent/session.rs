use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::context::Context;
use crate::config;
use crate::storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionFile {
    id: String,
    created_at: String,
    updated_at: String,
    model: String,
    provider: String,
    messages: Vec<crate::provider::ChatMessage>,
}

#[derive(Clone)]
pub struct Session {
    id: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    model: String,
    provider: String,
    context: Context,
}

impl Session {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            model: config::DEFAULT_MODEL.to_string(),
            provider: "openrouter".to_string(),
            context: Context::new().with_budget(config::DEFAULT_CONTEXT_WINDOW),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn set_provider(&mut self, provider: impl Into<String>) {
        self.provider = provider.into();
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.context.add_user(content);
        self.touch();
    }

    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.context.add(ChatMessage::assistant(content));
        self.touch();
    }

    pub fn add_tool_result(&mut self, tool: impl Into<String>, result: impl Into<String>) {
        self.context.add_tool_result(tool, result);
        self.touch();
    }

    pub fn clear(&mut self) {
        self.context.clear();
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    pub fn title(&self) -> String {
        self.context
            .messages()
            .iter()
            .find(|m| m.role == "user")
            .map(|m| {
                let t = m.content.lines().next().unwrap_or("").trim();
                if t.len() > 40 {
                    format!("{}…", &t[..40])
                } else {
                    t.to_string()
                }
            })
            .unwrap_or_else(|| "(empty)".to_string())
    }

    pub fn save(&self) -> Result<()> {
        let file = SessionFile {
            id: self.id.clone(),
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
            model: self.model.clone(),
            provider: self.provider.clone(),
            messages: self.context.messages().to_vec(),
        };
        let json = serde_json::to_string_pretty(&file)?;
        storage::save_session(&self.id, &json)
    }

    pub fn load(id: &str) -> Result<Option<Self>> {
        let json = match storage::load_session(id) {
            Ok(j) => j,
            Err(_) => return Ok(None),
        };
        let file: SessionFile = serde_json::from_str(&json)?;

        let mut context = Context::new().with_budget(config::DEFAULT_CONTEXT_WINDOW);
        for message in file.messages {
            context.add(message);
        }

        let created = DateTime::parse_from_rfc3339(&file.created_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let updated = DateTime::parse_from_rfc3339(&file.updated_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(Some(Self {
            id: file.id,
            created_at: created,
            updated_at: updated,
            model: file.model,
            provider: file.provider,
            context,
        }))
    }

    pub fn workspace(&self) -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

use crate::provider::ChatMessage;

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

    pub fn load_all() -> Self {
        let ids = storage::list_sessions();
        let mut sessions: Vec<Session> = Vec::new();
        for id in ids {
            if let Ok(Some(session)) = Session::load(&id) {
                sessions.push(session);
            }
        }
        if sessions.is_empty() {
            sessions.push(Session::new());
        }
        let current = sessions.len() - 1;
        Self { sessions, current }
    }

    pub fn current(&self) -> &Session {
        &self.sessions[self.current]
    }

    pub fn current_mut(&mut self) -> &mut Session {
        &mut self.sessions[self.current]
    }

    pub fn new_session(&mut self) -> usize {
        self.sessions.push(Session::new());
        self.current = self.sessions.len() - 1;
        self.current
    }

    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return false;
        }
        self.current = index;
        true
    }

    pub fn replace_current(&mut self, session: Session) {
        if self.sessions.is_empty() {
            self.sessions.push(session);
            self.current = 0;
            return;
        }
        self.sessions[self.current] = session;
    }

    pub fn previous(&mut self) -> bool {
        if self.current == 0 {
            return false;
        }
        self.current -= 1;
        true
    }

    #[allow(clippy::should_implement_trait)]
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

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn current_index(&self) -> usize {
        self.current
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn save_all(&self) -> Result<()> {
        for session in &self.sessions {
            session.save()?;
        }
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
