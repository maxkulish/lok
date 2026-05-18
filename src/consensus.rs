//! Consensus strategies for combining multiple backend responses.
//!
//! Supports:
//! - `first`: Use first successful response (no consensus)
//! - `synthesis`: LLM synthesizes multiple responses (default for text)
//! - `vote`: Majority vote (for classification/yes-no)
//! - `weighted_vote`: Weighted majority by backend tier

use crate::backend::TokenUsage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Consensus strategy for combining multiple backend responses
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConsensusStrategy {
    /// Use first successful response (no consensus needed)
    First,
    /// LLM synthesizes multiple responses into one (default)
    #[default]
    Synthesis,
    /// Majority vote - pick most common response (for classification)
    Vote,
    /// Weighted vote - weight by backend tier
    WeightedVote,
}

/// Result of a voting consensus
#[derive(Debug, Clone)]
pub struct VoteResult {
    /// The winning response
    pub winner: String,
    /// Vote breakdown: response -> count
    pub breakdown: HashMap<String, usize>,
    /// Total votes cast
    pub total: usize,
    /// Whether there was a tie (winner chosen by first occurrence)
    pub was_tie: bool,
}

/// Result of a weighted voting consensus
#[derive(Debug, Clone)]
pub struct WeightedVoteResult {
    /// The winning response
    pub winner: String,
    /// Weighted score breakdown: response -> total_weight
    pub breakdown: HashMap<String, f64>,
    /// Whether there was a tie
    pub was_tie: bool,
}

/// Backend tier weights for weighted voting
#[derive(Debug, Clone)]
pub struct BackendWeights {
    weights: HashMap<String, f64>,
    default_weight: f64,
}

impl Default for BackendWeights {
    fn default() -> Self {
        let mut weights = HashMap::new();
        // Default tier weights - cloud models weighted higher
        weights.insert("claude".to_string(), 2.0);
        weights.insert("codex".to_string(), 1.5);
        weights.insert("gemini".to_string(), 1.5);
        weights.insert("bedrock".to_string(), 2.0);
        weights.insert("ollama".to_string(), 1.0);

        Self {
            weights,
            default_weight: 1.0,
        }
    }
}

impl BackendWeights {
    /// Create custom weights
    #[allow(dead_code)]
    pub fn new(weights: HashMap<String, f64>) -> Self {
        Self {
            weights,
            default_weight: 1.0,
        }
    }

    /// Get weight for a backend
    pub fn get(&self, backend: &str) -> f64 {
        self.weights
            .get(backend)
            .copied()
            .unwrap_or(self.default_weight)
    }
}

/// Response from a backend for voting
#[derive(Debug, Clone)]
pub struct BackendResponse {
    pub backend: String,
    pub content: String,
    pub usage: Option<TokenUsage>,
}

/// Sum every `Some(usage)` value across the slice via `TokenUsage::saturating_add`.
/// Returns `None` if no element reported usage (matches the "no metering" case).
pub fn aggregate_usage(usages: impl IntoIterator<Item = Option<TokenUsage>>) -> Option<TokenUsage> {
    usages
        .into_iter()
        .flatten()
        .reduce(|acc, u| acc.saturating_add(&u))
}

