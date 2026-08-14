use super::{ChatMessage, config, context::Context, prompt};

#[derive(Debug)]
pub struct Session {
    context: Context,
    model: String,
}

impl Session {
    pub fn new() -> Self {
        let mut session = Self {
            context: Context::new(),
            model: config::default_model().to_string(),
        };

        session.initialize();

        session
    }

    fn initialize(&mut self) {
        if self.context.is_empty() {
            self.context.add(ChatMessage {
                role: "system".to_string(),
                content: prompt::SYSTEM_PROMPT.to_string(),
            });
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    pub fn messages(&self) -> &[ChatMessage] {
        self.context.messages()
    }

    pub fn clear(&mut self) {
        self.context.clear();
        self.initialize();
    }

    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.context.add(ChatMessage {
            role: "user".to_string(),
            content: content.into(),
        });
    }

    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.context.add(ChatMessage {
            role: "assistant".to_string(),
            content: content.into(),
        });
    }

    pub fn add_tool_result(&mut self, tool: impl Into<String>, result: impl Into<String>) {
        self.context.add(ChatMessage {
            role: "tool".to_string(),
            content: format!("Tool `{}` result:\n{}", tool.into(), result.into()),
        });
    }

    pub fn context_tokens(&self) -> usize {
        self.context.estimated_tokens()
    }

    pub fn context_usage_percent(&self) -> f64 {
        let used = self.context.estimated_tokens() as f64;

        let max = config::DEFAULT_CONTEXT_WINDOW as f64;

        (used / max) * 100.0
    }
}
