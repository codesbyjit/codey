use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

use super::{resolve_path, schema, Tool, ToolContext};
use crate::provider::string_arg;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "run_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace and return its output, exit code, and any errors. Use for builds, tests, git, and other CLI operations."
    }

    fn parameters(&self) -> Value {
        schema(
            json!({
                "command": {"type": "string", "description": "Shell command to execute"},
                "timeout_secs": {"type": "number", "description": "Optional timeout override in seconds"}
            }),
            &["command"],
        )
    }

    fn requires_confirmation(&self, args: &Value) -> bool {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        is_dangerous(command)
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> Result<String, String> {
        let command = string_arg(args, "command")?;
        let timeout = args
            .get("timeout_secs")
            .and_then(|v| v.as_f64())
            .map(|s| Duration::from_secs_f64(s.max(1.0)))
            .unwrap_or(COMMAND_TIMEOUT);

        let workspace = resolve_path(ctx, ".");

        let child = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to execute command: {e}"))?;

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(format!("Error waiting for command: {e}")),
            Err(_) => {
                return Err(format!(
                    "Command timed out after {}s: {command}",
                    timeout.as_secs()
                ));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);

        let mut result = String::new();
        if !stdout.trim().is_empty() {
            result.push_str(stdout.trim_end());
            result.push('\n');
        }
        if !stderr.trim().is_empty() {
            result.push_str("STDERR:\n");
            result.push_str(stderr.trim_end());
            result.push('\n');
        }
        if result.trim().is_empty() {
            result.push_str("(no output)\n");
        }
        result.push_str(&format!("exit code: {code}"));

        Ok(result)
    }
}

pub fn is_dangerous(command: &str) -> bool {
    let trimmed = command.trim();

    const SAFE_PREFIXES: &[&str] = &[
        "ls",
        "cat",
        "pwd",
        "echo",
        "rg",
        "grep",
        "git status",
        "git log",
        "git diff",
        "git show",
        "git branch",
        "cargo check",
        "cargo test",
        "cargo build",
        "cargo run",
        "cargo fmt",
        "npm test",
        "npm run",
        "npx",
        "make",
        "python -m pytest",
        "pytest",
        "go test",
        "go build",
        "go vet",
        "which",
        "head",
        "tail",
        "find",
        "wc",
        "tree",
        "git rev-parse",
        "cargo clippy",
    ];
    for prefix in SAFE_PREFIXES {
        if trimmed.starts_with(prefix) {
            return false;
        }
    }

    const DANGEROUS: &[&str] = &[
        "rm -rf",
        "rm -r",
        "rm -f",
        "rmdir",
        "git reset --hard",
        "git push --force",
        "git push -f",
        "git clean",
        "git checkout --",
        "git restore --staged",
        "mkfs",
        "dd if=",
        ":(){",
        "chmod -R",
        "chown -R",
        "shutdown",
        "reboot",
        "mv /",
        "truncate",
        "drop database",
        "drop table",
    ];
    for pattern in DANGEROUS {
        if trimmed.contains(pattern) {
            return true;
        }
    }

    if trimmed == "rm" || trimmed.starts_with("rm ") {
        return true;
    }

    false
}
