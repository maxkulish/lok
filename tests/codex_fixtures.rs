use serde_json::Value;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

const KNOWN_EVENT_TYPES: &[&str] = &[
    "thread.started",
    "turn.started",
    "turn.completed",
    "turn.failed",
    "item.started",
    "item.completed",
    "error",
];
const TERMINAL_EVENT_TYPES: &[&str] = &["turn.completed", "turn.failed", "error"];
const MAX_FIXTURE_BYTES: u64 = 20_000;
const MAX_CORPUS_BYTES: u64 = 50_000;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex")
}

fn jsonl_fixture_paths() -> Vec<PathBuf> {
    let mut paths = fs::read_dir(fixtures_dir())
        .expect("read tests/fixtures/codex")
        .map(|entry| entry.expect("read fixture dir entry").path())
        .filter(|path| path.extension() == Some(OsStr::new("jsonl")))
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    paths
}

fn load_fixture(name: &str) -> String {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|error| panic!("failed to read {name}: {error}"))
}

fn parse_jsonl(name: &str, stream: &str) -> Vec<Value> {
    let events = stream
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            let event = serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("{name}: line {} is not valid JSON: {error}", index + 1));
            let event_type = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{name}: line {} missing string type", index + 1));
            assert!(
                KNOWN_EVENT_TYPES.contains(&event_type),
                "{name}: line {} has unknown event type {event_type:?}",
                index + 1
            );
            event
        })
        .collect::<Vec<_>>();

    assert!(!events.is_empty(), "{name}: fixture must contain events");
    events
}

fn event_type(event: &Value) -> &str {
    event
        .get("type")
        .and_then(Value::as_str)
        .expect("event type already validated")
}

fn item_type(event: &Value) -> Option<&str> {
    event
        .get("item")
        .and_then(|item| item.get("type"))
        .and_then(Value::as_str)
}

fn is_agent_message(event: &Value) -> bool {
    event_type(event) == "item.completed" && item_type(event) == Some("agent_message")
}

fn has_non_agent_completed_item(event: &Value) -> bool {
    event_type(event) == "item.completed" && item_type(event) != Some("agent_message")
}

fn assert_terminal_event(name: &str, events: &[Value]) {
    let last_type = event_type(events.last().expect("events are non-empty"));
    assert!(
        TERMINAL_EVENT_TYPES.contains(&last_type),
        "{name}: final event must be terminal, got {last_type:?}"
    );
}

fn assert_no_unscrubbed_sensitive_text(name: &str, stream: &str) {
    let forbidden_literals = [
        "/Users/",
        "/home/",
        "C:\\Users\\",
        "$HOME",
        "/tmp/",
        "/var/folders/",
        "Bearer ",
    ];

    for literal in forbidden_literals {
        assert!(
            !stream.contains(literal),
            "{name}: fixture contains unsanitized sensitive marker {literal:?}"
        );
    }

    for lower_line in stream.lines().map(str::to_ascii_lowercase) {
        for marker in [
            "api_key",
            "api-key",
            "apikey",
            "access_token",
            "auth_token",
            "secret=",
            "secret\":",
            "password=",
            "password\":",
        ] {
            assert!(
                !lower_line.contains(marker),
                "{name}: fixture contains possible credential marker {marker:?}"
            );
        }
    }

    assert!(
        !stream.lines().any(looks_like_email),
        "{name}: fixture contains an unredacted email-looking string"
    );
}

fn looks_like_email(line: &str) -> bool {
    line.split(|character: char| character.is_whitespace() || character == '"' || character == '\'')
        .any(|word| {
            let Some((local, domain)) = word.split_once('@') else {
                return false;
            };
            !local.is_empty()
                && domain.contains('.')
                && domain
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        })
}

