use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use walkdir::WalkDir;

use super::{resolve_path, schema, Tool, ToolContext};
use crate::provider::string_arg;

const MAX_MATCHES: usize = 200;

pub struct SearchFilesTool;

#[async_trait]
impl Tool for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search file contents for a regular expression under a path. Returns matching lines with file and line numbers."
    }

    fn parameters(&self) -> Value {
        schema(
            json!({
                "pattern": {"type": "string", "description": "Regular expression to search for"},
                "path": {"type": "string", "description": "Directory or file to search (default workspace root)"},
                "file_glob": {"type": "string", "description": "Optional glob to limit files, e.g. \"*.rs\""}
            }),
            &["pattern"],
        )
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String> {
        let pattern = string_arg(args, "pattern")?;
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p.to_string(),
            _ => ".".to_string(),
        };
        let file_glob = args.get("file_glob").and_then(|v| v.as_str());

        let regex = Regex::new(&pattern).map_err(|e| format!("Invalid regular expression: {e}"))?;

        let target = resolve_path(ctx, &path);

        let mut matches: Vec<String> = Vec::new();

        for entry in WalkDir::new(&target)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            if let Some(glob) = file_glob {
                if !matches_glob(path.to_string_lossy().as_ref(), glob) {
                    continue;
                }
            }
            if is_hidden_or_large(path) {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (line_no, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matches.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        line_no + 1,
                        line.trim_end()
                    ));
                    if matches.len() >= MAX_MATCHES {
                        break;
                    }
                }
            }
            if matches.len() >= MAX_MATCHES {
                break;
            }
        }

        if matches.is_empty() {
            return Ok("No matches found.".to_string());
        }

        let mut out = matches.join("\n");
        if matches.len() >= MAX_MATCHES {
            out.push_str(&format!("\n\n[truncated at {MAX_MATCHES} matches]"));
        }
        Ok(out)
    }
}

fn is_hidden_or_large(path: &std::path::Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') && (name.starts_with(".git")) {
            return true;
        }
    }
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > 5_000_000 {
            return true;
        }
    }
    false
}

fn matches_glob(path: &str, glob: &str) -> bool {
    if !glob.contains('*') {
        return path.ends_with(glob);
    }
    let parts: Vec<&str> = glob.split('*').collect();
    let mut idx = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            idx = part.len();
        } else if i == parts.len() - 1 {
            return path[idx..].contains(part);
        } else if let Some(pos) = path[idx..].find(part) {
            idx += pos + part.len();
        } else {
            return false;
        }
    }
    true
}
