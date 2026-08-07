use super::config::BackendConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use super::TokenUsage;

/// AWS Bedrock backend, reached through the AWS SDK.
pub struct BedrockBackend {
    client: Client,
    /// Bedrock model identifier this instance invokes.
    pub model_id: String,
    /// Cache identity this instance was constructed under, when it came from
    /// [`create_backend`](super::create_backend). `None` for a hand-built
    /// instance, which is not in the cache and so reports unavailable.
    key: Option<super::BackendKey>,
}

#[derive(Serialize)]
struct BedrockRequest {
    anthropic_version: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Message {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub(crate) enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub(crate) enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub(crate) struct BedrockResponse {
    pub content: Vec<ResponseBlock>,
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<BedrockUsage>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct BedrockUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub(crate) enum ResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

impl BedrockBackend {
    /// Build the backend, loading AWS configuration from the ambient
    /// environment (profile, region, credentials).
    ///
    /// # Errors
    /// Returns an error when AWS configuration cannot be loaded or the config
    /// names no model.
    pub async fn new(config: &BackendConfig) -> Result<Self> {
        let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&aws_config);

        let model_id = config
            .model
            .clone()
            .unwrap_or_else(|| "us.anthropic.claude-sonnet-4-20250514-v1:0".to_string());

        Ok(Self {
            client,
            model_id,
            key: None,
        })
    }

    /// Attach the cache identity this instance was constructed under.
    ///
    /// Crate-internal on purpose: a public setter would let a caller stamp one
    /// instance with another entry's identity and read that entry's health back
    /// through [`Backend::is_available`](super::Backend::is_available).
    pub(crate) fn with_cache_key(mut self, key: super::BackendKey) -> Self {
        self.key = Some(key);
        self
    }

    #[allow(dead_code)]
    pub(crate) async fn invoke_with_messages(
        &self,
        system: Option<&str>,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<BedrockResponse> {
        self.invoke_with_messages_model(&self.model_id, system, messages, tools)
            .await
    }

    async fn invoke_with_messages_model(
        &self,
        model_id: &str,
        system: Option<&str>,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<BedrockResponse> {
        let request = BedrockRequest {
            anthropic_version: "bedrock-2023-05-31".to_string(),
            max_tokens: 4096,
            system: system.map(|s| s.to_string()),
            messages,
            tools,
        };

        let body = serde_json::to_vec(&request)?;

        let response = self
            .client
            .invoke_model()
            .model_id(model_id)
            .content_type("application/json")
            .body(Blob::new(body))
            .send()
            .await
            .context("Failed to invoke Bedrock model")?;

        let response_body = response.body.as_ref();
        let response: BedrockResponse =
            serde_json::from_slice(response_body).context("Failed to parse Bedrock response")?;

        Ok(response)
    }
}

#[async_trait]
impl super::Backend for BedrockBackend {
    fn name(&self) -> &str {
        "bedrock"
    }

    async fn query(
        &self,
        ctx: super::StepContext<'_>,
    ) -> std::result::Result<super::QueryOutput, super::BackendError> {
        let start = Instant::now();

        let messages = vec![Message {
            role: "user".to_string(),
            content: MessageContent::Text(ctx.prompt.to_string()),
        }];

        let effective_model_id = ctx
            .model
            .filter(|m| !m.is_empty())
            .unwrap_or(&self.model_id);

        let response = self
            .invoke_with_messages_model(effective_model_id, None, messages, None)
            .await
            .map_err(|err| {
                let msg = err.to_string();
                if msg.contains("RateExceeded") || msg.contains("ThrottlingException") {
                    super::BackendError::RateLimit {
                        message: msg,
                        retry_after_ms: None,
                    }
                } else if msg.contains("ValidationException") {
                    super::BackendError::Config { message: msg }
                } else if msg.contains("AccessDeniedException") {
                    super::BackendError::Auth { message: msg }
                } else if msg.contains("ModelNotReadyException")
                    || msg.contains("ServiceUnavailableException")
                {
                    super::BackendError::Unavailable { message: msg }
                } else {
                    super::BackendError::ExecutionFailed {
                        message: msg,
                        exit_code: None,
                    }
                }
            })?;

        let text = response
            .content
            .iter()
            .filter_map(|block| match block {
                ResponseBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let usage = response
            .usage
            .as_ref()
            .map(|u| TokenUsage::new(u.input_tokens, u.output_tokens));

        // Fall back to the requested effective model if Bedrock response omits model field.
        let model = response
            .model
            .clone()
            .or_else(|| Some(effective_model_id.to_string()));

        Ok(
            super::QueryOutput::from_text(text, "bedrock", start.elapsed())
                .with_model(model)
                .with_usage(usage),
        )
    }

    fn is_available(&self) -> bool {
        // Function reference rather than `|k| ...`: clippy's `redundant_closure`
        // is denied in CI, and the closure form trips it.
        self.key.as_ref().is_some_and(super::is_backend_available)
    }

    async fn health_check(&self) -> std::result::Result<super::HealthStatus, super::BackendError> {
        let available = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            || std::env::var("AWS_PROFILE").is_ok()
            || std::path::Path::new(&format!(
                "{}/.aws/credentials",
                std::env::var("HOME").unwrap_or_default()
            ))
            .exists();
        if available {
            Ok(super::HealthStatus::new_available())
        } else {
            Err(super::BackendError::Unavailable {
                message: format!("Bedrock backend '{}' is not available", self.name()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bedrock_response_deserialize_with_usage() {
        let json = r#"{
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "end_turn",
            "model": "us.anthropic.claude-sonnet-4-20250514-v1:0",
            "usage": {"input_tokens": 55, "output_tokens": 66}
        }"#;
        let parsed: BedrockResponse = serde_json::from_str(json).expect("should parse");
        assert_eq!(parsed.content.len(), 1);
        assert_eq!(
            parsed.model.as_deref(),
            Some("us.anthropic.claude-sonnet-4-20250514-v1:0")
        );
        let usage = parsed.usage.expect("usage should be present");
        assert_eq!(usage.input_tokens, 55);
        assert_eq!(usage.output_tokens, 66);
    }

    #[test]
    fn test_bedrock_response_deserialize_without_usage() {
        let json = r#"{
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "end_turn"
        }"#;
        let parsed: BedrockResponse = serde_json::from_str(json).expect("should parse");
        assert!(parsed.model.is_none());
        assert!(parsed.usage.is_none());
    }
}
