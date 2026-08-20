use async_trait::async_trait;
use serde_json::{json, Value};

use super::{resolve_path, schema, Tool, ToolContext};
use crate::provider::string_arg;

const MAX_READ_BYTES: usize = 200_000;

pub struct ListFilesTool;

#[async_trait]
impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List the files and directories at a path. Use '.' for the workspace root."
    }

    fn parameters(&self) -> Value {
        schema(
            json!({"path": {"type": "string", "description": "Directory to list"}}),
            &["path"],
        )
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String> {
        let path = string_arg(args, "path")?;
        let target = resolve_path(ctx, &path);

        let entries = std::fs::read_dir(&target)
            .map_err(|e| format!("Failed to list `{}`: {e}", target.display()))?;

        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                dirs.push(format!("{name}/"));
            } else {
                files.push(name);
            }
        }
        dirs.sort();
        files.sort();

        let mut out = String::new();
        for name in dirs.iter().chain(files.iter()) {
            out.push_str(name);
            out.push('\n');
        }
        if out.is_empty() {
            out.push_str("(empty directory)");
        }
        Ok(out)
    }
}

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. For large files only the first portion is returned."
    }

    fn parameters(&self) -> Value {
        schema(
            json!({"path": {"type": "string", "description": "File to read"}}),
            &["path"],
        )
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String> {
        let path = string_arg(args, "path")?;
        let target = resolve_path(ctx, &path);

        let bytes = std::fs::read(&target)
            .map_err(|e| format!("Failed to read `{}`: {e}", target.display()))?;

        if bytes.len() > MAX_READ_BYTES {
            let preview = String::from_utf8_lossy(&bytes[..MAX_READ_BYTES]);
            return Ok(format!(
                "File is {} bytes; returning first {MAX_READ_BYTES} bytes.\n\n{preview}\n\n[truncated — use search_file to find specific content]",
                bytes.len()
            ));
        }

        let text = String::from_utf8(bytes)
            .map_err(|_| format!("`{}` is not valid UTF-8", target.display()))?;

        Ok(text)
    }
}

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Create or overwrite a file with the given content. Use edit_file for targeted changes."
    }

    fn is_destructive(&self) -> bool {
        true
    }

    fn parameters(&self) -> Value {
        schema(
            json!({
                "path": {"type": "string", "description": "File to write"},
                "content": {"type": "string", "description": "Full new content"}
            }),
            &["path", "content"],
        )
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String> {
        let path = string_arg(args, "path")?;
        let content = string_arg(args, "content")?;
        let target = resolve_path(ctx, &path);

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create dir `{}`: {e}", parent.display()))?;
        }

        let len = content.len();
        let previous = std::fs::read_to_string(&target).ok();
        let diff_note = previous
            .map(|prev| format!("\n{}", line_diff(&prev, content.as_str())))
            .unwrap_or_default();
        std::fs::write(&target, &content)
            .map_err(|e| format!("Failed to write `{}`: {e}", target.display()))?;

        Ok(format!(
            "Wrote {} bytes to `{}`.{}",
            len,
            target.display(),
            diff_note
        ))
    }
}

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Make a targeted change to a file by replacing an exact old_string with new_string. Set replace_all to change every occurrence."
    }

    fn is_destructive(&self) -> bool {
        true
    }

    fn parameters(&self) -> Value {
        schema(
            json!({
                "path": {"type": "string", "description": "File to edit"},
                "old_string": {"type": "string", "description": "Exact text to replace"},
                "new_string": {"type": "string", "description": "Replacement text"},
                "replace_all": {"type": "boolean", "description": "Replace all occurrences", "default": false}
            }),
            &["path", "old_string", "new_string"],
        )
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String> {
        let path = string_arg(args, "path")?;
        let old_string = string_arg(args, "old_string")?;
        let new_string = string_arg(args, "new_string")?;
        let replace_all = args
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let target = resolve_path(ctx, &path);

        let original = std::fs::read_to_string(&target)
            .map_err(|e| format!("Failed to read `{}`: {e}", target.display()))?;

        if !original.contains(&old_string) {
            return Err(format!(
                "old_string not found in `{}`. The text must match exactly, including whitespace.",
                target.display()
            ));
        }

        let updated = if replace_all {
            original.replace(&old_string, &new_string)
        } else {
            original.replacen(&old_string, &new_string, 1)
        };

        std::fs::write(&target, &updated)
            .map_err(|e| format!("Failed to write `{}`: {e}", target.display()))?;

        let changed = if replace_all {
            original.matches(&old_string).count()
        } else {
            1
        };

        let mut diff = String::new();
        for line in old_string.lines() {
            diff.push_str(&format!("-{line}\n"));
        }
        for line in new_string.lines() {
            diff.push_str(&format!("+{line}\n"));
        }

        Ok(format!(
            "Edited `{}` ({} replacement(s)):\n{}",
            target.display(),
            changed,
            diff
        ))
    }
}

fn line_diff(old: &str, new: &str) -> String {
    let a: Vec<&str> = old.split('\n').collect();
    let b: Vec<&str> = new.split('\n').collect();
    if a.len().saturating_mul(b.len()) > 100_000 {
        return format!("(changed {} → {} lines)", a.len(), b.len());
    }

    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut out = String::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push_str(&format!("-{}\n", a[i]));
            i += 1;
        } else {
            out.push_str(&format!("+{}\n", b[j]));
            j += 1;
        }
    }
    while i < n {
        out.push_str(&format!("-{}\n", a[i]));
        i += 1;
    }
    while j < m {
        out.push_str(&format!("+{}\n", b[j]));
        j += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn line_diff_shows_added_and_removed() {
        let d = line_diff("a\nb\nc", "a\nB\nc\nd");
        assert!(d.contains("-b"));
        assert!(d.contains("+B"));
        assert!(d.contains("+d"));
        assert!(!d.contains("-a"));
    }

    #[tokio::test]
    async fn edit_file_reports_a_diff() {
        let dir = std::env::temp_dir().join("codey_edit_test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f.txt");
        std::fs::write(&file, "hello\nworld\n").unwrap();

        let ctx = ToolContext::new(&dir);
        let tool = EditFileTool;
        let out = tool
            .execute(
                &json!({"path": "f.txt", "old_string": "world", "new_string": "there"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(out.contains("-world"));
        assert!(out.contains("+there"));
        assert!(std::fs::read_to_string(&file).unwrap().contains("there"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
