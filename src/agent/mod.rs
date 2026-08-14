pub mod config;
pub mod prompt;
pub mod context;
pub mod session;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use session::Session;

use crate::tools;

const MAX_ITERATIONS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub(crate) role: String,
    pub(crate) content: String,
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

    #[serde(default)]
    path: Option<String>,

    #[serde(default)]
    pattern: Option<String>,

    #[serde(default)]
    command: Option<String>,
}

pub async fn run( // this is for codey -- run one liner
    user_prompt: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut session = Session::new();

    session.initialize();

    run_session(
        &mut session,
        user_prompt,
    )
    .await
}

pub async fn run_session ( // main tui
    session: &mut Session,
    user_prompt: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    session.add_user_message(user_prompt);

    let client = reqwest::Client::new();

    for iteration in 1..=MAX_ITERATIONS {
        println!(
            "\n--- Agent step {iteration} ---"
        );

        let response = call_model(
            &client,
            session,
        )
        .await?;

        let message = response
            .choices
            .first()
            .ok_or(
                "OpenRouter returned no choices"
            )?
            .message
            .clone();

        println!(
            "\nModel:\n{}",
            message.content
        );

        session.add_assistant_message(
            message.content.clone(),
        );

        let decision =
            parse_model_response(
                &message.content
            )?;

        match decision.response_type.as_str() {
            "tool" => {
                let tool_name = decision
                    .tool
                    .as_deref()
                    .ok_or(
                        "Tool response is missing `tool`"
                    )?;

                let arguments =
                    decision.arguments
                        .unwrap_or_else(|| {
                            Value::Object(
                                Default::default()
                            )
                        });

                println!(
                    "\nExecuting tool: {tool_name}"
                );

                let result =
                    execute_tool(
                        tool_name,
                        &arguments,
                    )?;

                println!(
                    "\nTool result:\n{result}"
                );

                session.add_tool_result(
                    tool_name,
                    &result,
                );
            }

            "final" => {
                return Ok(
                    decision
                        .content
                        .unwrap_or_else(
                            || "Done.".to_string()
                        )
                );
            }

            other => {
                return Err(
                    format!(
                        "Unknown response type: {other}"
                    )
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
    session: &Session,
) -> Result<OpenRouterResponse, Box<dyn std::error::Error>> {
    let config =
        config::Config::load()?;

    let payload =
        OpenRouterRequest {
            model: session.model().to_string(),
            messages: session
                .messages()
                .to_vec(),
        };

    let response = client
        .post(config.api_url())
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                config.api_key
            ),
        )
        .header(
            "Content-Type",
            "application/json",
        )
        .header(
            "X-OpenRouter-Title",
            "Codey",
        )
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status =
            response.status();

        let body =
            response.text().await?;

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

    if let Ok(response) =
        serde_json::from_str::<ModelResponse>(text)
    {
        return Ok(normalize_response(response));
    }

    if let Some(json) = extract_code_block(text) {
        if let Ok(response) =
            serde_json::from_str::<ModelResponse>(json)
        {
            return Ok(normalize_response(response));
        }
    }

    if let Some(json) = extract_json_object(text) {
        if let Ok(response) =
            serde_json::from_str::<ModelResponse>(json)
        {
            return Ok(normalize_response(response));
        }
    }

    if let Some(response) = parse_tool_call_format(text) {
        return Ok(response);
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

fn parse_tool_call_format(
    text: &str,
) -> Option<ModelResponse> {
    let start_marker = "<|tool_call_start|>";
    let end_marker = "<|tool_call_end|>";

    let start = text.find(start_marker)?;
    let end = text.find(end_marker)?;

    if end <= start {
        return None;
    }

    let call = text[
        start + start_marker.len()..end
    ]
    .trim();

    let call = call
        .strip_prefix('[')?
        .strip_suffix(']')?
        .trim();

    let open = call.find('(')?;
    let close = call.rfind(')')?;

    let tool = call[..open].trim();

    let arguments_text =
        &call[open + 1..close];

    let arguments =
        parse_python_style_arguments(arguments_text)?;

    Some(ModelResponse {
        response_type: "tool".to_string(),
        content: None,
        tool: Some(tool.to_string()),
        arguments: Some(arguments),
        path: None,
        pattern: None,
        command: None,
    })
}

fn parse_python_style_arguments(
    input: &str,
) -> Option<Value> {
    let mut object =
        serde_json::Map::new();

    let mut current = String::new();
    let mut parts = Vec::new();

    let mut quote = None;

    for character in input.chars() {
        match character {
            '\'' | '"' if quote.is_none() => {
                quote = Some(character);
                current.push(character);
            }

            '\'' | '"' if quote == Some(character) => {
                quote = None;
                current.push(character);
            }

            ',' if quote.is_none() => {
                parts.push(current.trim().to_string());
                current.clear();
            }

            _ => current.push(character),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    for part in parts {
        let (key, value) =
            part.split_once('=')?;

        let key = key.trim();

        let value = value.trim();

        let value = value
            .strip_prefix('\'')
            .and_then(|v| v.strip_suffix('\''))
            .or_else(|| {
                value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
            })?;

        object.insert(
            key.to_string(),
            Value::String(
                value.to_string()
            ),
        );
    }

    Some(Value::Object(object))
}