/// Perform majority vote on responses
///
/// Returns the most common response. Ties broken by first occurrence.
pub fn majority_vote(responses: &[BackendResponse]) -> Option<VoteResult> {
    if responses.is_empty() {
        return None;
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut first_seen: HashMap<String, usize> = HashMap::new();

    for (idx, resp) in responses.iter().enumerate() {
        // Normalize whitespace for comparison
        let normalized = resp.content.trim().to_string();
        *counts.entry(normalized.clone()).or_default() += 1;
        first_seen.entry(normalized).or_insert(idx);
    }

    let max_count = *counts.values().max().unwrap_or(&0);
    let winners: Vec<_> = counts
        .iter()
        .filter(|(_, &count)| count == max_count)
        .collect();

    let was_tie = winners.len() > 1;

    // Break tie by first occurrence
    let winner = winners
        .into_iter()
        .min_by_key(|(content, _)| first_seen.get(*content).unwrap_or(&usize::MAX))
        .map(|(content, _)| content.clone())?;

    Some(VoteResult {
        winner,
        breakdown: counts,
        total: responses.len(),
        was_tie,
    })
}

/// Perform weighted vote on responses
///
/// Each backend's vote is weighted by its tier. Returns the response with highest total weight.
pub fn weighted_vote(
    responses: &[BackendResponse],
    weights: &BackendWeights,
) -> Option<WeightedVoteResult> {
    if responses.is_empty() {
        return None;
    }

    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut first_seen: HashMap<String, usize> = HashMap::new();

    for (idx, resp) in responses.iter().enumerate() {
        let normalized = resp.content.trim().to_string();
        let weight = weights.get(&resp.backend);
        *scores.entry(normalized.clone()).or_default() += weight;
        first_seen.entry(normalized).or_insert(idx);
    }

    let max_score = scores.values().cloned().fold(0.0, f64::max);
    let winners: Vec<_> = scores
        .iter()
        .filter(|(_, &score)| (score - max_score).abs() < f64::EPSILON)
        .collect();

    let was_tie = winners.len() > 1;

    // Break tie by first occurrence
    let winner = winners
        .into_iter()
        .min_by_key(|(content, _)| first_seen.get(*content).unwrap_or(&usize::MAX))
        .map(|(content, _)| content.clone())?;

    Some(WeightedVoteResult {
        winner,
        breakdown: scores,
        was_tie,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_majority_vote_clear_winner() {
        let responses = vec![
            BackendResponse {
                backend: "claude".to_string(),
                content: "yes".to_string(),
                usage: None,
            },
            BackendResponse {
                backend: "codex".to_string(),
                content: "yes".to_string(),
                usage: None,
            },
            BackendResponse {
                backend: "ollama".to_string(),
                content: "no".to_string(),
                usage: None,
            },
        ];

        let result = majority_vote(&responses).unwrap();
        assert_eq!(result.winner, "yes");
        assert!(!result.was_tie);
        assert_eq!(result.breakdown.get("yes"), Some(&2));
        assert_eq!(result.breakdown.get("no"), Some(&1));
    }

    #[test]
    fn test_majority_vote_tie_first_wins() {
        let responses = vec![
            BackendResponse {
                backend: "claude".to_string(),
                content: "A".to_string(),
                usage: None,
            },
            BackendResponse {
                backend: "codex".to_string(),
                content: "B".to_string(),
                usage: None,
            },
        ];

        let result = majority_vote(&responses).unwrap();
        assert_eq!(result.winner, "A"); // First occurrence wins tie
        assert!(result.was_tie);
    }

    #[test]
    fn test_majority_vote_empty() {
        let responses: Vec<BackendResponse> = vec![];
        assert!(majority_vote(&responses).is_none());
    }

    #[test]
    fn test_weighted_vote() {
        let responses = vec![
            BackendResponse {
                backend: "claude".to_string(),
                content: "yes".to_string(),
                usage: None,
            }, // weight 2.0
            BackendResponse {
                backend: "ollama".to_string(),
                content: "no".to_string(),
                usage: None,
            }, // weight 1.0
            BackendResponse {
                backend: "ollama".to_string(),
                content: "no".to_string(),
                usage: None,
            }, // weight 1.0
        ];

        let weights = BackendWeights::default();
        let result = weighted_vote(&responses, &weights).unwrap();

        // claude (2.0) for "yes" vs ollama+ollama (2.0) for "no" - tie, first wins
        assert_eq!(result.winner, "yes");
        assert!(result.was_tie);
    }

    #[test]
    fn test_weighted_vote_clear_winner() {
        let responses = vec![
            BackendResponse {
                backend: "claude".to_string(),
                content: "yes".to_string(),
                usage: None,
            }, // 2.0
            BackendResponse {
                backend: "bedrock".to_string(),
                content: "yes".to_string(),
                usage: None,
            }, // 2.0
            BackendResponse {
                backend: "ollama".to_string(),
                content: "no".to_string(),
                usage: None,
            }, // 1.0
        ];

        let weights = BackendWeights::default();
        let result = weighted_vote(&responses, &weights).unwrap();

        assert_eq!(result.winner, "yes"); // 4.0 vs 1.0
        assert!(!result.was_tie);
    }

    #[test]
    fn test_whitespace_normalization() {
        let responses = vec![
            BackendResponse {
                backend: "claude".to_string(),
                content: "  yes  ".to_string(),
                usage: None,
            },
            BackendResponse {
                backend: "codex".to_string(),
                content: "yes".to_string(),
                usage: None,
            },
            BackendResponse {
                backend: "ollama".to_string(),
                content: "no".to_string(),
                usage: None,
            },
        ];

        let result = majority_vote(&responses).unwrap();
        assert_eq!(result.winner, "yes");
        assert_eq!(result.breakdown.get("yes"), Some(&2)); // Both normalized to "yes"
    }
}
