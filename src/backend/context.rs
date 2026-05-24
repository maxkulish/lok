use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Carrying struct for all per-step concerns passed to `Backend::query`.
///
/// All borrows are tied to the caller's stack frame; the backend must not
/// retain references past the `await` point.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct StepContext<'a> {
    pub prompt: &'a str,
    /// Conversation history (FR-19c). Empty slice = single-turn.
    pub history: &'a [Message],
    /// Model override from the active Step or caller config.
    pub model: Option<&'a str>,
    /// CWD for subprocess-backed backends.
    pub cwd: &'a Path,
    /// Sandbox level (FR-21). None = backend default.
    pub sandbox: Option<SandboxMode>,
    /// Per-step intent to parse and apply JSON file-edits from the response (FR-22).
    /// Backends that map to a sandbox flag use this to default to `WorkspaceWrite`
    /// when `sandbox` is `None`. Backends that ignore `sandbox` ignore this field too.
    pub apply_edits: bool,
    /// JSON Schema for structured output (FR-22). None = text mode.
    pub schema: Option<&'a Value>,
    /// Per-step options bag (temperature, top_p, etc.) (FR-24).
    /// `None` when no options are set.
    pub options: Option<&'a StepOptions>,
    /// Per-step timeout. None = no override; backend uses its own default.
    pub timeout: Option<Duration>,
}

/// Minimal options bag for per-step config passthrough.
/// Replace with a typed struct when FR-24 lands.
#[allow(dead_code)]
pub type StepOptions = std::collections::HashMap<String, serde_json::Value>;

impl<'a> StepContext<'a> {
    pub fn from_prompt(prompt: &'a str, cwd: &'a Path, model: Option<&'a str>) -> Self {
        Self {
            prompt,
            history: &[],
            model,
            cwd,
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        }
    }
}

/// Carrying struct for all health checks, version details, and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthStatus {
    pub available: bool,
    pub version: Option<String>,
    /// Discriminator for backends with multiple probe modes
    /// (e.g. "api" | "cli" for Claude, None for single-mode backends).
    pub mode: Option<String>,
    /// Human-readable failure reason when available is false.
    pub diagnostic: Option<String>,
    pub auth_method: Option<String>,
    pub capabilities: Option<serde_json::Value>,
    pub unusable_flags: Vec<String>,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelInfo {
    pub name: String,
    pub modified_at: Option<String>,
    pub size: Option<u64>,
    pub digest: Option<String>,
}

impl HealthStatus {
    pub fn new_available() -> Self {
        Self {
            available: true,
            version: None,
            mode: None,
            diagnostic: None,
            auth_method: None,
            capabilities: None,
            unusable_flags: Vec::new(),
            models: Vec::new(),
        }
    }

    pub fn new_unavailable() -> Self {
        Self {
            available: false,
            version: None,
            mode: None,
            diagnostic: None,
            auth_method: None,
            capabilities: None,
            unusable_flags: Vec::new(),
            models: Vec::new(),
        }
    }
}

/// Sandbox permission levels for subprocess backends.
/// - Codex: maps to `-s` modes.
/// - Gemini (opencode): maps to `--agent` semantics,
///   where `read-only` -> `plan` and `workspace-write`/`default` -> `build`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

/// One turn in a conversation history.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_context_default_is_phase1_equivalent() {
        let prompt = "hello";
        let cwd = Path::new("/tmp");

        let ctx = StepContext {
            prompt,
            history: &[],
            model: None,
            cwd,
            sandbox: None,
            apply_edits: false,
            schema: None,
            options: None,
            timeout: None,
        };

        // These three fields match the old positional arg contract
        assert_eq!(ctx.prompt, "hello");
        assert_eq!(ctx.cwd, Path::new("/tmp"));
        assert_eq!(ctx.model, None);

        // All future fields are None/empty, so Phase-1 behavior is preserved
        assert!(ctx.history.is_empty());
        assert!(ctx.sandbox.is_none());
        assert!(!ctx.apply_edits);
        assert!(ctx.schema.is_none());
        assert!(ctx.options.is_none());
        assert!(ctx.timeout.is_none());
    }

    #[test]
    fn test_step_context_is_copy() {
        // Verify StepContext can be passed by value (e.g. through RetryExecutor)
        fn assert_copy<T: Copy>() {}
        assert_copy::<StepContext>();
    }

    #[test]
    fn test_health_status_mode_and_diagnostic_serde() {
        // Verify that mode and diagnostic fields round-trip through JSON serde
        let status = HealthStatus {
            available: false,
            version: None,
            mode: Some("api".into()),
            diagnostic: Some("ANTHROPIC_API_KEY not set".into()),
            auth_method: None,
            capabilities: None,
            unusable_flags: vec!["--output-format json".into()],
            models: Vec::new(),
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: HealthStatus = serde_json::from_str(&json).unwrap();

        assert!(!deserialized.available);
        assert_eq!(deserialized.mode, Some("api".into()));
        assert_eq!(
            deserialized.diagnostic,
            Some("ANTHROPIC_API_KEY not set".into())
        );
        assert_eq!(
            deserialized.unusable_flags,
            vec!["--output-format json".to_string()]
        );
    }

    #[test]
    fn test_health_status_constructors_default_to_mode_none() {
        let available = HealthStatus::new_available();
        assert!(available.available);
        assert!(available.mode.is_none());
        assert!(available.diagnostic.is_none());

        let unavailable = HealthStatus::new_unavailable();
        assert!(!unavailable.available);
        assert!(unavailable.mode.is_none());
        assert!(unavailable.diagnostic.is_none());
    }

    #[test]
    fn test_sandbox_mode_serde_roundtrip() {
        // Verify kebab-case YAML parsing via JSON (same serde rename rules)
        let json = r#""read-only""#;
        let mode: SandboxMode = serde_json::from_str(json).unwrap();
        assert_eq!(mode, SandboxMode::ReadOnly);

        let json = r#""workspace-write""#;
        let mode: SandboxMode = serde_json::from_str(json).unwrap();
        assert_eq!(mode, SandboxMode::WorkspaceWrite);

        let json = r#""danger-full-access""#;
        let mode: SandboxMode = serde_json::from_str(json).unwrap();
        assert_eq!(mode, SandboxMode::DangerFullAccess);

        // Round-trip serialize + deserialize
        let serialized = serde_json::to_value(SandboxMode::ReadOnly).unwrap();
        assert_eq!(serialized, serde_json::json!("read-only"));
    }
}
