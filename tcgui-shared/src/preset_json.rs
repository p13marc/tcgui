//! JSON5 parsing for custom preset files with implicit `enabled: true` for TC features.
//!
//! This module provides deserialization of preset files from JSON5 format,
//! with the key feature that any TC config field present in the JSON automatically
//! gets `enabled: true` set, removing the need for verbose `enabled: true` in every config.
//!
//! # Example Preset File
//!
//! ```json5
//! {
//!     id: "office-vpn",
//!     name: "Office VPN",
//!     description: "Typical VPN connection to office",
//!     delay: { base_ms: 45, jitter_ms: 8 },
//!     loss: { percentage: 0.3 }
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::scenario_json::{
    CorruptConfigJson, DelayConfigJson, DuplicateConfigJson, LossConfigJson, RateLimitConfigJson,
    ReorderConfigJson,
};
use crate::{
    TcCorruptConfig, TcDelayConfig, TcDuplicateConfig, TcLossConfig, TcNetemConfig,
    TcRateLimitConfig, TcReorderConfig,
};

/// Error type for preset JSON5 parsing
#[derive(Debug)]
pub enum PresetParseError {
    /// JSON5 parsing error
    Json5Error(String),
    /// Validation error after parsing
    ValidationError(String),
    /// File I/O error
    IoError(String),
}

impl std::fmt::Display for PresetParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetParseError::Json5Error(msg) => write!(f, "JSON5 parse error: {}", msg),
            PresetParseError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            PresetParseError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for PresetParseError {}

impl From<std::io::Error> for PresetParseError {
    fn from(err: std::io::Error) -> Self {
        PresetParseError::IoError(err.to_string())
    }
}

/// A custom preset loaded from a JSON5 file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomPreset {
    /// Unique identifier for the preset
    pub id: String,
    /// Display name
    pub name: String,
    /// Description of the preset
    #[serde(default)]
    pub description: String,
    /// The TC configuration
    #[serde(flatten)]
    pub config: TcNetemConfig,
}

/// Intermediate struct for JSON5 deserialization of a preset file
/// This allows for implicit `enabled: true` when a TC feature is present
#[derive(Debug, Clone, Deserialize)]
pub struct PresetFile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Loss configuration (presence implies enabled)
    pub loss: Option<LossConfigJson>,
    /// Delay configuration (presence implies enabled)
    pub delay: Option<DelayConfigJson>,
    /// Duplicate configuration (presence implies enabled)
    pub duplicate: Option<DuplicateConfigJson>,
    /// Reorder configuration (presence implies enabled)
    pub reorder: Option<ReorderConfigJson>,
    /// Corrupt configuration (presence implies enabled)
    pub corrupt: Option<CorruptConfigJson>,
    /// Rate limit configuration (presence implies enabled)
    pub rate_limit: Option<RateLimitConfigJson>,
}

impl PresetFile {
    /// Convert to CustomPreset with implicit enabled=true for present fields
    pub fn to_custom_preset(self) -> CustomPreset {
        let config = TcNetemConfig {
            loss: match self.loss {
                Some(loss) => TcLossConfig {
                    enabled: true,
                    percentage: loss.percentage,
                    correlation: loss.correlation,
                },
                None => TcLossConfig::default(),
            },
            delay: match self.delay {
                Some(delay) => TcDelayConfig {
                    enabled: true,
                    base_ms: delay.base_ms,
                    jitter_ms: delay.jitter_ms,
                    correlation: delay.correlation,
                },
                None => TcDelayConfig::default(),
            },
            duplicate: match self.duplicate {
                Some(dup) => TcDuplicateConfig {
                    enabled: true,
                    percentage: dup.percentage,
                    correlation: dup.correlation,
                },
                None => TcDuplicateConfig::default(),
            },
            reorder: match self.reorder {
                Some(reorder) => TcReorderConfig {
                    enabled: true,
                    percentage: reorder.percentage,
                    correlation: reorder.correlation,
                    gap: reorder.gap,
                },
                None => TcReorderConfig {
                    enabled: false,
                    percentage: 0.0,
                    correlation: 0.0,
                    gap: 5,
                },
            },
            corrupt: match self.corrupt {
                Some(corrupt) => TcCorruptConfig {
                    enabled: true,
                    percentage: corrupt.percentage,
                    correlation: corrupt.correlation,
                },
                None => TcCorruptConfig::default(),
            },
            rate_limit: match self.rate_limit {
                Some(rate) => TcRateLimitConfig {
                    enabled: true,
                    rate_kbps: rate.rate_kbps,
                },
                None => TcRateLimitConfig {
                    enabled: false,
                    rate_kbps: 1000,
                },
            },
        };

        CustomPreset {
            id: self.id,
            name: self.name,
            description: self.description,
            config,
        }
    }
}

