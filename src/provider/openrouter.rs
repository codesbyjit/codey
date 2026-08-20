//! Uses our text-protocol (model returns JSON in the message) instead of native tool calling, for robustness across free models.

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_stream::StreamExt;

use super::{
    parse_decision, CompletionRequest, CompletionResult, DeltaSink, FinishDecision, Provider,
    ProviderError, StreamDelta,
};

const TITLE_HEADER: &str = "Codey";

pub struct OpenRouterProvider {
    provider: String,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenRouterProvider {
    pub fn new(provider: String, api_key: String, base_url: String, model: String) -> Self {
        Self {
            provider,
            api_key,
            base_url,
            model,
        }
    }

    fn build_body(&self, req: &CompletionRequest) -> Value {
        let mut body = json!({
            "model": req.model,
            "messages": req.messages,
            "temperature": req.temperature,
        });
        if let Some(max_tokens) = req.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        body
    }

    async fn post(&self, body: Value, stream: bool) -> Result<reqwest::Response, ProviderError> {
        let mut body = body;
        body["stream"] = json!(stream);

        let client = reqwest::Client::new();
        let response = client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .header("X-OpenRouter-Title", TITLE_HEADER)
            .header("X-Title", TITLE_HEADER)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ProviderError::Status {
                status,
                body: body_text,
            });
        }

        Ok(response)
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    fn name(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResult, ProviderError> {
        let body = self.build_body(&req);
        let response = self.post(body, false).await?;
        let parsed: Value = response
            .json()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;

        let content = parsed
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| ProviderError::Malformed("no content in provider response".into()))?
            .to_string();

        let decision = parse_decision(&content)?;
        Ok(CompletionResult {
            decision,
            raw: content,
        })
    }

    async fn stream(
        &self,
        req: CompletionRequest,
        deltas: DeltaSink,
    ) -> Result<CompletionResult, ProviderError> {
        let _ = deltas.send(StreamDelta::Status("contacting model…".into()));

        let body = self.build_body(&req);
        let response = self.post(body, true).await?;

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut extractor = crate::provider::StreamingAnswerExtractor::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProviderError::Transport(e.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = buffer.find('\n') {
                let line: String = buffer.drain(..=newline_pos).collect();
                let line = line.trim();
                if !line.starts_with("data:") {
                    continue;
                }
                let payload = line["data:".len()..].trim();
                if payload == "[DONE]" {
                    break;
                }
                if let Ok(value) = serde_json::from_str::<Value>(payload) {
                    if let Some(text) = value
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !text.is_empty() {
                            content.push_str(text);
                            let live = extractor.push(text);
                            if !live.is_empty() {
                                let _ = deltas.send(StreamDelta::Text(live));
                            }
                        }
                    }
                }
            }
        }

        let decision = parse_decision(&content)?;
        Ok(CompletionResult {
            decision,
            raw: content,
        })
    }
}

#[allow(dead_code)]
pub(crate) fn decision(text: &str) -> FinishDecision {
    parse_decision(text).unwrap()
}
