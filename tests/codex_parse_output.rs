//! Integration-level tests for Codex JSONL fixtures.
//!
//! # Binary-crate note
//!
//! This crate is binary-only (no `lib.rs`), so integration tests cannot import
//! internal items like `parse_jsonl_stream` from `src/backend/`. The functional
//! unit tests for the parser live in `src/backend/codex_event.rs` under the
//! `#[cfg(test)]` module.
//!
//! This file validates fixture structure and event ordering using the
//! `serde_json::Value` API, matching the approach used by `codex_fixtures.rs`.
//!
//! If the crate ever grows a `lib.rs`, these tests should be migrated to import
//! `parse_jsonl_stream` directly and replaced with calls to the actual parser.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex")
}

fn load_fixture(name: &str) -> String {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|error| panic!("failed to read {}: {}", name, error))
}

#[derive(Debug)]
struct ParsedStreamResult {
    terminal_error: Option<String>,
    parse_error: Option<String>,
    agent_message: Option<String>,
    usage: Option<Value>,
}

/// Emulate the pre-PR JSONL extraction semantics in this integration layer.
fn parse_jsonl_for_output(stream: &str) -> ParsedStreamResult {
    let mut current_turn_agent: Option<String> = None;
    let mut last_completed_agent: Option<String> = None;
    let mut last_completed_usage: Option<Value> = None;

    for line in stream.lines().filter(|line| !line.trim().is_empty()) {
        let event = serde_json::from_str::<Value>(line)
            .unwrap_or_else(|error| panic!("failed to parse JSONL line {line:?}: {error}"));

        let event_type = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("missing event type in line {line:?}"));

        match event_type {
            "turn.started" => {
                current_turn_agent = None;
            }
            "item.completed"
                if event
                    .get("item")
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("agent_message") =>
            {
                if let Some(text) = event
                    .get("item")
                    .and_then(|item| item.get("text"))
                    .and_then(Value::as_str)
                {
                    current_turn_agent = Some(text.to_string());
                }
            }
            "turn.completed" => {
                last_completed_agent = current_turn_agent.clone();
                last_completed_usage = event.get("usage").cloned();
                current_turn_agent = None;
            }
            "turn.failed" => {
                let message = event
                    .get("error")
                    .and_then(|err| err.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| event.get("error").and_then(Value::as_str))
                    .unwrap_or("Codex turn failed");
                return ParsedStreamResult {
                    terminal_error: Some(message.to_string()),
                    parse_error: None,
                    agent_message: None,
                    usage: None,
                };
            }
            "error" => {
                let message = event
                    .get("message")
                    .and_then(Value::as_str)
                    .or_else(|| event.get("error").and_then(Value::as_str))
                    .unwrap_or("Codex error event");
                return ParsedStreamResult {
                    terminal_error: Some(message.to_string()),
                    parse_error: None,
                    agent_message: None,
                    usage: None,
                };
            }
            _ => {}
        }
    }

    if last_completed_agent.is_none() && last_completed_usage.is_none() {
        ParsedStreamResult {
            terminal_error: None,
            parse_error: Some("Codex JSONL stream ended without turn.completed".to_string()),
            agent_message: None,
            usage: None,
        }
    } else if last_completed_agent.is_none() {
        ParsedStreamResult {
            terminal_error: None,
            parse_error: Some("turn.completed without agent_message".to_string()),
            agent_message: None,
            usage: last_completed_usage,
        }
    } else {
        ParsedStreamResult {
            terminal_error: None,
            parse_error: None,
            agent_message: last_completed_agent,
            usage: last_completed_usage,
        }
    }
}

fn pick_output(last_message_text: &str, parsed: &ParsedStreamResult) -> Result<String, String> {
    if let Some(err) = &parsed.terminal_error {
        return Err(err.clone());
    }

    let trimmed_last_message = last_message_text.trim_end_matches(&['\n', '\r'][..]);

    if !trimmed_last_message.trim().is_empty() {
        return Ok(trimmed_last_message.to_string());
    }

    if let Some(message) = &parsed.agent_message {
        return Ok(message.to_string());
    }

    Err(parsed
        .parse_error
        .clone()
        .unwrap_or_else(|| "Codex completed without output".to_string()))
}

/// Parse a JSONL stream into events, skipping blank lines.
fn parse_events(stream: &str) -> Vec<Value> {
    stream
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect()
}

#[test]
fn turn_completed_fixture_has_valid_structure() {
    let stream = load_fixture("turn-completed.jsonl");
    let events = parse_events(&stream);

    let types: Vec<&str> = events.iter().filter_map(|e| e["type"].as_str()).collect();
    assert_eq!(
        types,
        &[
            "thread.started",
            "turn.started",
            "item.completed",
            "turn.completed"
        ]
    );

    let last = events.last().unwrap();
    assert!(
        last["usage"].is_object(),
        "turn.completed should have usage"
    );
    assert_eq!(last["usage"]["input_tokens"].as_u64(), Some(23057));
    assert_eq!(last["usage"]["output_tokens"].as_u64(), Some(7));
}

