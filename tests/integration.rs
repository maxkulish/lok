//! Integration tests for lok workflow engine
//!
//! These tests use shell-only workflows to verify engine behavior
//! without requiring LLM backends.

use std::process::Command;

fn run_workflow(workflow_path: &str) -> (bool, String) {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--bin", "lok", "--", "run", workflow_path])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to execute lok");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
}

fn run_lok(args: &[&str]) -> (bool, String) {
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--bin", "lok", "--"])
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to execute lok");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
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
fn test_doctor_json_output() {
    let (_success, output) = run_lok(&["doctor", "--output", "json"]);

    // Doctor should produce valid JSON output
    let json_start = output.find('[');
    let json_end = output.rfind(']');

    if let (Some(start), Some(end)) = (json_start, json_end) {
        let json_str = &output[start..=end];
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
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
    } else {
        // If no JSON array found, the output might just be "No backends configured."
        // which is fine — valid behavior when no backends are set up
        assert!(
            output.contains("No backends configured.")
                || output.contains("[]")
                || output.trim().is_empty(),
            "Doctor should produce valid JSON or empty message: {}",
            output
        );
    }
}
