//! Configurable role-based routing for backend selection
//!
//! This module provides role-based routing that replaces hardcoded keyword-to-backend mappings
//! with a configurable `[roles]` and `[teams]` TOML schema.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Strategy for selecting backends and combining their responses
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum RoutingStrategy {
    /// Try backends in order, return first success
    /// Terminal errors (4xx client errors) short-circuit immediately
    First,
    /// Try backends in parallel, return when min_success backends succeed
    /// Remaining requests are cancelled once quorum is reached
    Parallel {
        /// Minimum number of successful responses required
        min_success: usize,
        /// Optional timeout for each backend invocation in seconds
        timeout_secs: Option<u64>,
    },
    /// Try backends sequentially, falling back on transient errors
    /// Transient errors (429, 500, timeouts) trigger next backend
    /// Terminal errors (401, 400) short-circuit immediately
    Fallback {
        /// Optional timeout for each backend invocation in seconds
        timeout_secs: Option<u64>,
    },
}

impl Default for RoutingStrategy {
    fn default() -> Self {
        Self::Fallback { timeout_secs: None }
    }
}

/// Errors that can occur during role resolution
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum RoleResolutionError {
    /// The requested role was not found in configuration
    RoleNotFound { role: String },
    /// The role was found but no backends are available (all filtered out)
    NoBackendsAvailable { role: String },
    /// Configuration validation error
    ValidationError { role: String, message: String },
}

impl fmt::Display for RoleResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RoleNotFound { role } => {
                write!(f, "Role '{}' not found in configuration", role)
            }
            Self::NoBackendsAvailable { role } => {
                write!(f, "No backends available for role '{}'", role)
            }
            Self::ValidationError { role, message } => {
                write!(f, "Validation error for role '{}': {}", role, message)
            }
        }
    }
}

impl std::error::Error for RoleResolutionError {}

/// The result of resolving a role to backend(s)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The role name that was resolved
    pub role: String,
    /// The team context (if any) that was used
    pub team: Option<String>,
    /// List of backend IDs to invoke
    pub backends: Vec<String>,
    /// Strategy for routing and combining responses
    pub strategy: RoutingStrategy,
    /// Whether this resolution came from a team override
    pub from_team_override: bool,
}

impl Resolution {
    /// Create a new resolution with the given role
    pub fn new(role: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            team: None,
            backends: Vec::new(),
            strategy: RoutingStrategy::default(),
            from_team_override: false,
        }
    }

    /// Returns the list of backend IDs
    #[allow(dead_code)]
    pub fn backend_ids(&self) -> Vec<String> {
        self.backends.clone()
    }

    /// Returns true if no backends are configured
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Set the team context
    pub fn with_team(mut self, team: impl Into<String>) -> Self {
        self.team = Some(team.into());
        self
    }

    /// Set the backends
    pub fn with_backends(mut self, backends: Vec<String>) -> Self {
        self.backends = backends;
        self
    }

    /// Set the strategy
    pub fn with_strategy(mut self, strategy: RoutingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Mark as coming from team override
    pub fn with_team_override(mut self) -> Self {
        self.from_team_override = true;
        self
    }
}

/// Configuration for a role - maps role names to backends and strategies
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RoleConfig {
    /// List of backend IDs to use for this role
    pub backends: Vec<String>,
    /// Routing strategy for this role
    #[serde(default)]
    pub strategy: RoutingStrategy,
}

impl RoleConfig {
    /// Create a new role config with the given backends
    #[allow(dead_code)]
    pub fn new(backends: Vec<String>) -> Self {
        Self {
            backends,
            strategy: RoutingStrategy::default(),
        }
    }
}

/// Configuration for a team - contains role overrides
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct TeamConfig {
    /// Role overrides specific to this team
    #[serde(default)]
    pub roles: HashMap<String, RoleConfig>,
}

/// Warning about configuration issues that don't prevent operation
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ValidationWarning {
    /// The role that has the issue
    pub role: String,
    /// Description of the warning
    pub message: String,
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Warning for role '{}': {}", self.role, self.message)
    }
}

