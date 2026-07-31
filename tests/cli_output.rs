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

    assert!(
        output.status.success(),
        "lok backends failed: {:?}",
        output.status.code()
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<_> = stderr.lines().collect();

    // The whole of stderr, not merely the line we went looking for. Asserting
    // on a found line would let extra chrome appear alongside it and still pass.
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one stderr line, got {}: {stderr:?}",
        lines.len()
    );

    // Byte-identical to the pre-CLO-591 `eprintln!("{} {}", "warning:".yellow(), w)`,
    // once ANSI colour is stripped.
    assert_eq!(
        strip_ansi(lines[0]),
        "warning: Invalid LOK_HEALTH_TTL 'bogus': expected number at 0; using default TTL (30m)",
        "TTL warning text or formatting changed"
    );
}

#[test]
fn rust_log_is_honoured_rather_than_hard_coded() {
    // Positive control: `RUST_LOG=off` must silence a warning that otherwise
    // appears. Asserting only the absence of noise would also pass for a
    // regression that ignored RUST_LOG entirely, so drive it in the direction
    // that requires the variable to be wired up.
    let silenced = Command::new(env!("CARGO_BIN_EXE_lok"))
        .arg("backends")
        .env("LOK_HEALTH_TTL", "bogus")
        .env("RUST_LOG", "off")
        .output()
        .expect("spawn lok");

    assert_eq!(
        String::from_utf8_lossy(&silenced.stderr),
        "",
        "RUST_LOG=off did not suppress the warning, so RUST_LOG is not wired"
    );
}

#[test]
fn raising_verbosity_does_not_leak_third_party_records() {
    // The logger filter is scoped to this crate: raising it to debug must not
    // pull in reqwest, hyper, tokio or rustls internals.
    let output = Command::new(env!("CARGO_BIN_EXE_lok"))
        .arg("backends")
        .env("RUST_LOG", "debug")
        .output()
        .expect("spawn lok");

    let stderr = String::from_utf8_lossy(&output.stderr);
    for noisy in ["hyper::", "reqwest::", "tokio::", "rustls::", "h2::"] {
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
