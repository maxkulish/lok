use crate::backend::{BackendError, TokenUsage};
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum CodexEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: String },

    #[serde(rename = "turn.started")]
    TurnStarted,

    #[serde(rename = "turn.completed")]
    TurnCompleted {
        #[serde(default)]
        usage: Option<CodexUsage>,
    },

    #[serde(rename = "turn.failed")]
    TurnFailed {
        #[serde(default)]
        error: Option<CodexError>,
    },

    #[serde(rename = "item.started")]
    ItemStarted {
        #[serde(default)]
        item: Option<CodexItem>,
    },

    #[serde(rename = "item.completed")]
    ItemCompleted {
        #[serde(default)]
        item: Option<CodexItem>,
    },

    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        message: Option<String>,
    },

    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum CodexItem {
    #[serde(rename = "agent_message")]
    AgentMessage {
        #[serde(default)]
        text: Option<String>,
    },

    #[serde(other)]
    Other,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(super) struct CodexUsage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub cached_input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub reasoning_output_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct CodexError {
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ParsedTurn {
    pub agent_message: Option<String>,
    pub usage: Option<TokenUsage>,
}

/// Parse a Codex JSONL stream and extract the final agent_message text from the last
/// `turn.completed` event. Returns `BackendError` on `turn.failed`, stream-level `error`,
/// or when no `turn.completed` is found.
pub(crate) fn parse_jsonl_stream(stream: &str) -> Result<ParsedTurn, BackendError> {
    let mut last_completed_turn: ParsedTurn = ParsedTurn::default();
    let mut current_turn_agent_message: Option<String> = None;

    for line in stream.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let event: CodexEvent = match serde_json::from_str(trimmed) {
            Ok(event) => event,
            Err(_e) => {
                // Skipping unparseable JSONL line
                continue;
            }
        };

        match event {
            CodexEvent::TurnStarted => {
                // Reset per-turn accumulator
                current_turn_agent_message = None;
            }
            CodexEvent::ItemCompleted { item } => {
                if let Some(CodexItem::AgentMessage { text }) = item {
                    current_turn_agent_message = text;
                }
                // Non-agent_message items are silently ignored
            }
            CodexEvent::ItemStarted { .. } => {
                // No-op; we only care about item.completed for extraction
            }
            CodexEvent::TurnCompleted { usage } => {
                let token_usage = usage.map(|u| TokenUsage::new(u.input_tokens, u.output_tokens));
                last_completed_turn = ParsedTurn {
                    agent_message: current_turn_agent_message.clone(),
                    usage: token_usage,
                };
                // Reset for potential next turn
                current_turn_agent_message = None;
            }
            CodexEvent::TurnFailed { error } => {
                let msg = error
                    .and_then(|e| e.message)
                    .unwrap_or_else(|| "Codex turn failed".to_string());
                return Err(BackendError::ExecutionFailed {
                    message: msg,
                    exit_code: None,
                });
            }
            CodexEvent::Error { message } => {
                let msg = message.unwrap_or_else(|| "Codex error event".to_string());
                return Err(BackendError::ExecutionFailed {
                    message: msg,
                    exit_code: None,
                });
            }
            CodexEvent::ThreadStarted { .. } | CodexEvent::Unknown => {
                // Skipping non-extraction event
            }
        }
    }

    // If we never saw a turn.completed, that's a parse failure
    if last_completed_turn.agent_message.is_none() && last_completed_turn.usage.is_none() {
        return Err(BackendError::Parse {
            message: "Codex JSONL stream ended without turn.completed event".to_string(),
        });
    }

    // turn.completed observed but no agent_message for that turn
    if last_completed_turn.agent_message.is_none() {
        return Err(BackendError::Parse {
            message: "turn.completed without agent_message".to_string(),
        });
    }

    Ok(last_completed_turn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_thread_started() {
        let s = r#"{"type":"thread.started","thread_id":"abc123"}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        assert!(matches!(e, CodexEvent::ThreadStarted { thread_id } if thread_id == "abc123"));
    }

    #[test]
    fn parses_turn_completed_with_usage() {
        let s = r#"{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":5,"output_tokens":20,"reasoning_output_tokens":3}}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        if let CodexEvent::TurnCompleted { usage: Some(u) } = e {
            assert_eq!(u.input_tokens, 10);
            assert_eq!(u.cached_input_tokens, 5);
            assert_eq!(u.output_tokens, 20);
            assert_eq!(u.reasoning_output_tokens, 3);
        } else {
            panic!("expected TurnCompleted with usage, got {:?}", e);
        }
    }

    #[test]
    fn parses_turn_failed_with_error_message() {
        let s = r#"{"type":"turn.failed","error":{"message":"model not supported"}}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        assert!(
            matches!(e, CodexEvent::TurnFailed { error: Some(ref err) } if err.message.as_deref() == Some("model not supported"))
        );
    }

    #[test]
    fn parses_item_completed_agent_message() {
        let s = r#"{"type":"item.completed","item":{"type":"agent_message","text":"hello"}}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        if let CodexEvent::ItemCompleted {
            item: Some(CodexItem::AgentMessage { text: Some(t) }),
        } = e
        {
            assert_eq!(t, "hello");
        } else {
            panic!("expected AgentMessage, got {:?}", e);
        }
    }

    #[test]
    fn parses_item_completed_other_kind_as_other() {
        let s = r#"{"type":"item.completed","item":{"type":"command_execution","command":"pwd"}}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        assert!(
            matches!(
                e,
                CodexEvent::ItemCompleted {
                    item: Some(CodexItem::Other)
                }
            ),
            "expected Other, got {:?}",
            e
        );
    }

    #[test]
    fn unknown_event_type_falls_to_unknown() {
        let s = r#"{"type":"future.event","foo":"bar"}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        assert!(
            matches!(e, CodexEvent::Unknown),
            "expected Unknown, got {:?}",
            e
        );
    }

    #[test]
    fn unknown_item_type_falls_to_other() {
        let s = r#"{"type":"item.completed","item":{"type":"future_item"}}"#;
        let e: CodexEvent = serde_json::from_str(s).unwrap();
        assert!(
            matches!(
                e,
                CodexEvent::ItemCompleted {
                    item: Some(CodexItem::Other)
                }
            ),
            "expected ItemCompleted with Other, got {:?}",
            e
        );
    }

    #[test]
    fn happy_path_single_turn() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"fixture happy path"}}
{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":7}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.agent_message, Some("fixture happy path".to_string()));
        assert_eq!(result.usage, Some(TokenUsage::new(10, 7)));
    }

    #[test]
    fn multi_turn_returns_last_agent_message() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"first turn"}}
{"type":"turn.completed","usage":{"input_tokens":5,"output_tokens":3}}
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"second turn"}}
{"type":"turn.completed","usage":{"input_tokens":8,"output_tokens":4}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.agent_message, Some("second turn".to_string()));
        assert_eq!(result.usage, Some(TokenUsage::new(8, 4)));
    }

    #[test]
    fn turn_failed_returns_execution_failed() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"error","message":"model is not supported"}
{"type":"turn.failed","error":{"message":"model is not supported"}}"#;
        let err = parse_jsonl_stream(stream).unwrap_err();
        assert!(
            matches!(
                err,
                BackendError::ExecutionFailed {
                    exit_code: None,
                    ..
                }
            ),
            "expected ExecutionFailed, got {:?}",
            err
        );
        let msg = match err {
            BackendError::ExecutionFailed { message, .. } => message,
            _ => unreachable!(),
        };
        assert!(msg.contains("not supported"), "error message was: {}", msg);
    }

    #[test]
    fn top_level_error_returns_execution_failed() {
        let stream = r#"{"type":"error","message":"internal server error"}"#;
        let err = parse_jsonl_stream(stream).unwrap_err();
        assert!(
            matches!(
                err,
                BackendError::ExecutionFailed {
                    exit_code: None,
                    ..
                }
            ),
            "expected ExecutionFailed, got {:?}",
            err
        );
    }

    #[test]
    fn missing_agent_message_returns_parse_error() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":7}}"#;
        let err = parse_jsonl_stream(stream).unwrap_err();
        assert!(
            matches!(err, BackendError::Parse { .. }),
            "expected Parse, got {:?}",
            err
        );
        let msg = match err {
            BackendError::Parse { message } => message,
            _ => unreachable!(),
        };
        assert!(
            msg.contains("without agent_message"),
            "error message was: {}",
            msg
        );
    }

    #[test]
    fn truncated_stream_returns_parse_error() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"incomplete"}}"#;
        let err = parse_jsonl_stream(stream).unwrap_err();
        assert!(
            matches!(err, BackendError::Parse { .. }),
            "expected Parse, got {:?}",
            err
        );
    }

    #[test]
    fn unparseable_line_skipped_then_succeeds() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