/// Resolves roles to backends using two-tier lookup
///
/// Resolution order:
/// 1. Team override (if team is specified and has role definition)
/// 2. Global role definition (from [roles] section)
/// 3. Error if role not found in either
#[derive(Debug, Clone)]
pub struct RoleResolver {
    /// Global role definitions from [roles]
    roles: HashMap<String, RoleConfig>,
    /// Team definitions from [teams]
    teams: HashMap<String, TeamConfig>,
    /// Default team from [defaults.team]
    default_team: Option<String>,
}

impl RoleResolver {
    /// Create a new resolver from configuration
    pub fn new(
        roles: HashMap<String, RoleConfig>,
        teams: HashMap<String, TeamConfig>,
        default_team: Option<String>,
    ) -> Self {
        Self {
            roles,
            teams,
            default_team,
        }
    }

    /// Resolve a role to a list of backends using two-tier lookup
    ///
    /// Resolution order:
    /// 1. If team_override is provided, check [teams.<team>.roles.<role>]
    /// 2. If default_team is configured, check [teams.<default>.roles.<role>]
    /// 3. Fall back to [roles.<role>]
    /// 4. Return RoleNotFound error if role not found anywhere
    ///
    /// After finding role config:
    /// - Filter backends by available_backends (skip disabled/unavailable)
    /// - Return NoBackendsAvailable if all filtered out
    pub fn resolve(
        &self,
        role: &str,
        team_override: Option<&str>,
        available_backends: &[String],
    ) -> Result<Resolution, RoleResolutionError> {
        // Determine active team: explicit override > config default > none
        let active_team = team_override
            .map(|s| s.to_string())
            .or_else(|| self.default_team.clone());

        // Try team override first (if we have an active team)
        let team_config = active_team.as_ref().and_then(|team| {
            self.teams
                .get(team)
                .and_then(|t| t.roles.get(role).map(|cfg| (team.as_str(), cfg)))
        });

        // Then fall back to global roles
        let (source_team, role_config) = match team_config {
            Some((team, cfg)) => (Some(team), cfg),
            None => match self.roles.get(role) {
                Some(cfg) => (None, cfg),
                None => {
                    return Err(RoleResolutionError::RoleNotFound {
                        role: role.to_string(),
                    });
                }
            },
        };

        // Filter backends by availability
        let available_set: std::collections::HashSet<&str> =
            available_backends.iter().map(|s| s.as_str()).collect();
        let filtered_backends: Vec<String> = role_config
            .backends
            .iter()
            .filter(|b| available_set.contains(b.as_str()))
            .cloned()
            .collect();

        if filtered_backends.is_empty() {
            return Err(RoleResolutionError::NoBackendsAvailable {
                role: role.to_string(),
            });
        }

        let resolution = Resolution::new(role)
            .with_team(active_team.clone().unwrap_or_default())
            .with_backends(filtered_backends)
            .with_strategy(role_config.strategy.clone());

        let resolution = if source_team.is_some() {
            resolution.with_team_override()
        } else {
            resolution
        };

        Ok(resolution)
    }

