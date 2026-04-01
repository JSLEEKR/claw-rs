//! Usage tracking for token consumption and cost estimation
//!
//! Tracks input/output/cache tokens per turn and estimates costs
//! based on model pricing.

use serde::{Deserialize, Serialize};

/// Token usage for a single API call
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens consumed
    pub input_tokens: usize,
    /// Output tokens generated
    pub output_tokens: usize,
    /// Tokens read from cache
    pub cache_read: usize,
    /// Tokens written to cache
    pub cache_write: usize,
}

impl TokenUsage {
    /// Total tokens (input + output)
    pub fn total(&self) -> usize {
        self.input_tokens + self.output_tokens
    }

    /// Total including cache operations
    pub fn total_with_cache(&self) -> usize {
        self.input_tokens + self.output_tokens + self.cache_read + self.cache_write
    }

    /// Add another usage record
    pub fn add(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

/// Per-turn usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnUsage {
    /// Turn number (1-indexed)
    pub turn: usize,
    /// Token usage for this turn
    pub usage: TokenUsage,
    /// Model used for this turn
    pub model: String,
    /// Estimated cost in USD
    pub cost_usd: f64,
}

/// Model pricing information (per million tokens)
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// Model name pattern
    pub model_pattern: String,
    /// Cost per million input tokens
    pub input_per_million: f64,
    /// Cost per million output tokens
    pub output_per_million: f64,
    /// Cost per million cache read tokens
    pub cache_read_per_million: f64,
    /// Cost per million cache write tokens
    pub cache_write_per_million: f64,
}

/// Known model pricing table
fn get_pricing(model: &str) -> ModelPricing {
    let model_lower = model.to_lowercase();

    if model_lower.contains("opus") {
        ModelPricing {
            model_pattern: "opus".to_string(),
            input_per_million: 15.0,
            output_per_million: 75.0,
            cache_read_per_million: 1.5,
            cache_write_per_million: 18.75,
        }
    } else if model_lower.contains("haiku") {
        ModelPricing {
            model_pattern: "haiku".to_string(),
            input_per_million: 0.25,
            output_per_million: 1.25,
            cache_read_per_million: 0.025,
            cache_write_per_million: 0.3,
        }
    } else {
        // Default to Sonnet pricing
        ModelPricing {
            model_pattern: "sonnet".to_string(),
            input_per_million: 3.0,
            output_per_million: 15.0,
            cache_read_per_million: 0.3,
            cache_write_per_million: 3.75,
        }
    }
}

/// Calculate cost for a token usage with a given model
pub fn calculate_cost(usage: &TokenUsage, model: &str) -> f64 {
    let pricing = get_pricing(model);
    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * pricing.input_per_million;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
    let cache_read_cost = (usage.cache_read as f64 / 1_000_000.0) * pricing.cache_read_per_million;
    let cache_write_cost = (usage.cache_write as f64 / 1_000_000.0) * pricing.cache_write_per_million;
    input_cost + output_cost + cache_read_cost + cache_write_cost
}

