//! JSON5 parsing for scenario files with implicit `enabled: true` for TC features.
//!
//! This module provides deserialization of scenario files from JSON5 format,
//! with the key feature that any TC config field present in the JSON automatically
//! gets `enabled: true` set, removing the need for verbose `enabled: true` in every config.
//!
//! Duration strings are supported using the `duration-string` crate format:
//! "50ms", "5s", "1m", "1h", "1d", "5m30s", etc.

use duration_string::DurationString;
use serde::Deserialize;

use crate::scenario::{NetworkScenario, ScenarioMetadata, ScenarioStep};
use crate::{
    TcCorruptConfig, TcDelayConfig, TcDuplicateConfig, TcLossConfig, TcNetemConfig,
    TcRateLimitConfig, TcReorderConfig,
};

/// Parse a duration string like "50ms", "5s", "1m", "1h" into milliseconds
/// Uses the `duration-string` crate for parsing.
pub fn parse_duration_string(s: &str) -> Result<u64, String> {
    let duration: DurationString = s
        .trim()
        .parse()
        .map_err(|e| format!("Invalid duration '{}': {}", s, e))?;

    let std_duration: std::time::Duration = duration.into();
    Ok(std_duration.as_millis() as u64)
}

/// Error type for scenario JSON5 parsing
#[derive(Debug)]
pub enum ScenarioParseError {
    /// JSON5 parsing error
    Json5Error(String),
    /// Validation error after parsing
    ValidationError(String),
    /// File I/O error
    IoError(String),
}

impl std::fmt::Display for ScenarioParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioParseError::Json5Error(msg) => write!(f, "JSON5 parse error: {}", msg),
            ScenarioParseError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ScenarioParseError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for ScenarioParseError {}

impl From<std::io::Error> for ScenarioParseError {
    fn from(err: std::io::Error) -> Self {
        ScenarioParseError::IoError(err.to_string())
    }
}

/// Intermediate struct for JSON5 deserialization of a scenario file
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub loop_scenario: bool,
    #[serde(default)]
    pub metadata: ScenarioMetadataJson,
    pub steps: Vec<ScenarioStepJson>,
}

/// Intermediate struct for scenario metadata
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScenarioMetadataJson {
    #[serde(default)]
    pub tags: Vec<String>,
    pub author: Option<String>,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    "1.0".to_string()
}

/// Intermediate struct for a scenario step
#[derive(Debug, Clone, Deserialize)]
pub struct ScenarioStepJson {
    /// Duration as a string like "30s", "500ms", "1m"
    pub duration: String,
    pub description: String,
    #[serde(default)]
    pub tc_config: TcConfigJson,
}

/// Intermediate struct for TC config with implicit enabled
/// Any field present automatically means enabled=true
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TcConfigJson {
    pub loss: Option<LossConfigJson>,
    pub delay: Option<DelayConfigJson>,
    pub duplicate: Option<DuplicateConfigJson>,
    pub reorder: Option<ReorderConfigJson>,
    pub corrupt: Option<CorruptConfigJson>,
    pub rate_limit: Option<RateLimitConfigJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LossConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DelayConfigJson {
    #[serde(default)]
    pub base_ms: f32,
    #[serde(default)]
    pub jitter_ms: f32,
    #[serde(default)]
    pub correlation: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DuplicateConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReorderConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
    #[serde(default = "default_gap")]
    pub gap: u32,
}

fn default_gap() -> u32 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub struct CorruptConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfigJson {
    #[serde(default = "default_rate")]
    pub rate_kbps: u32,
}

fn default_rate() -> u32 {
    1000
}

impl TcConfigJson {
    /// Convert to TcNetemConfig with implicit enabled=true for present fields
    pub fn to_tc_netem_config(&self) -> TcNetemConfig {
        TcNetemConfig {
            loss: match &self.loss {
                Some(loss) => TcLossConfig {
                    enabled: true, // Implicit!
                    percentage: loss.percentage,
                    correlation: loss.correlation,
                },
                None => TcLossConfig::default(),
            },
            delay: match &self.delay {
                Some(delay) => TcDelayConfig {
                    enabled: true, // Implicit!
                    base_ms: delay.base_ms,
                    jitter_ms: delay.jitter_ms,
                    correlation: delay.correlation,
                },
                None => TcDelayConfig::default(),
            },
            duplicate: match &self.duplicate {
                Some(dup) => TcDuplicateConfig {
                    enabled: true, // Implicit!
                    percentage: dup.percentage,
                    correlation: dup.correlation,
                },
                None => TcDuplicateConfig::default(),
            },
            reorder: match &self.reorder {
                Some(reorder) => TcReorderConfig {
                    enabled: true, // Implicit!
                    percentage: reorder.percentage,
                    correlation: reorder.correlation,
                    gap: reorder.gap,
                },
                None => TcReorderConfig {
                    enabled: false,
                    percentage: 0.0,
                    correlation: 0.0,
                    gap: 5, // Default gap
                },
            },
            corrupt: match &self.corrupt {
                Some(corrupt) => TcCorruptConfig {
                    enabled: true, // Implicit!
                    percentage: corrupt.percentage,
                    correlation: corrupt.correlation,
                },
                None => TcCorruptConfig::default(),
            },
            rate_limit: match &self.rate_limit {
                Some(rate) => TcRateLimitConfig {
                    enabled: true, // Implicit!
                    rate_kbps: rate.rate_kbps,
                },
                None => TcRateLimitConfig {
                    enabled: false,
                    rate_kbps: 1000, // Default rate
                },
            },
        }
    }
}

impl ScenarioFile {
    /// Convert to NetworkScenario
    /// Returns an error if any duration string is invalid
    pub fn to_network_scenario(self) -> Result<NetworkScenario, ScenarioParseError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut steps: Vec<ScenarioStep> = Vec::with_capacity(self.steps.len());
        for (i, step) in self.steps.into_iter().enumerate() {
            let duration_ms = parse_duration_string(&step.duration).map_err(|e| {
                ScenarioParseError::ValidationError(format!(
                    "Invalid duration '{}' in step {}: {}",
                    step.duration,
                    i + 1,
                    e
                ))
            })?;

            steps.push(ScenarioStep {
                duration_ms,
                description: step.description,
                tc_config: step.tc_config.to_tc_netem_config(),
            });
        }

        // Calculate total duration from steps
        let duration_ms: u64 = steps.iter().map(|s| s.duration_ms).sum();

        Ok(NetworkScenario {
            id: self.id,
            name: self.name,
            description: self.description,
            steps,
            loop_scenario: self.loop_scenario,
            created_at: now,
            modified_at: now,
            metadata: ScenarioMetadata {
                tags: self.metadata.tags,
                author: self.metadata.author,
                version: self.metadata.version,
                duration_ms,
            },
        })
    }
}