    /// Validate role configurations and return any warnings
    ///
    /// Checks:
    /// - min_success on First strategy (should not be set)
    /// - min_success on Fallback strategy (should not be set)
    /// - min_success >= 1 on Parallel strategy
    /// - min_success <= backends.len() on Parallel strategy
    /// - Unknown backend references (returned as warnings, not errors)
    #[allow(dead_code)]
    pub fn validate(
        &self,
        known_backends: &[String],
    ) -> (Vec<ValidationWarning>, Vec<RoleResolutionError>) {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();
        let known_set: std::collections::HashSet<&str> =
            known_backends.iter().map(|s| s.as_str()).collect();

        // Validate global roles
        for (role_name, role_config) in &self.roles {
            // Check strategy-specific constraints
            match &role_config.strategy {
                RoutingStrategy::First => {
                    // First strategy doesn't use min_success
                    // Nothing to validate here
                }
                RoutingStrategy::Parallel { min_success, .. } => {
                    if *min_success < 1 {
                        errors.push(RoleResolutionError::ValidationError {
                            role: role_name.clone(),
                            message: format!(
                                "Parallel strategy min_success must be >= 1, got {}",
                                min_success
                            ),
                        });
                    }
                    if *min_success > role_config.backends.len() {
                        errors.push(RoleResolutionError::ValidationError {
                            role: role_name.clone(),
                            message: format!(
                                "Parallel strategy min_success ({}) exceeds number of backends ({})",
                                min_success,
                                role_config.backends.len()
                            ),
                        });
                    }
                }
                RoutingStrategy::Fallback { .. } => {
                    // Fallback strategy doesn't use min_success
                    // Nothing to validate here
                }
            }

            // Check for unknown backends
            for backend in &role_config.backends {
                if !known_set.contains(backend.as_str()) {
                    warnings.push(ValidationWarning {
                        role: role_name.clone(),
                        message: format!("Unknown backend reference: '{}'", backend),
                    });
                }
            }
        }

        // Validate team role overrides
        for (team_name, team_config) in &self.teams {
            for (role_name, role_config) in &team_config.roles {
                match &role_config.strategy {
                    RoutingStrategy::First => {}
                    RoutingStrategy::Parallel { min_success, .. } => {
                        if *min_success < 1 {
                            errors.push(RoleResolutionError::ValidationError {
                                role: format!("{}/{}", team_name, role_name),
                                message: format!(
                                    "Parallel strategy min_success must be >= 1, got {}",
                                    min_success
                                ),
                            });
                        }
                        if *min_success > role_config.backends.len() {
                            errors.push(RoleResolutionError::ValidationError {
                                role: format!("{}/{}", team_name, role_name),
                                message: format!(
                                    "Parallel strategy min_success ({}) exceeds number of backends ({})",
                                    min_success,
                                    role_config.backends.len()
                                ),
                            });
                        }
                    }
                    RoutingStrategy::Fallback { .. } => {}
                }

                for backend in &role_config.backends {
                    if !known_set.contains(backend.as_str()) {
                        warnings.push(ValidationWarning {
                            role: format!("{}/{}", team_name, role_name),
                            message: format!("Unknown backend reference: '{}'", backend),
                        });
                    }
                }
            }
        }

        (warnings, errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_strategy_default_is_fallback() {
        let strategy: RoutingStrategy = Default::default();
        assert!(matches!(
            strategy,
            RoutingStrategy::Fallback { timeout_secs: None }
        ));
    }

    #[test]
    fn test_role_resolution_error_display() {
        let err = RoleResolutionError::RoleNotFound {
            role: "unknown".to_string(),
        };
        assert_eq!(err.to_string(), "Role 'unknown' not found in configuration");

        let err = RoleResolutionError::NoBackendsAvailable {
            role: "test".to_string(),
        };
        assert_eq!(err.to_string(), "No backends available for role 'test'");

        let err = RoleResolutionError::ValidationError {
            role: "bad".to_string(),
            message: "invalid config".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Validation error for role 'bad': invalid config"
        );
    }

    #[test]
    fn test_resolution_builder() {
        let res = Resolution::new("test-role")
            .with_team("my-team")
            .with_backends(vec!["backend1".to_string(), "backend2".to_string()])
            .with_strategy(RoutingStrategy::Parallel {
                min_success: 2,
                timeout_secs: Some(30),
            })
            .with_team_override();

        assert_eq!(res.role, "test-role");
        assert_eq!(res.team, Some("my-team".to_string()));
        assert_eq!(res.backends, vec!["backend1", "backend2"]);
        assert!(matches!(
            res.strategy,
            RoutingStrategy::Parallel {
                min_success: 2,
                timeout_secs: Some(30)
            }
        ));
        assert!(res.from_team_override);
        assert!(!res.is_empty());
    }

    #[test]
    fn test_resolution_is_empty() {
        let res = Resolution::new("test");
        assert!(res.is_empty());

        let res = Resolution::new("test").with_backends(vec!["b1".to_string()]);
        assert!(!res.is_empty());
    }

    #[test]
    fn test_role_config_new() {
        let config = RoleConfig::new(vec!["b1".to_string(), "b2".to_string()]);
        assert_eq!(config.backends, vec!["b1", "b2"]);
        assert!(matches!(
            config.strategy,
            RoutingStrategy::Fallback { timeout_secs: None }
        ));
    }

    #[test]
    fn test_team_config_default() {
        let config = TeamConfig::default();
        assert!(config.roles.is_empty());
    }

    #[test]
    fn test_role_resolver_resolve_global_role() {
        let mut roles = HashMap::new();
        roles.insert(
            "code-review".to_string(),
            RoleConfig::new(vec!["codex".to_string(), "claude".to_string()]),
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);
        let available = vec![
            "codex".to_string(),
            "claude".to_string(),
            "gemini".to_string(),
        ];

        let result = resolver.resolve("code-review", None, &available);
        assert!(result.is_ok());

        let resolution = result.unwrap();
        assert_eq!(resolution.role, "code-review");
        assert_eq!(resolution.backends, vec!["codex", "claude"]);
        assert!(!resolution.from_team_override);
    }

    #[test]
    fn test_role_resolver_role_not_found() {
        let resolver = RoleResolver::new(HashMap::new(), HashMap::new(), None);
        let available = vec!["codex".to_string()];

        let result = resolver.resolve("unknown-role", None, &available);
        assert!(matches!(
            result,
            Err(RoleResolutionError::RoleNotFound { .. })
        ));
    }

    #[test]
    fn test_role_resolver_no_backends_available() {
        let mut roles = HashMap::new();
        roles.insert(
            "test-role".to_string(),
            RoleConfig::new(vec!["unavailable".to_string()]),
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);
        let available = vec!["codex".to_string(), "claude".to_string()];

        let result = resolver.resolve("test-role", None, &available);
        assert!(matches!(
            result,
            Err(RoleResolutionError::NoBackendsAvailable { .. })
        ));
    }

    #[test]
    fn test_role_resolver_team_override() {
        let mut roles = HashMap::new();
        roles.insert(
            "review".to_string(),
            RoleConfig::new(vec!["codex".to_string()]),
        );

        let mut team_roles = HashMap::new();
        team_roles.insert(
            "review".to_string(),
            RoleConfig::new(vec!["claude".to_string(), "gemini".to_string()]),
        );

        let mut teams = HashMap::new();
        teams.insert(
            "security-team".to_string(),
            TeamConfig { roles: team_roles },
        );

        let resolver = RoleResolver::new(roles, teams, None);
        let available = vec![
            "codex".to_string(),
            "claude".to_string(),
            "gemini".to_string(),
        ];

        // Without team override, should use global role
        let result = resolver.resolve("review", None, &available).unwrap();
        assert_eq!(result.backends, vec!["codex"]);
        assert!(!result.from_team_override);

        // With team override, should use team config
        let result = resolver
            .resolve("review", Some("security-team"), &available)
            .unwrap();
        assert_eq!(result.backends, vec!["claude", "gemini"]);
        assert!(result.from_team_override);
    }

    #[test]
    fn test_role_resolver_default_team() {
        let mut roles = HashMap::new();
        roles.insert(
            "review".to_string(),
            RoleConfig::new(vec!["codex".to_string()]),
        );

        let mut team_roles = HashMap::new();
        team_roles.insert(
            "review".to_string(),
            RoleConfig::new(vec!["claude".to_string()]),
        );

        let mut teams = HashMap::new();
        teams.insert("default-team".to_string(), TeamConfig { roles: team_roles });

        let resolver = RoleResolver::new(roles, teams, Some("default-team".to_string()));
        let available = vec!["codex".to_string(), "claude".to_string()];

        // Should use default team when no override provided
        let result = resolver.resolve("review", None, &available).unwrap();
        assert_eq!(result.backends, vec!["claude"]);
        assert!(result.from_team_override);
    }

    #[test]
    fn test_role_resolver_team_override_takes_precedence() {
        let mut team_roles = HashMap::new();
        team_roles.insert(
            "review".to_string(),
            RoleConfig::new(vec!["gemini".to_string()]),
        );

        let mut teams = HashMap::new();
        teams.insert(
            "default-team".to_string(),
            TeamConfig {
                roles: team_roles.clone(),
            },
        );
        teams.insert(
            "override-team".to_string(),
            TeamConfig { roles: team_roles },
        );

        let resolver = RoleResolver::new(HashMap::new(), teams, Some("default-team".to_string()));
        let available = vec!["gemini".to_string()];

        // Explicit override should take precedence over default
        let result = resolver
            .resolve("review", Some("override-team"), &available)
            .unwrap();
        assert_eq!(result.team, Some("override-team".to_string()));
    }

    #[test]
    fn test_role_resolver_team_can_define_custom_role() {
        // Team defines a role that doesn't exist in global config
        let mut team_roles = HashMap::new();
        team_roles.insert(
            "custom-role".to_string(),
            RoleConfig::new(vec!["ollama".to_string()]),
        );

        let mut teams = HashMap::new();
        teams.insert("custom-team".to_string(), TeamConfig { roles: team_roles });

        let resolver = RoleResolver::new(HashMap::new(), teams, None);
        let available = vec!["ollama".to_string()];

        // Custom role should be found via team override
        let result = resolver
            .resolve("custom-role", Some("custom-team"), &available)
            .unwrap();
        assert_eq!(result.backends, vec!["ollama"]);
        assert!(result.from_team_override);
    }

    #[test]
    fn test_validation_parallel_min_success_too_low() {
        let mut roles = HashMap::new();
        roles.insert(
            "test".to_string(),
            RoleConfig {
                backends: vec!["b1".to_string()],
                strategy: RoutingStrategy::Parallel {
                    min_success: 0,
                    timeout_secs: None,
                },
            },
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);
        let (warnings, errors) = resolver.validate(&["b1".to_string()]);

        assert!(warnings.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            matches!(&errors[0], RoleResolutionError::ValidationError { role, .. } if role == "test")
        );
    }

    #[test]
    fn test_validation_parallel_min_success_exceeds_backends() {
        let mut roles = HashMap::new();
        roles.insert(
            "test".to_string(),
            RoleConfig {
                backends: vec!["b1".to_string()],
                strategy: RoutingStrategy::Parallel {
                    min_success: 5,
                    timeout_secs: None,
                },
            },
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);
        let (warnings, errors) = resolver.validate(&["b1".to_string()]);

        assert!(warnings.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("exceeds number of backends"));
    }

    #[test]
    fn test_validation_unknown_backend() {
        let mut roles = HashMap::new();
        roles.insert(
            "test".to_string(),
            RoleConfig::new(vec!["unknown-backend".to_string()]),
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);
        let (warnings, errors) = resolver.validate(&["known".to_string()]);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("unknown-backend"));
        assert!(errors.is_empty());
    }

    #[test]
    fn test_valid_parallel_config() {
        let mut roles = HashMap::new();
        roles.insert(
            "test".to_string(),
            RoleConfig {
                backends: vec!["b1".to_string(), "b2".to_string(), "b3".to_string()],
                strategy: RoutingStrategy::Parallel {
                    min_success: 2,
                    timeout_secs: Some(30),
                },
            },
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);
        let (warnings, errors) =
            resolver.validate(&["b1".to_string(), "b2".to_string(), "b3".to_string()]);

        assert!(warnings.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn test_backend_filtering() {
        let mut roles = HashMap::new();
        roles.insert(
            "test".to_string(),
            RoleConfig::new(vec!["b1".to_string(), "b2".to_string(), "b3".to_string()]),
        );

        let resolver = RoleResolver::new(roles, HashMap::new(), None);

        // Only b1 and b3 are available
        let available = vec!["b1".to_string(), "b3".to_string()];
        let result = resolver.resolve("test", None, &available).unwrap();

        assert_eq!(result.backends, vec!["b1", "b3"]);
    }

    #[test]
    fn test_team_config_serialization() {
        let mut team_roles = HashMap::new();
        team_roles.insert(
            "review".to_string(),
            RoleConfig::new(vec!["codex".to_string()]),
        );

        let team = TeamConfig { roles: team_roles };

        // Serialize to TOML
        let toml_str = toml::to_string(&team).unwrap();
        assert!(toml_str.contains("review"));
        assert!(toml_str.contains("codex"));

        // Deserialize back
        let deserialized: TeamConfig = toml::from_str(&toml_str).unwrap();
        assert!(deserialized.roles.contains_key("review"));
    }

    #[test]
    fn test_role_config_serialization() {
        let config = RoleConfig {
            backends: vec!["b1".to_string(), "b2".to_string()],
            strategy: RoutingStrategy::Parallel {
                min_success: 2,
                timeout_secs: Some(60),
            },
        };

        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: RoleConfig = toml::from_str(&toml_str).unwrap();

        assert_eq!(deserialized.backends, vec!["b1", "b2"]);
        assert!(matches!(
            deserialized.strategy,
            RoutingStrategy::Parallel {
                min_success: 2,
                timeout_secs: Some(60)
            }
        ));
    }
}
