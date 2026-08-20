use crate::provider::ChatMessage;

const CHARS_PER_TOKEN: usize = 4;

const MESSAGE_OVERHEAD: usize = 4;

#[derive(Debug, Default, Clone)]
pub struct Context {
    messages: Vec<ChatMessage>,

    budget: usize,
}

impl Context {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            budget: crate::config::DEFAULT_CONTEXT_WINDOW,
        }
    }

    pub fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }

    pub fn add(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    pub fn add_user(&mut self, content: impl Into<String>) {
        self.add(ChatMessage::user(content));
    }

    pub fn add_assistant(&mut self, content: impl Into<String>) {
        self.add(ChatMessage::assistant(content));
    }

    pub fn add_tool_result(&mut self, tool: impl Into<String>, result: impl Into<String>) {
        let tool_name = tool.into();
        self.add(ChatMessage::tool(
            tool_name.clone(),
            format!("Tool `{}` result:\n{}", tool_name, result.into()),
        ));
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn estimated_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| m.content.len() / CHARS_PER_TOKEN + MESSAGE_OVERHEAD)
            .sum()
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn usage_percent(&self) -> f64 {
        if self.budget == 0 {
            return 0.0;
        }
        (self.estimated_tokens() as f64 / self.budget as f64) * 100.0
    }

    pub fn truncate_to_budget(&mut self, keep_below_percent: f64) -> usize {
        if self.usage_percent() < keep_below_percent {
            return 0;
        }

        let mut removed = 0;

        let mut i = 0;
        while i < self.messages.len()
            && self.usage_percent() >= keep_below_percent
            && self.messages.len() > 2
        {
            let role = &self.messages[i].role;
            if role == "tool" || role == "assistant" {
                self.messages.remove(i);
                removed += 1;
            } else {
                i += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_tokens() {
        let mut ctx = Context::new();
        ctx.add_user("hello world");
        ctx.add_assistant("this is a response");
        assert!(ctx.estimated_tokens() > 0);
        assert_eq!(ctx.len(), 2);
    }

    #[test]
    fn truncates_when_over_budget() {
        let mut ctx = Context::new().with_budget(20);
        ctx.add_user("keep me");
        for _ in 0..50 {
            ctx.add_tool_result("shell", "x".repeat(200));
        }
        let removed = ctx.truncate_to_budget(80.0);
        assert!(removed > 0);
        assert_eq!(ctx.messages().first().unwrap().role, "user");
    }
}