/// Format a cost as a USD string
pub fn format_cost(cost: f64) -> String {
    // Normalize negative zero to positive zero, and clamp negative values
    let cost = if cost <= 0.0 { 0.0 } else { cost };
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Session-level usage tracker
pub struct UsageTracker {
    /// Per-turn usage records
    turns: Vec<TurnUsage>,
    /// Cumulative usage
    cumulative: TokenUsage,
    /// Current model
    model: String,
}

impl UsageTracker {
    /// Create a new tracker for a given model
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            turns: Vec::new(),
            cumulative: TokenUsage::default(),
            model: model.into(),
        }
    }

    /// Record usage for a turn
    pub fn record_turn(&mut self, usage: TokenUsage) {
        let turn = self.turns.len() + 1;
        let cost = calculate_cost(&usage, &self.model);
        self.cumulative.add(&usage);
        self.turns.push(TurnUsage {
            turn,
            usage,
            model: self.model.clone(),
            cost_usd: cost,
        });
    }

    /// Get cumulative usage
    pub fn cumulative(&self) -> &TokenUsage {
        &self.cumulative
    }

    /// Get all turn records
    pub fn turns(&self) -> &[TurnUsage] {
        &self.turns
    }

    /// Get total cost in USD
    pub fn total_cost(&self) -> f64 {
        let sum: f64 = self.turns.iter().map(|t| t.cost_usd).sum();
        if sum == 0.0 { 0.0 } else { sum }
    }

    /// Format total cost as USD string
    pub fn total_cost_formatted(&self) -> String {
        format_cost(self.total_cost())
    }

    /// Get number of turns recorded
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Get the current model
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Set the model (for model changes mid-session)
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    /// Generate a summary of usage
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Model: {}", self.model));
        lines.push(format!("Turns: {}", self.turn_count()));
        lines.push(format!(
            "Input tokens: {}",
            self.cumulative.input_tokens
        ));
        lines.push(format!(
            "Output tokens: {}",
            self.cumulative.output_tokens
        ));
        if self.cumulative.cache_read > 0 {
            lines.push(format!("Cache read: {}", self.cumulative.cache_read));
        }
        if self.cumulative.cache_write > 0 {
            lines.push(format!("Cache write: {}", self.cumulative.cache_write));
        }
        lines.push(format!(
            "Total tokens: {}",
            self.cumulative.total()
        ));
        lines.push(format!("Estimated cost: {}", self.total_cost_formatted()));
        lines.join("\n")
    }
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new("claude-sonnet-4-20250514")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_usage_total() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read: 0,
            cache_write: 0,
        };
        assert_eq!(usage.total(), 150);
    }

    #[test]
    fn test_token_usage_total_with_cache() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read: 20,
            cache_write: 10,
        };
        assert_eq!(usage.total_with_cache(), 180);
    }

    #[test]
    fn test_token_usage_add() {
        let mut a = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read: 10,
            cache_write: 5,
        };
        let b = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read: 20,
            cache_write: 10,
        };
        a.add(&b);
        assert_eq!(a.input_tokens, 300);
        assert_eq!(a.output_tokens, 150);
        assert_eq!(a.cache_read, 30);
        assert_eq!(a.cache_write, 15);
    }

    #[test]
    fn test_calculate_cost_sonnet() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read: 0,
            cache_write: 0,
        };
        let cost = calculate_cost(&usage, "claude-sonnet-4-20250514");
        // Sonnet: $3/M input + $15/M output = $18
        assert!((cost - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_cost_opus() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read: 0,
            cache_write: 0,
        };
        let cost = calculate_cost(&usage, "claude-opus-4-20250514");
        // Opus: $15/M input + $75/M output = $90
        assert!((cost - 90.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_cost_haiku() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_read: 0,
            cache_write: 0,
        };
        let cost = calculate_cost(&usage, "claude-3-haiku");
        // Haiku: $0.25/M input + $1.25/M output = $1.50
        assert!((cost - 1.5).abs() < 0.01);
    }

    #[test]
    fn test_calculate_cost_with_cache() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 0,
            cache_read: 1_000_000,
            cache_write: 1_000_000,
        };
        let cost = calculate_cost(&usage, "claude-sonnet-4-20250514");
        // Sonnet: $3 input + $0.3 cache_read + $3.75 cache_write = $7.05
        assert!((cost - 7.05).abs() < 0.01);
    }

    #[test]
    fn test_format_cost_small() {
        assert_eq!(format_cost(0.001), "$0.0010");
    }

    #[test]
    fn test_format_cost_medium() {
        assert_eq!(format_cost(0.123), "$0.123");
    }

    #[test]
    fn test_format_cost_large() {
        assert_eq!(format_cost(12.5), "$12.50");
    }

    #[test]
    fn test_tracker_new() {
        let tracker = UsageTracker::new("claude-sonnet-4-20250514");
        assert_eq!(tracker.turn_count(), 0);
        assert_eq!(tracker.model(), "claude-sonnet-4-20250514");
        assert_eq!(tracker.cumulative().total(), 0);
    }

    #[test]
    fn test_tracker_record_turn() {
        let mut tracker = UsageTracker::new("claude-sonnet-4-20250514");
        tracker.record_turn(TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read: 0,
            cache_write: 0,
        });
        assert_eq!(tracker.turn_count(), 1);
        assert_eq!(tracker.cumulative().input_tokens, 1000);
        assert_eq!(tracker.cumulative().output_tokens, 500);
        assert!(tracker.total_cost() > 0.0);
    }

    #[test]
    fn test_tracker_multiple_turns() {
        let mut tracker = UsageTracker::new("claude-sonnet-4-20250514");
        for _ in 0..3 {
            tracker.record_turn(TokenUsage {
                input_tokens: 1000,
                output_tokens: 500,
                cache_read: 0,
                cache_write: 0,
            });
        }
        assert_eq!(tracker.turn_count(), 3);
        assert_eq!(tracker.cumulative().input_tokens, 3000);
        assert_eq!(tracker.cumulative().output_tokens, 1500);
    }

    #[test]
    fn test_tracker_summary() {
        let mut tracker = UsageTracker::new("claude-sonnet-4-20250514");
        tracker.record_turn(TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read: 0,
            cache_write: 0,
        });
        let summary = tracker.summary();
        assert!(summary.contains("sonnet"));
        assert!(summary.contains("Turns: 1"));
        assert!(summary.contains("Input tokens: 1000"));
        assert!(summary.contains("Output tokens: 500"));
    }

    #[test]
    fn test_tracker_set_model() {
        let mut tracker = UsageTracker::new("claude-sonnet-4-20250514");
        tracker.set_model("claude-opus-4-20250514");
        assert_eq!(tracker.model(), "claude-opus-4-20250514");
    }

    #[test]
    fn test_tracker_cost_formatted() {
        let tracker = UsageTracker::new("claude-sonnet-4-20250514");
        assert_eq!(tracker.total_cost_formatted(), "$0.0000");
    }

    #[test]
    fn test_zero_usage() {
        let usage = TokenUsage::default();
        assert_eq!(usage.total(), 0);
        assert_eq!(calculate_cost(&usage, "any-model"), 0.0);
    }

    #[test]
    fn test_format_cost_negative_zero() {
        // -0.0 should display as $0.0000, not $-0.0000
        assert_eq!(format_cost(-0.0), "$0.0000");
    }

    #[test]
    fn test_format_cost_zero() {
        assert_eq!(format_cost(0.0), "$0.0000");
    }
}
