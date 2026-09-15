//! CLO-632 AC6: a cloned repository's `./lok.toml` cannot make `lok ask` run
//! a binary of its choosing.
//!
//! Feature-gated on `cli` because it spawns the `lok` binary, which carries
//! `required-features = ["cli"]`. Unix-only because the hostile backend is a
//! shell script marked executable.
#![cfg(all(feature = "cli", unix))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

/// A project directory holding a `lok.toml` whose codex `command` is a script
/// that creates `marker` when it runs. Every other backend is disabled, so no
/// real `claude`, `opencode` or `ollama` is reached even if the gate breaks.
fn hostile_project(dir: &Path) {
    let script = dir.join("build-helper");
    let marker = dir.join("marker");
    fs::write(&script, format!("#!/bin/sh\n: > '{}'\n", marker.display())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    fs::write(
        dir.join("lok.toml"),
        format!(
            "[backends.codex]\ncommand = \"{}\"\n\n\
             [backends.gemini]\nenabled = false\n\n\
             [backends.claude]\nenabled = false\n\n\
             [backends.ollama]\nenabled = false\n",
            script.display()
        ),
    )
    .unwrap();
}

/// Runs `lok` in `dir` with an empty `HOME` and `PATH` limited to `dir`.
fn lok_in(dir: &Path, home: &Path, extra_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lok"))
        .args(extra_args)
        .args(["ask", "--no-cache", "hi"])
        .current_dir(dir)
        .env_clear()
        .env("HOME", home)
        .env("PATH", dir)
        .output()
        .expect("spawn lok")
}

#[test]
fn hostile_project_lok_toml_never_runs_its_command() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    hostile_project(project.path());
    let project_file = project.path().join("lok.toml");
    let marker = project.path().join("marker");

    let gated = lok_in(project.path(), home.path(), &[]);
    let stderr = String::from_utf8_lossy(&gated.stderr);
    assert!(!gated.status.success(), "lok ask should refuse: {stderr}");
    assert!(stderr.contains("backends.codex.command"), "{stderr}");
    assert!(
        stderr.contains(&project_file.display().to_string()),
        "{stderr}"
    );
    assert!(!marker.exists(), "the project command ran despite the gate");

    // Positive control: the same file passed explicitly is trusted, so the
    // script does run. Without this, a broken script would pass the check above.
    let project_file = project_file.display().to_string();
    lok_in(project.path(), home.path(), &["--config", &project_file]);
    assert!(marker.exists(), "the control run never executed the script");
}
