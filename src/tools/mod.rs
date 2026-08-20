pub mod filesystem;
pub mod registry;
pub mod search;
pub mod shell;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

pub use filesystem::{EditFileTool, ListFilesTool, ReadFileTool, WriteFileTool};
pub use registry::ToolRegistry;
pub use search::SearchFilesTool;
pub use shell::ShellTool;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub workspace: PathBuf,
}

impl ToolContext {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;

    fn description(&self) -> &str;

    fn parameters(&self) -> Value;

    fn definition(&self) -> crate::provider::ToolDefinition {
        crate::provider::ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }

    fn is_destructive(&self) -> bool {
        false
    }

    fn requires_confirmation(&self, args: &Value) -> bool {
        let _ = args;
        self.is_destructive()
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String>;
}

pub fn resolve_path(ctx: &ToolContext, path: &str) -> PathBuf {
    let candidate = if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        ctx.workspace.join(path)
    };

    normalize(&candidate)
}

fn normalize(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

pub fn is_within(dir: &Path, path: &Path) -> bool {
    let dir = normalize(dir);
    let path = normalize(path);
    path.starts_with(&dir)
}

pub fn schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

pub fn builtin_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ListFilesTool));
    registry.register(Box::new(ReadFileTool));
    registry.register(Box::new(WriteFileTool));
    registry.register(Box::new(EditFileTool));
    registry.register(Box::new(SearchFilesTool));
    registry.register(Box::new(ShellTool));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_paths_against_workspace() {
        let ctx = ToolContext::new("/home/user/project");
        assert_eq!(
            resolve_path(&ctx, "src/main.rs"),
            PathBuf::from("/home/user/project/src/main.rs")
        );
    }

    #[test]
    fn honors_absolute_paths() {
        let ctx = ToolContext::new("/home/user/project");
        assert_eq!(
            resolve_path(&ctx, "/etc/passwd"),
            PathBuf::from("/etc/passwd")
        );
    }

    #[test]
    fn is_within_detects_containment() {
        let dir = PathBuf::from("/home/user/project");
        assert!(is_within(&dir, &dir.join("src/main.rs")));
        assert!(!is_within(&dir, &PathBuf::from("/home/user/other")));
    }

    #[test]
    fn schema_builds_object() {
        let s = schema(json!({"path": {"type": "string"}}), &["path"]);
        assert_eq!(s["type"], "object");
        assert!(s["required"].as_array().unwrap().contains(&json!("path")));
    }
}
