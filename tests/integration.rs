//! Integration tests for lok workflow engine
//!
//! These tests use shell-only workflows to verify engine behavior
//! without requiring LLM backends.

use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::process::Command;

fn run_workflow(workflow_path: &str) -> (bool, String) {
    run_workflow_with_env(Path::new(workflow_path), &[])
}

fn run_workflow_with_env(workflow_path: &Path, envs: &[(&str, &OsStr)]) -> (bool, String) {
    let mut command = Command::new("cargo");
    command
        .args([
            "run",
            "--quiet",
            "--bin",
            "lok",
            "--",
            "run",
            workflow_path.to_str().expect("workflow path is UTF-8"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"));
    for (key, value) in envs {
        command.env(key, value);
    }

    let output = command.output().expect("Failed to execute lok");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
}

fn run_lok(args: &[&str]) -> (bool, String, String) {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--bin", "lok", "--"])
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to execute lok");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), stdout, stderr)
}

#[test]
fn test_interpolation_workflow() {
    let (success, output) = run_workflow("tests/workflows/test_interpolation.toml");

    assert!(success, "Workflow failed: {}", output);
    assert!(
        output.contains("hello from step1"),
        "Missing step1 output: {}",
        output
    );
    assert!(
        output.contains("step1 said:"),
        "Missing interpolation: {}",
        output
    );
}

#[test]
fn test_shell_hash_expansion_workflow() {
    let (success, output) = run_workflow("tests/workflows/test_shell_hash_expansion.toml");

    assert!(success, "Workflow failed: {}", output);
    assert!(
        output.contains("[OK] ollama_review"),
        "ollama_review did not run: {}",
        output
    );
    assert!(
        output.contains("  short\n  hello"),
        "Missing ${{#OUTPUT}} branch output in step results: {}",
        output
    );
}

#[test]
fn test_conditionals_workflow() {
    let (success, output) = run_workflow("tests/workflows/test_conditionals.toml");

    assert!(success, "Workflow failed: {}", output);

    // should_run should execute (ISSUES_FOUND matches)
    assert!(
        output.contains("CONDITIONAL_RAN: issues branch"),
        "Should have run issues branch: {}",
        output
    );

    // should_skip should NOT execute (NO_ISSUES doesn't match)
    assert!(
        !output.contains("CONDITIONAL_RAN: clean branch"),
        "Should have skipped clean branch: {}",
        output
    );

    // not() condition should work
    assert!(
        output.contains("NOT_CONDITION_RAN: correct"),
        "not() condition failed: {}",
        output
    );

    // equals() should work
    assert!(
        output.contains("EQUALS_WORKED"),
        "equals() condition failed: {}",
        output
    );
}

#[test]
fn test_retry_workflow() {
    // Clean up any existing counter file first
    let _ = std::fs::remove_file("/tmp/lok_retry_test_counter");

    let (success, output) = run_workflow("tests/workflows/test_retry.toml");

    assert!(success, "Workflow should succeed after retries: {}", output);
    assert!(
        output.contains("SUCCESS on attempt"),
        "Should show success message: {}",
        output
    );
    assert!(
        output.contains("Retry") || output.contains("will retry"),
        "Should show retry attempts: {}",
        output
    );
}

#[test]
fn test_parallel_workflow() {
    let (success, output) = run_workflow("tests/workflows/test_parallel.toml");

    assert!(success, "Workflow failed: {}", output);

    // Lok prints "[parallel] Running N steps in parallel" when parallelizing
    assert!(
        output.contains("[parallel]") || output.contains("Running 3 steps in parallel"),
        "Steps should run in parallel: {}",
        output
    );

    assert!(
        output.contains("A done") && output.contains("B done") && output.contains("C done"),
        "All parallel steps should complete: {}",
        output
    );
}

#[test]
fn test_validate_workflow() {
    let (success, output) = run_workflow("tests/workflows/test_validate.toml");

    assert!(
        success,
        "Workflow should succeed (soft failures only): {}",
        output
    );

    // empty_output step should fail validation
    assert!(
        output.contains("Validation failed") && output.contains("not_empty"),
        "Should show validation failure for empty output: {}",
        output
    );

    // valid_output, length_check, contains_check should pass
    // final step should succeed (min_deps_success = 3, and 3+ deps succeed)
    assert!(
        output.contains("All validation tests completed"),
        "Final step should run: {}",
        output
    );
}

#[test]
fn test_llm_validate_workflow() {
    let (success, output) = run_workflow("tests/workflows/test_llm_validate.toml");

    assert!(
        success,
        "Workflow should succeed (soft failures + min_deps_success): {}",
        output
    );

    // Test 1: heuristic_gates_llm - heuristic fails so LLM is never called
    // Even though backend "nonexistent" doesn't exist, this should fail from heuristic, not backend error
    assert!(
        output.contains("Validation failed") && output.contains("heuristic:not_empty"),
        "Heuristic should fail before LLM is invoked: {}",
        output
    );

    // Test 2: backend_not_found_fail - should show validator error
    assert!(
        output.contains("Validation backend not found"),
        "Should show backend not found error: {}",
        output
    );

    // Test 3 & 4: on_error = "pass" and "skip" - steps should succeed
    // Test 6: summary step should run (at least 2 deps succeed: pass + skip)
    assert!(
        output.contains("LLM validation tests completed"),
        "Summary step should run (min_deps_success met): {}",
        output
    );
}

#[test]
fn test_shell_escape_positive_control_executes_payload() {
    let marker_dir = tempfile::tempdir().expect("marker tempdir");
    let marker = marker_dir.path().join("executed");
    let payload = format!("$(touch \"{}\")", marker.display());
    let command = format!("printf '%s\\n' {payload}");
    let output = Command::new("sh")
        .args(["-c", &command])
        .output()
        .expect("run positive control");
    assert!(output.status.success());
    assert!(marker.exists(), "positive control did not execute payload");
}

#[test]
fn test_shell_escape_hostile_output_workflow() {
    let marker_dir = tempfile::tempdir().expect("marker tempdir");
    let output_dir = tempfile::tempdir().expect("output tempdir");
    let workflow = Path::new("tests/workflows/test_shell_escape_hostile.toml");
    let envs = [
        ("LOK_TEST_MARKER_DIR", marker_dir.path().as_os_str()),
        ("LOK_TEST_OUT_DIR", output_dir.path().as_os_str()),
    ];

    let (success, output) = run_workflow_with_env(workflow, &envs);
    assert!(success, "Hostile workflow failed: {output}");
    for step in ["write_file", "compose", "pipe"] {
        assert!(
            output.contains(&format!("[OK] {step}")),
            "{step} did not run: {output}"
        );
    }
    assert!(
        output.contains("LOK_TEST_DELIMITER_IS_DATA"),
        "delimiter control did not pass: {output}"
    );
    assert!(
        fs::read_dir(marker_dir.path())
            .expect("read marker directory")
            .next()
            .is_none(),
        "hostile output executed a command"
    );

    let payload = r#"'; touch "$LOK_TEST_MARKER_DIR/quote"; '
`touch "$LOK_TEST_MARKER_DIR/backtick"`
$(touch "$LOK_TEST_MARKER_DIR/command-substitution")
; touch "$LOK_TEST_MARKER_DIR/semicolon"
ENDOLLAMA
touch "$LOK_TEST_MARKER_DIR/after-ENDOLLAMA"
ENDFALLBACK
touch "$LOK_TEST_MARKER_DIR/after-ENDFALLBACK"
ENDCLAUDE
touch "$LOK_TEST_MARKER_DIR/after-ENDCLAUDE"
ENDSYNTH
touch "$LOK_TEST_MARKER_DIR/after-ENDSYNTH"
LOKEOF
touch "$LOK_TEST_MARKER_DIR/after-LOKEOF"
EOF
touch "$LOK_TEST_MARKER_DIR/after-EOF"
LOK_WF_CODEX_OUTPUT_EOF
touch "$LOK_TEST_MARKER_DIR/after-LOK_WF_CODEX_OUTPUT_EOF"
LOK_WF_SYNTH_OUTPUT_EOF
touch "$LOK_TEST_MARKER_DIR/after-LOK_WF_SYNTH_OUTPUT_EOF"
LOK_WF_FALLBACK_OUTPUT_EOF
touch "$LOK_TEST_MARKER_DIR/after-LOK_WF_FALLBACK_OUTPUT_EOF""#;
    let written = fs::read_to_string(output_dir.path().join("written.txt")).expect("written file");
    assert_eq!(written.trim_end_matches('\n'), payload);
    let composed =
        fs::read_to_string(output_dir.path().join("composed.txt")).expect("composed file");
    assert_eq!(composed, format!("Header\n\n{payload}\n"));
}

#[test]
fn test_doctor_json_output() {
    let (_success, output, _stderr) = run_lok(&["doctor", "--output", "json"]);

    // Doctor should produce valid JSON output (from stdout, ignoring stderr diagnostics)
    let trimmed = output.trim();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(trimmed);
    assert!(
        parsed.is_ok(),
        "Doctor JSON output should parse as valid JSON: {}",
        output
    );
    let arr = parsed.unwrap();
    assert!(
        arr.is_array(),
        "Doctor JSON output should be a JSON array: {}",
        output
    );
    // Verify each entry has required fields
    if let Some(entries) = arr.as_array() {
        for entry in entries {
            assert!(
                entry.get("backend").is_some(),
                "Each entry should have 'backend' field: {}",
                entry
            );
            assert!(
                entry.get("available").is_some(),
                "Each entry should have 'available' field: {}",
                entry
            );
        }
    }
}
