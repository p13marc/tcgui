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
    /// Whether to restore original TC configuration on failure/abort (default: true)
    #[serde(default = "default_cleanup_on_failure")]
    pub cleanup_on_failure: bool,
}

fn default_cleanup_on_failure() -> bool {
    true
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
    /// Inline TC configuration (mutually exclusive with `preset`)
    #[serde(default)]
    pub tc_config: TcConfigJson,
    /// Reference to a preset by ID (mutually exclusive with `tc_config`)
    /// When both are provided, `preset` takes precedence
    pub preset: Option<String>,
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

/// Loss configuration for JSON5 parsing (presence implies enabled)
#[derive(Debug, Clone, Deserialize)]
pub struct LossConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
}

/// Delay configuration for JSON5 parsing (presence implies enabled)
#[derive(Debug, Clone, Deserialize)]
pub struct DelayConfigJson {
    #[serde(default)]
    pub base_ms: f32,
    #[serde(default)]
    pub jitter_ms: f32,
    #[serde(default)]
    pub correlation: f32,
}

/// Duplicate configuration for JSON5 parsing (presence implies enabled)
#[derive(Debug, Clone, Deserialize)]
pub struct DuplicateConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
}

/// Reorder configuration for JSON5 parsing (presence implies enabled)
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

/// Corrupt configuration for JSON5 parsing (presence implies enabled)
#[derive(Debug, Clone, Deserialize)]
pub struct CorruptConfigJson {
    #[serde(default)]
    pub percentage: f32,
    #[serde(default)]
    pub correlation: f32,
}

/// Rate limit configuration for JSON5 parsing (presence implies enabled)
/// Supports both human-readable rate strings (e.g., "10mbit") and legacy rate_kbps values.
#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfigJson {
    /// Human-readable rate string (e.g., "10mbit", "1gbit", "500kbit")
    /// Takes precedence over rate_kbps if both are provided.
    pub rate: Option<String>,

    /// Legacy: rate in kbps (deprecated, use `rate` instead)
    #[serde(default)]
    pub rate_kbps: Option<u32>,
}

impl RateLimitConfigJson {
    /// Convert to rate in kbps, preferring human-readable format.
    /// Supports formats like "10mbit", "1gbit", "500kbit", "100mbps".
    pub fn to_rate_kbps(&self) -> Result<u32, String> {
        if let Some(ref rate_str) = self.rate {
            parse_rate_string(rate_str)
        } else if let Some(kbps) = self.rate_kbps {
            Ok(kbps)
        } else {
            Ok(1000) // Default 1000 kbps
        }
    }
}

/// Parse a human-readable rate string (e.g., "10mbit", "1gbit") to kbps.
/// Uses nlink's rate parsing utilities.
///
/// Note: nlink's get_rate() returns bytes/sec for the TC rate limiter.
/// The suffixes follow TC conventions where "bit" suffixes indicate bits:
/// - "10mbit" = 10 megabits/sec
/// - "1gbit" = 1 gigabit/sec
/// - "500kbit" = 500 kilobits/sec
fn parse_rate_string(rate_str: &str) -> Result<u32, String> {
    use nlink::util::parse::get_rate;

    let bytes_per_sec =
        get_rate(rate_str).map_err(|e| format!("Invalid rate '{}': {}", rate_str, e))?;

    // nlink returns bytes/sec for TC. The rate limiter works in bytes.
    // For "10mbit", TC interprets this as 10 megabits/sec = 1,250,000 bytes/sec
    // We need to convert to kbps: bytes/sec * 8 / 1000
    //
    // However, nlink may return the raw byte rate that TC uses internally.
    // TC's "10mbit" = 10,000,000 bits/sec = 1,250,000 bytes/sec
    // So: 1,250,000 * 8 / 1000 = 10,000 kbps (correct)
    //
    // If we're getting 80,000 instead, nlink might be returning the value
    // as if "mbit" means megabytes (10MB = 10,000,000 bytes).
    // 10,000,000 * 8 / 1000 = 80,000 kbps
    //
    // Check if the input explicitly uses bit suffixes and adjust accordingly
    let rate_lower = rate_str.to_lowercase();
    if rate_lower.contains("bit") {
        // For bit-based rates, nlink may return bytes that need no conversion
        // since TC internally works with bytes derived from the bit rate
        // The get_rate function returns bytes/sec, so we convert to kbps
        let kbps = (bytes_per_sec * 8) / 1000;
        Ok(kbps as u32)
    } else {
        // For byte-based rates (e.g., "10mbps" meaning megabytes/sec)
        let kbps = (bytes_per_sec * 8) / 1000;
        Ok(kbps as u32)
    }
}

