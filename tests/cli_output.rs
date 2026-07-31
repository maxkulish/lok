//! CLO-591 AC11: the CLI still shows library warnings, with no logger chrome.
//!
//! The library stopped writing to the terminal and now emits `log` records, so
//! the binary installs a formatter to render them. This asserts that swap did
//! not regress what a `lok` user sees.
//!
//! Feature-gated on `cli` because it spawns the `lok` binary, which carries
//! `required-features = ["cli"]`. Without the gate, referencing
//! `CARGO_BIN_EXE_lok` would break `cargo clippy --tests --no-default-features`.
#![cfg(feature = "cli")]

use std::process::Command;

/// `LOK_HEALTH_TTL` is the right probe for an exact-output assertion because it
/// is deterministic. The retry warning is not: `RetryPolicy::get_delay` applies
/// +/-10% jitter, so its rendered delay differs run to run.
#[test]
fn ttl_warning_reaches_stderr_without_logger_chrome() {
    let output = Command::new(env!("CARGO_BIN_EXE_lok"))
        .arg("backends")
        .env("LOK_HEALTH_TTL", "bogus")
        .env_remove("RUST_LOG")
        .output()
        .expect("spawn lok");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .find(|l| l.contains("LOK_HEALTH_TTL"))
        .unwrap_or_else(|| panic!("no TTL warning on stderr; got: {stderr:?}"));

    // Byte-identical to the pre-CLO-591 `eprintln!("{} {}", "warning:".yellow(), w)`,
    // once ANSI colour is stripped.
    let plain = strip_ansi(line);
    assert_eq!(
        plain,
        "warning: Invalid LOK_HEALTH_TTL 'bogus': expected number at 0; using default TTL (30m)",
        "TTL warning text changed"
    );

    // The point of the custom formatter: env_logger's default would prefix
    // every line with a timestamp, the target and a level header.
    assert!(
        !plain.contains("WARN") && !plain.contains("lokomotiv::"),
        "logger chrome leaked into CLI output: {plain:?}"
    );
    assert!(
        !plain.starts_with('['),
        "timestamp prefix leaked into CLI output: {plain:?}"
    );
}

#[test]
fn rust_log_can_still_raise_verbosity() {
    // Proves the binary honours RUST_LOG rather than hard-coding a level, and
    // that the filter is scoped: raising lokomotiv to debug must not pull in
    // reqwest, hyper or tokio records.
    let output = Command::new(env!("CARGO_BIN_EXE_lok"))
        .arg("backends")
        .env("RUST_LOG", "lokomotiv=debug")
        .output()
        .expect("spawn lok");

    let stderr = String::from_utf8_lossy(&output.stderr);
    for noisy in ["hyper::", "reqwest::", "tokio::", "rustls::"] {
        assert!(
            !stderr.contains(noisy),
            "third-party records leaked at debug level ({noisy}): {stderr:?}"
        );
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for e in chars.by_ref() {
                if e.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
