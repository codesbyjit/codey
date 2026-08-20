use std::collections::HashMap;

use serde_json::Value;

use super::Tool;
use crate::provider::ToolDefinition;

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    by_name: HashMap<String, usize>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            by_name: HashMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        if let Some(index) = self.by_name.get(&name) {
            self.tools[*index] = tool;
        } else {
            self.by_name.insert(name, self.tools.len());
            self.tools.push(tool);
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.by_name.get(name).map(|i| self.tools[*i].as_ref())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name().to_string()).collect()
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    pub fn is_destructive(&self, name: &str) -> bool {
        self.get(name).map(|t| t.is_destructive()).unwrap_or(false)
    }

    pub async fn execute(
        &self,
        name: &str,
        args: &Value,
        ctx: &super::ToolContext,
    ) -> Result<String, String> {
        match self.get(name) {
            Some(tool) => tool.execute(args, ctx).await,
            None => Err(format!(
                "Unknown tool `{name}`. Available tools: {}.",
                self.names().join(", ")
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Tool, ToolContext};
    use async_trait::async_trait;
    use serde_json::{json, Value};

    struct FakeTool;

    #[async_trait]
    impl Tool for FakeTool {
        fn name(&self) -> &str {
            "fake"
        }
        fn description(&self) -> &str {
            "a fake tool"
        }
        fn parameters(&self) -> Value {
            json!({})
        }
        async fn execute(&self, _args: &Value, _ctx: &ToolContext) -> Result<String, String> {
            Ok("fake result".into())
        }
    }

    #[tokio::test]
    async fn registers_and_executes() {
        let mut reg = ToolRegistry::new();
        reg.register(Box::new(FakeTool));
        assert!(reg.contains("fake"));
        assert_eq!(reg.names(), vec!["fake".to_string()]);
        let ctx = ToolContext::new("/tmp");
        let out = reg.execute("fake", &json!({}), &ctx).await.unwrap();
        assert_eq!(out, "fake result");
    }

    #[tokio::test]
    async fn unknown_tool_returns_model_readable_error() {
        let reg = ToolRegistry::new();
        let ctx = ToolContext::new("/tmp");
        let err = reg
            .execute("does_not_exist", &json!({}), &ctx)
            .await
            .unwrap_err();
        assert!(err.contains("Unknown tool"));
    }
}
