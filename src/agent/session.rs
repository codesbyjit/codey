use super::{
    config,
    context::Context,
    prompt,
    ChatMessage,
};

#[derive(Debug)]
pub struct Session {
    context: Context,
    model: String,
}

impl Session {
    pub fn new() -> Self {
        Self {
            context: Context::new(),
            model: config::default_model().to_string(),
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn set_model(
        &mut self,
        model: impl Into<String>,
    ) {
        self.model = model.into();
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.context
    }

    pub fn clear(&mut self) {
        self.context.clear();
        self.initialize();
    }

    pub fn initialize(&mut self) {
        self.context.add(ChatMessage {
            role: "system".to_string(),
            content: prompt::SYSTEM_PROMPT.to_string(),
        });
    }

    pub fn add_user_message(
        &mut self,
        content: impl Into<String>,
    ) {
        self.context.add(ChatMessage {
            role: "user".to_string(),
            content: content.into(),
        });
    }

    pub fn add_assistant_message(
        &mut self,
        content: impl Into<String>,
    ) {
        self.context.add(ChatMessage {
            role: "assistant".to_string(),
            content: content.into(),
        });
    }

    pub fn add_tool_result(
        &mut self,
        tool: &str,
        result: &str,
    ) {
        self.context.add(ChatMessage {
            role: "user".to_string(),
            content: format!(
                "Tool `{tool}` result:\n{result}"
            ),
        });
    }

    pub fn messages(&self) -> &[ChatMessage] {
        self.context.messages()
    }
}