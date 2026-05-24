use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

const MAX_FIXTURE_BYTES: u64 = 20_000;
const MAX_CORPUS_BYTES: u64 = 50_000;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gemini")
}

fn json_fixture_paths() -> Vec<PathBuf> {
    let mut paths = fs::read_dir(fixtures_dir())
        .expect("read tests/fixtures/gemini")
        .map(|entry| entry.expect("read fixture dir entry").path())
        .filter(|path| path.extension() == Some(OsStr::new("json")))
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    paths
}

fn load_fixture(name: &str) -> String {
    fs::read_to_string(fixtures_dir().join(name))
        .unwrap_or_else(|error| panic!("failed to read {name}: {error}"))
}

fn parse_json_values(content: &str) -> Vec<serde_json::Value> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(content) {
        return vec![value];
    }

    content
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(value) => Some(value),
                Err(error) => panic!("invalid JSON fixture line {}: {error}", idx + 1),
            }
        })
        .collect()
}

fn has_user_visible_text(value: &serde_json::Value) -> bool {
    value.get("response").is_some()
        || value.get("text").is_some()
        || value.get("content").is_some()
        || value
            .get("message")
            .and_then(|message| message.get("content"))
            .is_some()
        || value.get("output").is_some()
        || value.get("result").is_some()
        || value
            .get("part")
            .and_then(|part| part.get("text"))
            .is_some()
}

#[test]
fn fixtures_under_size_cap() {
    let mut corpus_bytes: u64 = 0;
    for path in json_fixture_paths() {
        let meta =
            fs::metadata(&path).unwrap_or_else(|e| panic!("metadata for {}: {e}", path.display()));
        assert!(
            meta.len() <= MAX_FIXTURE_BYTES,
            "{} exceeds {} bytes",
            path.display(),
            MAX_FIXTURE_BYTES
        );
        corpus_bytes += meta.len();
    }
    assert!(
        corpus_bytes <= MAX_CORPUS_BYTES,
        "fixture corpus {corpus_bytes} exceeds cap {MAX_CORPUS_BYTES}"
    );
}

#[test]
fn every_fixture_is_valid_json_or_known_malformed() {
    let malformed_allowlist: &[&str] = &["malformed.json"];

    for path in json_fixture_paths() {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .expect("fixture path has utf-8 filename");
        let content = load_fixture(name);

        if malformed_allowlist.contains(&name) {
            assert!(
                serde_json::from_str::<serde_json::Value>(&content).is_err(),
                "{name}: expected malformed fixture to fail JSON parse"
            );
            continue;
        }

        let values = parse_json_values(&content);
        assert!(
            !values.is_empty(),
            "{name}: expected at least one JSON object"
        );

        if name != "error-envelope.json" {
            let has_text = values.iter().any(has_user_visible_text);
            assert!(has_text, "{name}: expected response-like text field");
        }
    }
}

#[test]
fn success_with_stats_fixture_contains_expected_shape() {
    let content = load_fixture("success-with-stats.json");
    let values = parse_json_values(&content);

    assert!(
        values.iter().any(|value| {
            value.get("message").is_some()
                || value.get("response").is_some()
                || value.get("text").is_some()
                || value
                    .get("part")
                    .and_then(|part| part.get("text"))
                    .is_some()
                || value
                    .get("output")
                    .or_else(|| value.get("result"))
                    .is_some()
        }),
        "expected message/response/text/output/result field for success fixture"
    );

    let usage_present = values.iter().any(|value| {
        value.get("usage").is_some()
            || value.get("stats").is_some()
            || value.get("tokens").is_some()
            || value
                .get("part")
                .and_then(|part| {
                    part.get("tokens")
                        .or_else(|| part.get("usage"))
                        .or_else(|| part.get("stats"))
                })
                .is_some()
    });
    assert!(
        usage_present,
        "expected usage-like field for success-with-stats fixture"
    );
}

#[test]
fn success_no_stats_fixture_has_response_no_stats() {
    let content = load_fixture("success-no-stats.json");
    let values = parse_json_values(&content);
    assert!(
        values.iter().any(|value| {
            value.get("message").is_some()
                || value.get("response").is_some()
                || value.get("text").is_some()
                || value
                    .get("part")
                    .and_then(|part| part.get("text"))
                    .is_some()
        }),
        "expected response-like text for success-no-stats fixture"
    );

    let has_usage = values.iter().any(|value| {
        value.get("stats").is_some()
            || value.get("usage").is_some()
            || value.get("tokens").is_some()
            || value
                .get("part")
                .and_then(|part| {
                    part.get("tokens")
                        .or_else(|| part.get("usage"))
                        .or_else(|| part.get("stats"))
                })
                .is_some()
    });
    assert!(!has_usage, "expected no usage in success-no-stats fixture");
}

#[test]
fn fixtures_are_scrubbed() {
    let forbidden_literals = [
        "/Users/",
        "/home/",
        "C:\\Users\\",
        "$HOME",
        "/tmp/",
        "/var/folders/",
        "Bearer ",
    ];

    for path in json_fixture_paths() {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .expect("fixture path has utf-8 filename");
        let stream = load_fixture(name);

        for literal in &forbidden_literals {
            assert!(
                !stream.contains(literal),
                "{name}: fixture contains unsanitized sensitive marker {literal:?}"
            );
        }

        for lower_line in stream.lines().map(|line| line.to_ascii_lowercase()) {
            for marker in [
                "api_key",
                "api-key",
                "apikey",
                "access_token",
                "auth_token",
                "\"token\":",
                "token=",
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
    }
}
