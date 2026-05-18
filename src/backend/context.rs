use serde_json::Value;
use std::path::Path;
use std::time::Duration;

/// Carrying struct for all per-step concerns passed to `Backend::query`.
///
/// All borrows are tied to the caller's stack frame; the backend must not
/// retain references past the `await` point.
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
pub type StepOptions = std::collections::HashMap<String, serde_json::Value>;

/// Placeholder for the full health status struct introduced in FR-9/9a.
/// Empty today so `Backend::health_check` return type is stable.
#[derive(Debug, Clone)]
pub struct HealthStatus;

/// Sandbox permission levels for subprocess backends.
/// Maps to Codex `-s` and Gemini `--approval-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

/// One turn in a conversation history.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

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
}