/// Parse a preset from a JSON5 string
pub fn parse_preset_json5(json5_content: &str) -> Result<PresetFile, PresetParseError> {
    json5::from_str(json5_content).map_err(|e| PresetParseError::Json5Error(e.to_string()))
}

/// Parse and convert a preset from a JSON5 string
pub fn parse_preset(json5_content: &str) -> Result<CustomPreset, PresetParseError> {
    let preset_file = parse_preset_json5(json5_content)?;
    Ok(preset_file.to_custom_preset())
}

/// Parse a preset from a file path
pub fn parse_preset_file(path: &std::path::Path) -> Result<CustomPreset, PresetParseError> {
    let content = std::fs::read_to_string(path)?;
    parse_preset(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_preset() {
        let json5 = r#"
        {
            id: "test",
            name: "Test Preset"
        }
        "#;

        let preset = parse_preset(json5).unwrap();
        assert_eq!(preset.id, "test");
        assert_eq!(preset.name, "Test Preset");
        assert!(!preset.config.loss.enabled);
        assert!(!preset.config.delay.enabled);
    }

    #[test]
    fn test_parse_with_loss_implicit_enabled() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            loss: { percentage: 5 }
        }
        "#;

        let preset = parse_preset(json5).unwrap();
        assert!(preset.config.loss.enabled);
        assert_eq!(preset.config.loss.percentage, 5.0);
        assert!(!preset.config.delay.enabled);
    }

    #[test]
    fn test_parse_with_multiple_features() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            description: "A test preset",
            loss: { percentage: 5, correlation: 10 },
            delay: { base_ms: 100, jitter_ms: 20 },
            rate_limit: { rate_kbps: 1000 }
        }
        "#;

        let preset = parse_preset(json5).unwrap();
        assert_eq!(preset.description, "A test preset");

        assert!(preset.config.loss.enabled);
        assert_eq!(preset.config.loss.percentage, 5.0);
        assert_eq!(preset.config.loss.correlation, 10.0);

        assert!(preset.config.delay.enabled);
        assert_eq!(preset.config.delay.base_ms, 100.0);
        assert_eq!(preset.config.delay.jitter_ms, 20.0);

        assert!(preset.config.rate_limit.enabled);
        assert_eq!(preset.config.rate_limit.rate_kbps, 1000);

        assert!(!preset.config.duplicate.enabled);
        assert!(!preset.config.reorder.enabled);
        assert!(!preset.config.corrupt.enabled);
    }

    #[test]
    fn test_parse_reorder_with_gap() {
        let json5 = r#"
        {
            id: "test",
            name: "Test",
            reorder: { percentage: 5, gap: 3 }
        }
        "#;

        let preset = parse_preset(json5).unwrap();
        assert!(preset.config.reorder.enabled);
        assert_eq!(preset.config.reorder.percentage, 5.0);
        assert_eq!(preset.config.reorder.gap, 3);
    }

    #[test]
    fn test_parse_json5_with_comments() {
        let json5 = r#"
        {
            // This is a comment
            id: "test",
            name: "Test",
            loss: { percentage: 5 }, // trailing comment
        }
        "#;

        let preset = parse_preset(json5).unwrap();
        assert_eq!(preset.id, "test");
        assert!(preset.config.loss.enabled);
    }

    #[test]
    fn test_full_preset_example() {
        let json5 = r#"
        {
            id: "stress-test",
            name: "Network Stress Test",
            description: "Extreme conditions for stress testing",
            loss: { percentage: 20 },
            delay: { base_ms: 500, jitter_ms: 200 },
            duplicate: { percentage: 5 },
            corrupt: { percentage: 2 },
            rate_limit: { rate_kbps: 256 }
        }
        "#;

        let preset = parse_preset(json5).unwrap();
        assert_eq!(preset.id, "stress-test");
        assert!(preset.config.loss.enabled);
        assert!(preset.config.delay.enabled);
        assert!(preset.config.duplicate.enabled);
        assert!(preset.config.corrupt.enabled);
        assert!(preset.config.rate_limit.enabled);
        assert!(!preset.config.reorder.enabled);
    }
}
