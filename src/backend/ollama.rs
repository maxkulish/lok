//! Ollama backend - HTTP API for local LLMs

use super::{Backend, TokenUsage};
use crate::config::BackendConfig;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaBackend {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Option<ChatMessage>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

impl OllamaBackend {
    pub fn new(config: &BackendConfig) -> Result<Self> {
        let base_url = config
            .command
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string());

        let client = Client::builder().build()?;

        Ok(Self {
            client,
            base_url,
            model,
        })
    }

    async fn chat(
        &self,
        prompt: &str,
        model_override: Option<&str>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError> {
        let effective_model = model_override
            .filter(|m| !m.is_empty())
            .unwrap_or(&self.model)
            .to_string();
        let request = ChatRequest {
            model: effective_model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            stream: false,
        };

        let start = std::time::Instant::now();
        let response = self
            .client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                let elapsed_ms = start.elapsed().as_millis() as u64;
                if e.is_timeout() {
                    super::BackendError::Timeout {
                        message: format!("Ollama request timed out: {}", e),
                        elapsed_ms,
                    }
                } else if e.is_connect() {
                    super::BackendError::Network {
                        message: format!("Ollama connection failed: {}", e),
                    }
                } else {
                    super::BackendError::Network {
                        message: format!("Ollama request failed: {}", e),
                    }
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            let msg = format!("Ollama error {}: {}", status, error_text);
            return Err(match status.as_u16() {
                429 => super::BackendError::RateLimit {
                    message: msg,
                    retry_after_ms: None,
                },
                _ => super::BackendError::ExecutionFailed {
                    message: msg,
                    exit_code: None,
                },
            });
        }

        let chat_response: ChatResponse =
            response
                .json()
                .await
                .map_err(|e| super::BackendError::Parse {
                    message: format!("Failed to parse Ollama response: {}", e),
                })?;

        let text = chat_response
            .message
            .map(|msg| msg.content)
            .unwrap_or_default();

        let usage = chat_response
            .prompt_eval_count
            .zip(chat_response.eval_count)
            .map(|(p, c)| TokenUsage::new(p, c));

        // Fall back to the requested effective model when the API response omits
        // the `model` field, so the output's model is always populated.
        let model = chat_response.model.or(Some(effective_model));

        Ok(
            super::QueryOutput::from_text(text, "ollama", start.elapsed())
                .with_model(model)
                .with_usage(usage),
        )
    }
}

#[derive(Deserialize)]
struct VersionResponse {
    version: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<super::ModelInfo>,
}

#[async_trait]
impl Backend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn query(
        &self,
        ctx: super::StepContext<'_>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError> {
        self.chat(ctx.prompt, ctx.model).await
    }

    fn is_available(&self) -> bool {
        super::Engine::is_backend_available(self.name())
    }

    async fn health_check(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
        let version_url = format!("{}/api/version", self.base_url);
        let version_res = match self
            .client
            .get(&version_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp,
            _ => return Ok(super::HealthStatus::new_unavailable()),
        };

        let version_data: VersionResponse = match version_res.json().await {
            Ok(data) => data,
            _ => return Ok(super::HealthStatus::new_unavailable()),
        };

        let tags_url = format!("{}/api/tags", self.base_url);
        let tags_res = match self
            .client
            .get(&tags_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp,
            _ => return Ok(super::HealthStatus::new_unavailable()),
        };

        let tags_data: TagsResponse = match tags_res.json().await {
            Ok(data) => data,
            _ => return Ok(super::HealthStatus::new_unavailable()),
        };

        Ok(super::HealthStatus {
            available: true,
            version: Some(version_data.version),
            mode: None,
            diagnostic: None,
            auth_method: None,
            capabilities: None,
            unusable_flags: Vec::new(),
            models: tags_data.models,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_response_deserialize_with_counts() {
        let json = r#"{
            "message": {"role": "assistant", "content": "hello"},
            "model": "llama3.2",
            "prompt_eval_count": 42,
            "eval_count": 17
        }"#;
        let parsed: ChatResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.model.as_deref(), Some("llama3.2"));
        assert_eq!(parsed.prompt_eval_count, Some(42));
        assert_eq!(parsed.eval_count, Some(17));
        assert_eq!(
            parsed.message.as_ref().map(|m| m.content.as_str()),
            Some("hello")
        );
    }

    #[test]
    fn test_ollama_response_deserialize_partial_counts() {
        // Only one of the two counts present - TokenUsage should NOT be constructed
        // because zip() returns None when either side is None.
        let json = r#"{
            "message": {"role": "assistant", "content": "hello"},
            "model": "llama3.2",
            "prompt_eval_count": 42
        }"#;
        let parsed: ChatResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.prompt_eval_count, Some(42));
        assert_eq!(parsed.eval_count, None);
        let usage_opt = parsed
            .prompt_eval_count
            .zip(parsed.eval_count)
            .map(|(p, c)| TokenUsage::new(p, c));
        assert!(usage_opt.is_none());
    }

    #[test]
    fn test_ollama_response_deserialize_without_model() {
        let json = r#"{
            "message": {"role": "assistant", "content": "hello"}
        }"#;
        let parsed: ChatResponse = serde_json::from_str(json).expect("should parse");
        assert!(parsed.model.is_none());
        assert!(parsed.prompt_eval_count.is_none());
        assert!(parsed.eval_count.is_none());
    }

    #[test]
    fn test_ollama_tags_deserialization() {
        let json = r#"{
            "models": [
                {
                    "name": "llama3:latest",
                    "model": "llama3:latest",
                    "modified_at": "2024-05-16T15:22:20.558Z",
                    "size": 4200000000,
                    "digest": "sha256:abc123xyz"
                }
            ]
        }"#;
        let parsed: TagsResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.models.len(), 1);
        let model = &parsed.models[0];
        assert_eq!(model.name, "llama3:latest");
        assert_eq!(
            model.modified_at.as_deref(),
            Some("2024-05-16T15:22:20.558Z")
        );
        assert_eq!(model.size, Some(4200000000));
        assert_eq!(model.digest.as_deref(), Some("sha256:abc123xyz"));
    }

    #[tokio::test]
    async fn test_ollama_health_check_connection_refused() {
        let config = BackendConfig {
            enabled: true,
            command: Some("http://127.0.0.1:54321".to_string()),
            ..Default::default()
        };
        let backend = OllamaBackend::new(&config).expect("should construct");
        let health = backend.health_check().await.expect("should not error");
        assert!(!health.available);
    }
}