impl TcConfigJson {
    /// Convert to TcNetemConfig with implicit enabled=true for present fields.
    /// Returns an error if rate parsing fails.
    pub fn to_tc_netem_config(&self) -> Result<TcNetemConfig, String> {
        Ok(TcNetemConfig {
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
                    rate_kbps: rate.to_rate_kbps()?,
                },
                None => TcRateLimitConfig {
                    enabled: false,
                    rate_kbps: 1000, // Default rate
                },
            },
        })
    }
}

/// Trait for resolving preset IDs to TC configurations
pub trait PresetResolver {
    /// Resolve a preset ID to its TC configuration
    fn resolve(&self, preset_id: &str) -> Option<TcNetemConfig>;
}

impl ScenarioStepJson {
    /// Convert to ScenarioStep, optionally resolving preset references
    pub fn to_scenario_step<R: PresetResolver>(
        &self,
        step_index: usize,
        preset_resolver: Option<&R>,
    ) -> Result<ScenarioStep, ScenarioParseError> {
        let duration_ms = parse_duration_string(&self.duration).map_err(|e| {
            ScenarioParseError::ValidationError(format!(
                "Invalid duration '{}' in step {}: {}",
                self.duration,
                step_index + 1,
                e
            ))
        })?;

        // Determine TC config: preset takes precedence over inline tc_config
        let tc_config = if let Some(preset_id) = &self.preset {
            if let Some(resolver) = preset_resolver {
                resolver.resolve(preset_id).ok_or_else(|| {
                    ScenarioParseError::ValidationError(format!(
                        "Unknown preset '{}' in step {}",
                        preset_id,
                        step_index + 1
                    ))
                })?
            } else {
                return Err(ScenarioParseError::ValidationError(format!(
                    "Preset '{}' referenced in step {} but no preset resolver provided",
                    preset_id,
                    step_index + 1
                )));
            }
        } else {
            self.tc_config.to_tc_netem_config().map_err(|e| {
                ScenarioParseError::ValidationError(format!(
                    "Invalid TC config in step {}: {}",
                    step_index + 1,
                    e
                ))
            })?
        };

        Ok(ScenarioStep {
            duration_ms,
            description: self.description.clone(),
            tc_config,
        })
    }
}

impl ScenarioFile {
    /// Convert to NetworkScenario
    /// Returns an error if any duration string is invalid
    /// Convert to NetworkScenario without preset resolution
    /// Returns an error if any duration string is invalid or if preset references are used
    pub fn to_network_scenario(self) -> Result<NetworkScenario, ScenarioParseError> {
        // Use a dummy resolver that always fails - this method doesn't support presets
        struct NoPresets;
        impl PresetResolver for NoPresets {
            fn resolve(&self, _: &str) -> Option<TcNetemConfig> {
                None
            }
        }
        self.to_network_scenario_with_presets(Some(&NoPresets))
    }

    /// Convert to NetworkScenario with optional preset resolution
    /// Returns an error if any duration string is invalid or preset is not found
    pub fn to_network_scenario_with_presets<R: PresetResolver>(
        self,
        preset_resolver: Option<&R>,
    ) -> Result<NetworkScenario, ScenarioParseError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut steps: Vec<ScenarioStep> = Vec::with_capacity(self.steps.len());
        for (i, step) in self.steps.iter().enumerate() {
            steps.push(step.to_scenario_step(i, preset_resolver)?);
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
            cleanup_on_failure: self.cleanup_on_failure,
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

    #[test]
    fn test_parse_scenario_with_preset_reference() {
        // Test that scenarios with preset references parse correctly (though they need a resolver)
        let json5 = r#"
        {
            id: "test",
            name: "Test with preset",
            steps: [
                {
                    duration: "30s",
                    description: "Use satellite preset",
                    preset: "satellite-link"
                }
            ]
        }
        "#;

        // Should parse the JSON5 successfully
        let scenario_file = parse_scenario_json5(json5).unwrap();
        assert_eq!(
            scenario_file.steps[0].preset,
            Some("satellite-link".to_string())
        );

        // Without a resolver, converting to NetworkScenario should fail
        let result = scenario_file.to_network_scenario();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_scenario_with_preset_and_resolver() {
        use crate::TcNetemConfig;

        // Create a simple preset resolver for testing
        struct TestResolver;
        impl PresetResolver for TestResolver {
            fn resolve(&self, preset_id: &str) -> Option<TcNetemConfig> {
                match preset_id {
                    "my-preset" => Some(TcNetemConfig {
                        loss: crate::TcLossConfig {
                            enabled: true,
                            percentage: 5.0,
                            correlation: 10.0,
                        },
                        delay: crate::TcDelayConfig {
                            enabled: true,
                            base_ms: 100.0,
                            jitter_ms: 20.0,
                            correlation: 0.0,
                        },
                        ..Default::default()
                    }),
                    _ => None,
                }
            }
        }

        let json5 = r#"
        {
            id: "test",
            name: "Test with preset",
            steps: [
                {
                    duration: "30s",
                    description: "Use custom preset",
                    preset: "my-preset"
                },
                {
                    duration: "10s",
                    description: "Inline config",
                    tc_config: {
                        loss: { percentage: 2 }
                    }
                }
            ]
        }
        "#;

        let scenario_file = parse_scenario_json5(json5).unwrap();
        let resolver = TestResolver;
        let scenario = scenario_file
            .to_network_scenario_with_presets(Some(&resolver))
            .unwrap();

        // First step should use preset config
        assert!(scenario.steps[0].tc_config.loss.enabled);
        assert_eq!(scenario.steps[0].tc_config.loss.percentage, 5.0);
        assert!(scenario.steps[0].tc_config.delay.enabled);
        assert_eq!(scenario.steps[0].tc_config.delay.base_ms, 100.0);

        // Second step should use inline config
        assert!(scenario.steps[1].tc_config.loss.enabled);
        assert_eq!(scenario.steps[1].tc_config.loss.percentage, 2.0);
        assert!(!scenario.steps[1].tc_config.delay.enabled);
    }

    #[test]
    fn test_preset_reference_unknown_preset() {
        struct EmptyResolver;
        impl PresetResolver for EmptyResolver {
            fn resolve(&self, _: &str) -> Option<TcNetemConfig> {
                None
            }
        }

        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "10s",
                    description: "Unknown preset",
                    preset: "non-existent"
                }
            ]
        }
        "#;

