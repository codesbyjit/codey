pub mod config;
pub mod prompt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools;

const MAX_ITERATIONS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ModelResponse {
    #[serde(rename = "type")]
    response_type: String,

    #[serde(default)]
    content: Option<String>,

    #[serde(default)]
    tool: Option<String>,

    #[serde(default)]
    arguments: Option<Value>,

    // Shorthand tool format support:
    //
    // {
    //   "type": "read_file",
    //   "path": "src/main.rs"
    // }
    #[serde(default)]
    path: Option<String>,

    #[serde(default)]
    pattern: Option<String>,

    #[serde(default)]
    command: Option<String>,
}

pub async fn run(
    user_prompt: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let mut history = vec![
        ChatMessage {
            role: "system".to_string(),
            content: prompt::SYSTEM_PROMPT.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user_prompt.to_string(),
        },
    ];

    for iteration in 1..=MAX_ITERATIONS {
        println!("\n--- Agent step {iteration} ---");

        let response = call_model(&client, &history).await?;

        let message = response
            .choices
            .first()
            .ok_or("OpenRouter returned no choices")?
            .message
            .clone();

        println!("\nModel:\n{}", message.content);

        history.push(message.clone());

        let decision = parse_model_response(&message.content)?;

        match decision.response_type.as_str() {
            "tool" => {
                let tool_name = decision
                    .tool
                    .as_deref()
                    .ok_or("Tool response is missing `tool`")?;

                let arguments = decision
                    .arguments
                    .unwrap_or_else(|| Value::Object(Default::default()));

                println!("\nExecuting tool: {tool_name}");

                let result = execute_tool(tool_name, &arguments)?;

                println!("\nTool result:\n{result}");

                history.push(ChatMessage {
                    role: "user".to_string(),
                    content: format!(
                        "Tool `{tool_name}` result:\n{result}"
                    ),
                });
            }

            "final" => {
                return Ok(
                    decision
                        .content
                        .unwrap_or_else(|| "Done.".to_string())
                );
            }

            other => {
                return Err(
                    format!("Unknown response type: {other}")
                        .into()
                );
            }
        }
    }

    Err(
        format!(
            "Codey stopped after reaching the maximum of {MAX_ITERATIONS} agent steps."
        )
        .into(),
    )
}

async fn call_model(
    client: &reqwest::Client,
    history: &[ChatMessage],
) -> Result<OpenRouterResponse, Box<dyn std::error::Error>> {
    let config = config::Config::load()?;

    let payload = OpenRouterRequest {
        model: config.model.clone(),
        messages: history.to_vec(),
    };

    let response = client
        .post(config.api_url())
        .header(
            "Authorization",
            format!("Bearer {}", config.api_key),
        )
        .header("Content-Type", "application/json")
        .header("X-OpenRouter-Title", "Codey")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;

        return Err(
            format!(
                "OpenRouter error ({status}): {body}"
            )
            .into(),
        );
    }

    Ok(response.json().await?)
}

fn parse_model_response(
    text: &str,
) -> Result<ModelResponse, Box<dyn std::error::Error>> {
    let text = text.trim();

    // 1. Normal JSON.
    if let Ok(response) =
        serde_json::from_str::<ModelResponse>(text)
    {
        return Ok(normalize_response(response));
    }

    // 2. JSON inside ```json ... ```.
    if let Some(json) = extract_code_block(text) {
        if let Ok(response) =
            serde_json::from_str::<ModelResponse>(json)
        {
            return Ok(normalize_response(response));
        }
    }

    // 3. JSON surrounded by explanatory text.
    if let Some(json) = extract_json_object(text) {
        if let Ok(response) =
            serde_json::from_str::<ModelResponse>(json)
        {
            return Ok(normalize_response(response));
        }
    }

    Err(
        format!(
            "Model returned an invalid Codey response:\n\n{text}"
        )
        .into(),
    )
}

fn normalize_response(
    mut response: ModelResponse,
) -> ModelResponse {
    // Expected format:
    //
    // {
    //   "type": "tool",
    //   "tool": "read_file",
    //   "arguments": {
    //     "path": "src/main.rs"
    //   }
    // }
    //
    // But some models return:
    //
    // {
    //   "type": "read_file",
    //   "path": "src/main.rs"
    // }
    //
    // Normalize the second form into the first.

    if response.response_type == "tool"
        || response.response_type == "final"
    {
        return response;
    }

    let tool = match response.response_type.as_str() {
        "read_file" => "read_file",
        "write_file" => "write_file",
        "list_files" => "list_files",
        "search" => "search",
        "shell" => "shell",
        _ => return response,
    };

    let arguments = match tool {
        "read_file" | "list_files" => {
            serde_json::json!({
                "path": response
                    .path
                    .clone()
                    .unwrap_or_else(|| ".".to_string())
            })
        }

        "write_file" => {
            serde_json::json!({
                "path": response
                    .path
                    .clone()
                    .unwrap_or_default(),
                "content": response
                    .content
                    .clone()
                    .unwrap_or_default()
            })
        }

        "search" => {
            serde_json::json!({
                "pattern": response
                    .pattern
                    .clone()
                    .unwrap_or_default(),
                "path": response
                    .path
                    .clone()
                    .unwrap_or_else(|| ".".to_string())
            })
        }

        "shell" => {
            serde_json::json!({
                "command": response
                    .command
                    .clone()
                    .unwrap_or_default()
            })
        }

        _ => unreachable!(),
    };

    response.response_type = "tool".to_string();
    response.tool = Some(tool.to_string());
    response.arguments = Some(arguments);

    response
}

fn extract_code_block(text: &str) -> Option<&str> {
    let start = text.find("```json")?;
    let start = start + "```json".len();

    let remaining = &text[start..];

    let end = remaining.find("```")?;

    Some(remaining[..end].trim())
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;

    if start >= end {
        return None;
    }

    Some(&text[start..=end])
}

fn execute_tool(
    tool: &str,
    arguments: &Value,
) -> Result<String, Box<dyn std::error::Error>> {
    match tool {
        "read_file" => {
            let path = required_string(
                arguments,
                "path",
            )?;

            Ok(tools::read_file(path)?)
        }

        "write_file" => {
            let path = required_string(
                arguments,
                "path",
            )?;

            let content = required_string(
                arguments,
                "content",
            )?;

            Ok(tools::write_file(
                path,
                content,
            )?)
        }

        "list_files" => {
            let path = required_string(
                arguments,
                "path",
            )?;

            Ok(tools::list_files(path)?)
        }

        "search" => {
            let pattern = required_string(
                arguments,
                "pattern",
            )?;

            let path = required_string(
                arguments,
                "path",
            )?;

            Ok(tools::search(
                pattern,
                path,
            )?)
        }

        "shell" => {
            let command = required_string(
                arguments,
                "command",
            )?;

            println!(
                "\nCodey wants to execute:\n$ {command}"
            );

            println!("Executing automatically for MVP...");

            Ok(tools::shell(command)?)
        }

        _ => Err(
            format!("Unknown tool: `{tool}`")
                .into()
        ),
    }
}

fn required_string<'a>(
    arguments: &'a Value,
    name: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "Missing string argument `{name}`"
            )
            .into()
        })
}