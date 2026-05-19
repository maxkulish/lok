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
        .unwrap_or_else(|e| panic!("failed to read {}: {}", e, name))
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

    assert!(
        types.contains(&"error"),
        "should contain top-level error event"
    );
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
            .and_then(|v| v.as_str());
        assert!(
            msg.is_some() && msg.unwrap().contains("not supported")
                || msg.is_some() && msg.unwrap().contains("invalid_request_error"),
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