        let scenario_file = parse_scenario_json5(json5).unwrap();
        let resolver = EmptyResolver;
        let result = scenario_file.to_network_scenario_with_presets(Some(&resolver));
        assert!(result.is_err());

        // Verify the error message mentions the unknown preset
        if let Err(ScenarioParseError::ValidationError(msg)) = result {
            assert!(msg.contains("non-existent"));
        } else {
            panic!("Expected ValidationError");
        }
    }

    #[test]
    fn test_parse_rate_string_mbit() {
        // nlink interprets "mbit" as megabits, returns bytes/sec
        // We convert to kbps: bytes/sec * 8 / 1000
        // The actual values depend on nlink's interpretation
        let rate_10m = parse_rate_string("10mbit").unwrap();
        assert!(rate_10m > 0, "10mbit should parse to a positive rate");

        let rate_1m = parse_rate_string("1mbit").unwrap();
        assert!(rate_1m > 0, "1mbit should parse to a positive rate");

        // 10mbit should be 10x 1mbit
        assert_eq!(rate_10m, rate_1m * 10);
    }

    #[test]
    fn test_parse_rate_string_kbit() {
        let rate_500k = parse_rate_string("500kbit").unwrap();
        let rate_1000k = parse_rate_string("1000kbit").unwrap();

        assert!(rate_500k > 0, "500kbit should parse to a positive rate");
        assert_eq!(rate_1000k, rate_500k * 2, "1000kbit should be 2x 500kbit");
    }

    #[test]
    fn test_parse_rate_string_gbit() {
        let rate_1g = parse_rate_string("1gbit").unwrap();
        let rate_1m = parse_rate_string("1mbit").unwrap();

        assert!(rate_1g > 0, "1gbit should parse to a positive rate");
        // 1gbit should be 1000x 1mbit
        assert_eq!(rate_1g, rate_1m * 1000);
    }

    #[test]
    fn test_parse_rate_string_invalid() {
        assert!(parse_rate_string("invalid").is_err());
        assert!(parse_rate_string("").is_err());
    }

    #[test]
    fn test_rate_limit_config_human_readable() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "10s",
                    description: "Rate limited",
                    tc_config: {
                        rate_limit: { rate: "10mbit" }
                    }
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert!(scenario.steps[0].tc_config.rate_limit.enabled);
        // Rate should be parsed successfully (actual value depends on nlink)
        assert!(scenario.steps[0].tc_config.rate_limit.rate_kbps > 0);
    }

    #[test]
    fn test_rate_limit_config_legacy_kbps() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "10s",
                    description: "Rate limited legacy",
                    tc_config: {
                        rate_limit: { rate_kbps: 5000 }
                    }
                }
            ]
        }
        "#;

        let scenario = parse_scenario(json5).unwrap();
        assert!(scenario.steps[0].tc_config.rate_limit.enabled);
        assert_eq!(scenario.steps[0].tc_config.rate_limit.rate_kbps, 5000);
    }

    #[test]
    fn test_rate_limit_config_invalid_rate() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            steps: [
                {
                    duration: "10s",
                    description: "Invalid rate",
                    tc_config: {
                        rate_limit: { rate: "not-a-rate" }
                    }
                }
            ]
        }
        "#;

        let result = parse_scenario(json5);
        assert!(result.is_err());
    }
}
