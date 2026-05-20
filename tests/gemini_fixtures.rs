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

        let value = serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|e| panic!("{name}: fixture must be valid JSON: {e}"));

        // Success fixtures should have a response field; error-envelope does not (by design)
        if name != "error-envelope.json" {
            assert!(
                value.get("response").is_some(),
                "{name}: expected 'response' field"
            );
        }
    }
}

#[test]
fn success_with_stats_fixture_contains_expected_shape() {
    let content = load_fixture("success-with-stats.json");
    let value = serde_json::from_str::<serde_json::Value>(&content)
        .expect("success-with-stats.json is valid JSON");

    let response = value
        .get("response")
        .and_then(|v| v.as_str())
        .expect("response is a string");
    assert!(!response.is_empty());

    let stats = value.get("stats").expect("stats present");
    let models = stats
        .get("models")
        .and_then(|m| m.as_object())
        .expect("stats.models is an object");
    assert!(!models.is_empty());

    for (model_name, model) in models {
        let tokens = model
            .get("tokens")
            .and_then(|t| t.as_object())
            .unwrap_or_else(|| panic!("{model_name} has tokens object"));
        assert!(
            tokens.get("prompt").is_some(),
            "{model_name}: tokens.prompt missing"
        );
        assert!(
            tokens.get("candidates").is_some(),
            "{model_name}: tokens.candidates missing"
        );
    }
}

#[test]
fn success_no_stats_fixture_has_response_no_stats() {
    let content = load_fixture("success-no-stats.json");
    let value = serde_json::from_str::<serde_json::Value>(&content)
        .expect("success-no-stats.json is valid JSON");
    assert!(value.get("response").is_some());
    assert!(value.get("stats").is_none());
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