/// Parse a scenario from a JSON5 string
pub fn parse_scenario_json5(json5_content: &str) -> Result<ScenarioFile, ScenarioParseError> {
    json5::from_str(json5_content).map_err(|e| ScenarioParseError::Json5Error(e.to_string()))
}

/// Parse and convert a scenario from a JSON5 string
pub fn parse_scenario(json5_content: &str) -> Result<NetworkScenario, ScenarioParseError> {
    let scenario_file = parse_scenario_json5(json5_content)?;
    scenario_file.to_network_scenario()
}

/// Parse a scenario from a file path
pub fn parse_scenario_file(path: &std::path::Path) -> Result<NetworkScenario, ScenarioParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_scenario(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_string_milliseconds() {
        assert_eq!(parse_duration_string("50ms").unwrap(), 50);
        assert_eq!(parse_duration_string("100ms").unwrap(), 100);
        assert_eq!(parse_duration_string("1500ms").unwrap(), 1500);
    }

    #[test]
    fn test_parse_duration_string_seconds() {
        assert_eq!(parse_duration_string("1s").unwrap(), 1000);
        assert_eq!(parse_duration_string("5s").unwrap(), 5000);
        assert_eq!(parse_duration_string("30s").unwrap(), 30000);
        // Use 1500ms instead of 1.5s (crate doesn't support fractional values)
        assert_eq!(parse_duration_string("1500ms").unwrap(), 1500);
    }

    #[test]
    fn test_parse_duration_string_minutes() {
        assert_eq!(parse_duration_string("1m").unwrap(), 60000);
        assert_eq!(parse_duration_string("2m").unwrap(), 120000);
        // Compound format: 1m30s = 90 seconds
        assert_eq!(parse_duration_string("1m30s").unwrap(), 90000);
    }

    #[test]
    fn test_parse_duration_string_hours() {
        assert_eq!(parse_duration_string("1h").unwrap(), 3600000);
        assert_eq!(parse_duration_string("2h").unwrap(), 7200000);
    }

    #[test]
    fn test_parse_duration_string_compound() {
        // The duration-string crate supports compound formats
        assert_eq!(parse_duration_string("1h30m").unwrap(), 5400000);
        assert_eq!(parse_duration_string("2m30s").unwrap(), 150000);
    }

    #[test]
    fn test_parse_duration_string_with_whitespace() {
        assert_eq!(parse_duration_string("  30s  ").unwrap(), 30000);
    }

    #[test]
    fn test_parse_duration_string_invalid() {
        assert!(parse_duration_string("").is_err());
        assert!(parse_duration_string("abc").is_err());
        assert!(parse_duration_string("10x").is_err());
    }

    #[test]
    fn test_parse_minimal_scenario() {
        let json5 = r#"
        {
            id: "test",
            name: "Test Scenario",
            steps: [
                {
                    duration: "30s",
                    description: "Initial state",
                    tc_config: {}
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert_eq!(scenario.id, "test");
        assert_eq!(scenario.name, "Test Scenario");
        assert_eq!(scenario.steps.len(), 1);
        assert_eq!(scenario.steps[0].duration_ms, 30000);
        assert!(!scenario.steps[0].tc_config.loss.enabled);
        assert!(!scenario.steps[0].tc_config.delay.enabled);
    }

    #[test]
    fn test_parse_with_loss_implicit_enabled() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "5s",
                    description: "Loss enabled",
                    tc_config: {
                        loss: { percentage: 5 }
                    }
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert!(scenario.steps[0].tc_config.loss.enabled);
        assert_eq!(scenario.steps[0].tc_config.loss.percentage, 5.0);
        assert!(!scenario.steps[0].tc_config.delay.enabled);
    }

    #[test]
    fn test_parse_with_multiple_features() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "1m",
                    description: "Multiple features",
                    tc_config: {
                        loss: { percentage: 5, correlation: 10 },
                        delay: { base_ms: 100, jitter_ms: 20 },
                        rate_limit: { rate_kbps: 1000 }
                    }
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        let tc = &scenario.steps[0].tc_config;

        assert!(tc.loss.enabled);
        assert_eq!(tc.loss.percentage, 5.0);
        assert_eq!(tc.loss.correlation, 10.0);

        assert!(tc.delay.enabled);
        assert_eq!(tc.delay.base_ms, 100.0);
        assert_eq!(tc.delay.jitter_ms, 20.0);

        assert!(tc.rate_limit.enabled);
        assert_eq!(tc.rate_limit.rate_kbps, 1000);

        assert!(!tc.duplicate.enabled);
        assert!(!tc.reorder.enabled);
        assert!(!tc.corrupt.enabled);
    }

    #[test]
    fn test_parse_with_metadata() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            description: "A test scenario",
            metadata: {
                tags: ["test", "demo"],
                author: "Test Author",
                version: "2.0"
            },
            steps: [
                { duration: "10s", description: "Step 1", tc_config: {} }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert_eq!(scenario.description, "A test scenario");
        assert_eq!(scenario.metadata.tags, vec!["test", "demo"]);
        assert_eq!(scenario.metadata.author, Some("Test Author".to_string()));
        assert_eq!(scenario.metadata.version, "2.0");
    }

    #[test]
    fn test_parse_with_various_duration_formats() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                { duration: "500ms", description: "Milliseconds", tc_config: {} },
                { duration: "5s", description: "Seconds", tc_config: {} },
                { duration: "1m", description: "Minutes", tc_config: {} }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert_eq!(scenario.steps[0].duration_ms, 500);
        assert_eq!(scenario.steps[1].duration_ms, 5000);
        assert_eq!(scenario.steps[2].duration_ms, 60000);
        // Total: 500 + 5000 + 60000 = 65500
        assert_eq!(scenario.metadata.duration_ms, 65500);
    }

    #[test]
    fn test_parse_loop_scenario() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            loop_scenario: true,
            steps: [
                { duration: "10s", description: "Step 1", tc_config: {} }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert!(scenario.loop_scenario);
    }

    #[test]
    fn test_parse_reorder_with_gap() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "30s",
                    description: "Reorder",
                    tc_config: {
                        reorder: { percentage: 5, gap: 3 }
                    }
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert!(scenario.steps[0].tc_config.reorder.enabled);
        assert_eq!(scenario.steps[0].tc_config.reorder.percentage, 5.0);
        assert_eq!(scenario.steps[0].tc_config.reorder.gap, 3);
    }

    #[test]
    fn test_parse_empty_tc_config_clears_all() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "10s",
                    description: "All disabled",
                    tc_config: {}
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        let tc = &scenario.steps[0].tc_config;

        assert!(!tc.loss.enabled);
        assert!(!tc.delay.enabled);
        assert!(!tc.duplicate.enabled);
        assert!(!tc.reorder.enabled);
        assert!(!tc.corrupt.enabled);
        assert!(!tc.rate_limit.enabled);
    }

    #[test]
    fn test_parse_json5_with_comments() {
        let json5 = r#"
        {
            // This is a comment
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "30s",
                    description: "Step", // trailing comment
                    tc_config: {
                        loss: { percentage: 5 }, // enable loss
                    }
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert_eq!(scenario.id, "test");
        assert!(scenario.steps[0].tc_config.loss.enabled);
    }

    #[test]
    fn test_parse_json5_with_trailing_comma() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "10s",
                    description: "Step",
                    tc_config: {
                        loss: { percentage: 5, },
                    },
                },
            ],
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert_eq!(scenario.id, "test");
    }

    #[test]
    fn test_duration_calculation() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                { duration: "10s", description: "Step 1", tc_config: {} },
                { duration: "20s", description: "Step 2", tc_config: {} },
                { duration: "30s", description: "Step 3", tc_config: {} }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        // Total: 10000 + 20000 + 30000 = 60000
        assert_eq!(scenario.metadata.duration_ms, 60000);
    }

    #[test]
    fn test_invalid_duration_error() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                { duration: "invalid", description: "Bad step", tc_config: {} }
            ]
        }
        "#;

        let result = parse_scenario(json5);
        assert!(result.is_err());
    }
}