#[test]
fn multi_turn_reasoning_fixture_has_command_execution_items() {
    let stream = load_fixture("multi-turn-reasoning.jsonl");
    let events = parse_events(&stream);

    let types: Vec<&str> = events.iter().filter_map(|e| e["type"].as_str()).collect();
    assert!(
        types.contains(&"item.completed"),
        "should contain item.completed events"
    );

    // Count items that have agent_message or command_execution
    let agent_msgs: Vec<&Value> = events
        .iter()
        .filter(|e| {
            e["type"] == "item.completed" && e["item"]["type"].as_str() == Some("agent_message")
        })
        .collect();
    assert!(
        !agent_msgs.is_empty(),
        "should have at least one agent_message"
    );

    // Last event should be turn.completed with usage
    let last = events.last().unwrap();
    assert_eq!(last["type"], "turn.completed");
    assert!(last["usage"].is_object());
}

#[test]
fn turn_failed_fixture_has_error_then_failed() {
    let stream = load_fixture("turn-failed.jsonl");
    let events = parse_events(&stream);

    let types: Vec<&str> = events.iter().filter_map(|e| e["type"].as_str()).collect();

    assert!(types.contains(&"error"), "should contain top-level error");
    assert!(
        types.contains(&"turn.failed"),
        "should contain turn.failed event"
    );

    // Verify the error message is present in the JSONL
    let error_events: Vec<&Value> = events
        .iter()
        .filter(|e| e["type"] == "error" || e["type"] == "turn.failed")
        .collect();
    for ev in &error_events {
        let msg = ev
            .pointer("/error/message")
            .or_else(|| ev.get("message"))
            .and_then(Value::as_str);
        assert!(
            (msg.is_some() && msg.unwrap().contains("not supported"))
                || (msg.is_some() && msg.unwrap().contains("invalid_request_error")),
            "error event should contain a failure message, got: {:?}",
            msg
        );
    }
}

#[test]
fn missing_agent_message_fixture_ends_turn_without_agent() {
    let stream = load_fixture("missing-agent-message.jsonl");
    let events = parse_events(&stream);

    let last = events.last().unwrap();
    assert_eq!(last["type"], "turn.completed");

    // No item.completed with agent_message type
    let agent_msgs: Vec<&Value> = events
        .iter()
        .filter(|e| {
            e["type"] == "item.completed" && e["item"]["type"].as_str() == Some("agent_message")
        })
        .collect();
    assert!(
        agent_msgs.is_empty(),
        "missing-agent-message fixture should NOT have agent_message items"
    );
}

#[test]
fn output_prefers_last_message_file_when_present() {
    let stream = load_fixture("turn-completed.jsonl");
    let parsed = parse_jsonl_for_output(&stream);
    let last_message = load_fixture("turn-completed.last-message.txt");

    let selected = pick_output(&last_message, &parsed).expect("should select an output");
    assert_eq!(selected, "fixture happy path (from -o file)");

    assert!(
        parsed.usage.is_some(),
        "valid turn-completed fixture must carry usage"
    );
}

#[test]
fn output_prefers_last_message_when_jsonl_missing_agent_message() {
    let stream = load_fixture("missing-agent-message.jsonl");
    let parsed = parse_jsonl_for_output(&stream);
    let last_message = load_fixture("missing-agent-message.last-message.txt");

    let selected = pick_output(&last_message, &parsed).expect("should select last-message text");
    assert_eq!(selected, "fallback from last message file");

    let usage = parsed
        .usage
        .expect("usage should still be parsed from JSONL");
    assert_eq!(usage["input_tokens"].as_u64(), Some(23057));
}

#[test]
fn output_falls_back_to_jsonl_when_last_message_missing_or_empty() {
    let stream = load_fixture("multi-turn-reasoning.jsonl");
    let parsed = parse_jsonl_for_output(&stream);
    let last_message = load_fixture("multi-turn-reasoning.last-message.txt");

    let selected = pick_output(&last_message, &parsed).expect("should parse from JSONL");
    assert_eq!(selected, "323");
}

#[test]
fn output_reports_terminal_error_when_last_message_populated_but_turn_failed() {
    let stream = load_fixture("turn-failed.jsonl");
    let parsed = parse_jsonl_for_output(&stream);
    let last_message = load_fixture("turn-failed.last-message.txt");

    let selected = pick_output(&last_message, &parsed);
    assert!(
        selected.is_err(),
        "terminal JSONL failure should win over last message"
    );
    let message = selected.expect_err("expect terminal failure");

    assert!(
        message.contains("not supported") || message.contains("invalid_request_error"),
        "terminal error should be preserved as failure, got: {message}"
    );
}