not valid json
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"hello"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.agent_message, Some("hello".to_string()));
    }

    #[test]
    fn multiple_agent_messages_in_turn_returns_last() {
        let stream = r#"{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"first"}}
{"type":"item.completed","item":{"type":"agent_message","text":"second"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.agent_message, Some("second".to_string()));
    }

    #[test]
    fn item_started_does_not_affect_extraction() {
        let stream = r#"{"type":"turn.started"}
{"type":"item.started","item":{"type":"agent_message"}}
{"type":"item.completed","item":{"type":"agent_message","text":"msg"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.agent_message, Some("msg".to_string()));
    }

    #[test]
    fn usage_extracted_to_token_usage() {
        let stream = r#"{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"msg"}}
{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.usage, Some(TokenUsage::new(100, 50)));
    }

    #[test]
    fn unknown_event_in_stream_is_skipped() {
        let stream = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"future.unknown","data":"whatever"}
{"type":"turn.started"}
{"type":"item.completed","item":{"type":"agent_message","text":"hello"}}
{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#;
        let result = parse_jsonl_stream(stream).unwrap();
        assert_eq!(result.agent_message, Some("hello".to_string()));
    }

    #[test]
    fn empty_stream_returns_parse_error() {
        let err = parse_jsonl_stream("").unwrap_err();
        assert!(matches!(err, BackendError::Parse { .. }));
    }

    #[test]
    fn fixture_turn_completed_returns_happy_path_message() {
        let stream = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/codex/turn-completed.jsonl"
        ))
        .expect("turn-completed.jsonl exists");
        let result = parse_jsonl_stream(&stream).expect("turn-completed should parse");
        assert_eq!(result.agent_message, Some("fixture happy path".to_string()));
        let usage = result.usage.expect("turn.completed should have usage");
        assert_eq!(usage.prompt_tokens, 23057);
        assert_eq!(usage.completion_tokens, 7);
    }

    #[test]
    fn fixture_multi_turn_reasoning_returns_only_final_agent_message() {
        let stream = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/codex/multi-turn-reasoning.jsonl"
        ))
        .expect("multi-turn-reasoning.jsonl exists");
        let result = parse_jsonl_stream(&stream).expect("multi-turn-reasoning should parse");
        assert_eq!(result.agent_message, Some("323".to_string()));
        let usage = result.usage.expect("turn.completed should have usage");
        assert_eq!(usage.prompt_tokens, 46243);
        assert_eq!(usage.completion_tokens, 95);
    }

    #[test]
    fn fixture_turn_failed_returns_execution_failed() {
        let stream = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/codex/turn-failed.jsonl"
        ))
        .expect("turn-failed.jsonl exists");
        let err = parse_jsonl_stream(&stream).expect_err("turn-failed should fail");
        let msg = match &err {
            BackendError::ExecutionFailed { message, .. } => message.clone(),
            other => panic!("expected ExecutionFailed, got {:?}", other),
        };
        assert!(
            msg.contains("not supported") || msg.contains("invalid_request_error"),
            "error message should contain model failure details, got: {}",
            msg
        );
    }

    #[test]
    fn fixture_missing_agent_message_returns_parse_error() {
        let stream = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/codex/missing-agent-message.jsonl"
        ))
        .expect("missing-agent-message.jsonl exists");
        let err = parse_jsonl_stream(&stream).expect_err("missing-agent-message should fail");
        assert!(
            matches!(err, BackendError::Parse { .. }),
            "expected Parse error, got {:?}",
            err
        );
    }
}