#[test]
fn every_fixture_is_line_valid_jsonl() {
    let paths = jsonl_fixture_paths();
    assert_eq!(paths.len(), 4, "expected the four FR-40 Codex fixtures");

    for path in paths {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .expect("fixture path has utf-8 filename");
        let stream = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {name}: {error}"));
        let events = parse_jsonl(name, &stream);
        assert_terminal_event(name, &events);
    }
}

#[test]
fn fixtures_do_not_exceed_reviewable_size_caps() {
    let mut corpus_bytes = 0;

    for path in jsonl_fixture_paths() {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .expect("fixture path has utf-8 filename");
        let bytes = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("failed to stat {name}: {error}"))
            .len();
        assert!(
            bytes <= MAX_FIXTURE_BYTES,
            "{name}: fixture has {bytes} bytes, above {MAX_FIXTURE_BYTES} byte cap"
        );
        corpus_bytes += bytes;
    }

    assert!(
        corpus_bytes <= MAX_CORPUS_BYTES,
        "Codex fixture corpus has {corpus_bytes} bytes, above {MAX_CORPUS_BYTES} byte cap"
    );
}

#[test]
fn fixtures_do_not_contain_obvious_sensitive_text() {
    for path in jsonl_fixture_paths() {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .expect("fixture path has utf-8 filename");
        let stream = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {name}: {error}"));
        assert_no_unscrubbed_sensitive_text(name, &stream);
    }
}

#[test]
fn turn_completed_fixture_is_valid_jsonl_with_agent_message() {
    let stream = load_fixture("turn-completed.jsonl");
    let events = parse_jsonl("turn-completed.jsonl", &stream);

    assert_eq!(
        event_type(events.last().expect("events are non-empty")),
        "turn.completed"
    );
    assert!(
        events.iter().any(|event| {
            is_agent_message(event)
                && event
                    .get("item")
                    .and_then(|item| item.get("text"))
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty())
        }),
        "turn-completed.jsonl must include a non-empty final agent_message"
    );
}

#[test]
fn turn_failed_fixture_terminates_in_failure() {
    let stream = load_fixture("turn-failed.jsonl");
    let events = parse_jsonl("turn-failed.jsonl", &stream);
    let terminal = events.last().expect("events are non-empty");
    let terminal_type = event_type(terminal);

    assert!(
        ["turn.failed", "error"].contains(&terminal_type),
        "turn-failed.jsonl must end in turn.failed or error"
    );
    assert!(
        terminal.get("error").is_some() || terminal.get("message").is_some(),
        "turn-failed.jsonl terminal event must carry error details"
    );
}

#[test]
fn multi_turn_reasoning_fixture_reports_reasoning_tokens() {
    let stream = load_fixture("multi-turn-reasoning.jsonl");
    let events = parse_jsonl("multi-turn-reasoning.jsonl", &stream);
    let completed = events
        .iter()
        .rev()
        .find(|event| event_type(event) == "turn.completed")
        .expect("multi-turn-reasoning.jsonl has turn.completed");
    let reasoning_tokens = completed
        .get("usage")
        .and_then(|usage| usage.get("reasoning_output_tokens"))
        .and_then(Value::as_u64)
        .expect("turn.completed usage has reasoning_output_tokens");

    assert!(
        reasoning_tokens > 0,
        "multi-turn-reasoning.jsonl must report non-zero reasoning_output_tokens"
    );
    assert!(
        events.iter().any(has_non_agent_completed_item),
        "multi-turn-reasoning.jsonl must include an intermediate non-agent item.completed event"
    );
}

#[test]
fn missing_agent_message_fixture_has_no_agent_message_item() {
    let stream = load_fixture("missing-agent-message.jsonl");
    let events = parse_jsonl("missing-agent-message.jsonl", &stream);

    assert_eq!(
        event_type(events.last().expect("events are non-empty")),
        "turn.completed"
    );
    assert!(
        !events.iter().any(is_agent_message),
        "missing-agent-message.jsonl must not contain item.completed agent_message events"
    );
}
