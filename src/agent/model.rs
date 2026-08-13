use serde::{Deserialize, Serialize};
use crate::agent::config;
use crate::agent::prompt;

#[derive(Serialize, Deserialize)]
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

#[derive(Deserialize, Debug)]
struct ModelDecision {
    action: String,
    path: Option<String>,
    content: String,
}

pub async fn ask_model(user_prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
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

    let client = reqwest::Client::new();

    let payload = OpenRouterRequest {
        model: config::MODEL_NAME.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: prompt::SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ],
    };

    let res = client
        .post(config::API_URL)
        .header("Authorization", format!("Bearer {}", config::API_KEY))
        .header("Content-Type", "application/json")
        .header("X-OpenRouter-Title", "Codey Rust App") 
        .json(&payload)
        .send()
        .await?;

    if !res.status().is_success() {
        let status = res.status();
        let err_body = res.text().await?;
        return Err(format!("API Error Status ({}): {}", status, err_body).into());
    }

    let api_response: OpenRouterResponse = res.json().await?;

    if let Some(choice) = api_response.choices.first() {
        Ok(choice.message.content.clone())
    } else {
        Err("Received an empty response array from OpenRouter backend.".into())
    }
}
