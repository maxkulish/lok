use serde::de::{self, Visitor};
use serde::ser;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for a single LLM backend.
///
/// This struct is intentionally independent from lok's orchestration `Config`
/// so it can be used by library consumers that only want to run queries.
/// `PartialEq + Eq + Hash` make this usable inside
/// [`BackendKey`](crate::backend::BackendKey), which keys the backend cache on
/// configuration rather than on name alone. Every field is a `bool`, integer,
/// `String`, `Vec<String>` or `Duration`, all of which are `Eq + Hash`; adding a
/// floating-point field would break the derive and require a decision about how
/// to hash it.
#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    /// Whether this backend may be selected. Defaults to `true`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Executable to invoke for subprocess-backed backends, or the endpoint URL
    /// for HTTP-backed ones such as Ollama.
    pub command: Option<String>,
    /// Extra arguments appended to every invocation of `command`.
    #[serde(default)]
    pub args: Vec<String>,
    /// Number of leading lines to strip from the backend's stdout before it is
    /// treated as the response, for CLIs that print a banner.
    #[serde(default)]
    pub skip_lines: usize,
    /// Environment variable holding the API key, for backends that authenticate
    /// with one. The key itself is never stored in config.
    pub api_key_env: Option<String>,
    /// Model identifier passed to the backend. Backend-specific; `None` leaves
    /// the backend's own default in place.
    pub model: Option<String>,
    /// Per-backend timeout duration (overrides defaults.timeout). Accepts
    /// human-readable strings like "30s" or raw integers (seconds).
    #[serde(
        default,
        deserialize_with = "deser_duration_seconds",
        serialize_with = "serialize_duration_seconds"
    )]
    pub timeout: Option<Duration>,
    /// Per-backend retry limit (overrides defaults.max_retries)
    pub max_retries: Option<usize>,
    /// Per-backend retry delay in milliseconds (overrides defaults.retry_delay_ms)
    pub retry_delay_ms: Option<u64>,
}

/// Default value for `BackendConfig::enabled`. Crate-internal: the binary
/// carries its own `default_enabled` in `src/cache.rs`, so nothing outside this
/// module ever referenced the library's copy.
pub(crate) fn default_enabled() -> bool {
    true
}

/// Fallbacks a caller supplies when a `BackendConfig` leaves retry fields unset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryDefaults {
    /// Retry count used when `BackendConfig::max_retries` is `None`.
    pub max_retries: usize,
    /// Base delay in milliseconds used when `BackendConfig::retry_delay_ms` is
    /// `None`.
    pub retry_delay_ms: u64,
}

impl Default for RetryDefaults {
    fn default() -> Self {
        Self {
            max_retries: 0,
            retry_delay_ms: 1000,
        }
    }
}

// ---------------------------------------------------------------------------
// Custom serde deserializers for duration fields
// ---------------------------------------------------------------------------

/// Deserialize an `Option<Duration>` from a human-readable duration string
/// ("30s", "5m", "1h") or a raw integer (interpreted as **seconds**).
/// Used for config-level fields (`BackendConfig.timeout`).
pub(crate) fn deser_duration_seconds<'de, D: de::Deserializer<'de>>(
    d: D,
) -> Result<Option<Duration>, D::Error> {
    d.deserialize_any(DurationSecondsVisitor)
}

/// Serialize an `Option<Duration>` as an integer (seconds).
/// Used for config-level fields (`BackendConfig.timeout`).
pub(crate) fn serialize_duration_seconds<S: ser::Serializer>(
    val: &Option<Duration>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match val {
        Some(d) => s.serialize_u64(d.as_secs()),
        None => s.serialize_none(),
    }
}

/// Visitor used by `deser_duration_seconds`. Exposed within the crate so
/// `crate::config` can reuse it when re-exporting the seconds-level helpers.
pub(crate) struct DurationSecondsVisitor;

impl<'de> Visitor<'de> for DurationSecondsVisitor {
    type Value = Option<Duration>;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a duration string like \"30s\" or an integer (seconds)")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        humantime::parse_duration(v)
            .map(Some)
            .map_err(|e| de::Error::custom(format!("invalid duration string: {e}")))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        if v < 0 {
            return Err(de::Error::invalid_value(
                de::Unexpected::Signed(v),
                &"non-negative integer or duration string",
            ));
        }
        Ok(Some(Duration::from_secs(v as u64)))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Some(Duration::from_secs(v)))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_seconds_toml(input: &str) -> Option<Duration> {
        #[derive(Deserialize)]
        struct S {
            #[serde(deserialize_with = "deser_duration_seconds")]
            timeout: Option<Duration>,
        }
        toml::from_str::<S>(input).unwrap().timeout
    }

    #[test]
    fn test_backend_config_deserializes_without_orchestration_config() {
        let toml_str = r#"
command = "ollama"
args = ["serve"]
model = "llama3.2"
timeout = "30s"
max_retries = 3
retry_delay_ms = 500
"#;
        let config: BackendConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.command, Some("ollama".to_string()));
        assert_eq!(config.args, vec!["serve"]);
        assert_eq!(config.model, Some("llama3.2".to_string()));
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.max_retries, Some(3));
        assert_eq!(config.retry_delay_ms, Some(500));
    }

    #[test]
    fn test_backend_config_rejects_unknown_fields() {
        let toml_str = r#"
command = "ollama"
unknown_field = "oops"
"#;
        let result = toml::from_str::<BackendConfig>(toml_str);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown field"),
            "error should mention unknown field: {}",
            err
        );
    }

    #[test]
    fn test_backend_config_timeout_human_and_integer_forms() {
        assert_eq!(
            roundtrip_seconds_toml("timeout = \"30s\""),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            roundtrip_seconds_toml("timeout = 30"),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn test_retry_defaults_default() {
        let defaults = RetryDefaults::default();
        assert_eq!(defaults.max_retries, 0);
        assert_eq!(defaults.retry_delay_ms, 1000);
    }
}
