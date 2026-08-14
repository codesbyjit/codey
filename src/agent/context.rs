use super::ChatMessage;

#[derive(Debug, Default)]
pub struct Context {
    messages: Vec<ChatMessage>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    pub fn add(&mut self, message: ChatMessage) {
        self.messages.push(message);
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
            .map(|message| message.content.len() / 4)
            .sum()
    }
}
