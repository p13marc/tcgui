use serde::{Deserialize, Serialize};

/// Predefined network condition presets for common scenarios
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum NetworkPreset {
    /// User-defined custom settings
    #[default]
    Custom,
    /// High latency satellite connection (500ms delay, low loss)
    SatelliteLink,
    /// Mobile/cellular network (medium delay, jitter, some reorder)
    CellularNetwork,
    /// Poor WiFi connection (high loss, some corruption)
    PoorWiFi,
    /// WAN link simulation (medium delay, low loss)
    WanLink,
    /// Unreliable connection (high loss, duplication, reorder)
    UnreliableConnection,
    /// High latency with bandwidth constraints
    HighLatencyLowBandwidth,
    /// Testing scenario with all features
    TestAll,
}

/// Complete configuration for a network preset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetConfiguration {
    /// Display name of the preset
    pub name: String,
    /// Description of what this preset simulates
    pub description: String,
    /// Packet loss percentage (0.0-100.0)
    pub loss: f32,
    /// Loss correlation percentage
    pub correlation: Option<f32>,
    /// Base delay in milliseconds
    pub delay_ms: Option<f32>,
    /// Delay jitter in milliseconds
    pub delay_jitter_ms: Option<f32>,
    /// Delay correlation percentage
    pub delay_correlation: Option<f32>,
    /// Packet duplication percentage
    pub duplicate_percent: Option<f32>,
    /// Duplication correlation percentage
    pub duplicate_correlation: Option<f32>,
    /// Packet reordering percentage
    pub reorder_percent: Option<f32>,
    /// Reordering correlation percentage
    pub reorder_correlation: Option<f32>,
    /// Reordering gap parameter
    pub reorder_gap: Option<u32>,
    /// Packet corruption percentage
    pub corrupt_percent: Option<f32>,
    /// Corruption correlation percentage
    pub corrupt_correlation: Option<f32>,
    /// Rate limiting in kbps
    pub rate_limit_kbps: Option<u32>,
    /// Whether delay feature should be enabled
    pub delay_enabled: bool,
    /// Whether duplication feature should be enabled
    pub duplicate_enabled: bool,
    /// Whether reordering feature should be enabled
    pub reorder_enabled: bool,
    /// Whether corruption feature should be enabled
    pub corrupt_enabled: bool,
    /// Whether rate limiting feature should be enabled
    pub rate_limit_enabled: bool,
}

impl NetworkPreset {
    /// Get the configuration for this preset
    pub fn get_configuration(&self) -> PresetConfiguration {
        match self {
            NetworkPreset::Custom => PresetConfiguration {
                name: "Custom".to_string(),
                description: "User-defined custom settings".to_string(),
                loss: 0.0,
                correlation: None,
                delay_ms: None,
                delay_jitter_ms: None,
                delay_correlation: None,
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: None,
                reorder_correlation: None,
                reorder_gap: None,
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: None,
                delay_enabled: false,
                duplicate_enabled: false,
                reorder_enabled: false,
                corrupt_enabled: false,
                rate_limit_enabled: false,
            },

            NetworkPreset::SatelliteLink => PresetConfiguration {
                name: "Satellite Link".to_string(),
                description: "High latency satellite connection (500ms delay, reliable)"
                    .to_string(),
                loss: 1.0,
                correlation: Some(10.0),
                delay_ms: Some(500.0),
                delay_jitter_ms: Some(20.0),
                delay_correlation: Some(25.0),
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: None,
                reorder_correlation: None,
                reorder_gap: None,
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: Some(2000), // 2 Mbps typical satellite
                delay_enabled: true,
                duplicate_enabled: false,
                reorder_enabled: false,
                corrupt_enabled: false,
                rate_limit_enabled: true,
            },

            NetworkPreset::CellularNetwork => PresetConfiguration {
                name: "Cellular Network".to_string(),
                description: "Mobile network with variable latency and some packet reordering"
                    .to_string(),
                loss: 2.0,
                correlation: Some(15.0),
                delay_ms: Some(150.0),
                delay_jitter_ms: Some(50.0),
                delay_correlation: Some(30.0),
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: Some(5.0),
                reorder_correlation: Some(20.0),
                reorder_gap: Some(3),
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: Some(10000), // 10 Mbps
                delay_enabled: true,
                duplicate_enabled: false,
                reorder_enabled: true,
                corrupt_enabled: false,
                rate_limit_enabled: true,
            },

            NetworkPreset::PoorWiFi => PresetConfiguration {
                name: "Poor WiFi".to_string(),
                description: "Congested WiFi with packet loss and occasional corruption"
                    .to_string(),
                loss: 8.0,
                correlation: Some(25.0),
                delay_ms: Some(80.0),
                delay_jitter_ms: Some(30.0),
                delay_correlation: Some(15.0),
                duplicate_percent: Some(1.0),
                duplicate_correlation: Some(10.0),
                reorder_percent: Some(3.0),
                reorder_correlation: Some(15.0),
                reorder_gap: Some(2),
                corrupt_percent: Some(0.5),
                corrupt_correlation: Some(5.0),
                rate_limit_kbps: None,
                delay_enabled: true,
                duplicate_enabled: true,
                reorder_enabled: true,
                corrupt_enabled: true,
                rate_limit_enabled: false,
            },

            NetworkPreset::WanLink => PresetConfiguration {
                name: "WAN Link".to_string(),
                description: "Wide Area Network with moderate latency and low loss".to_string(),
                loss: 0.5,
                correlation: Some(5.0),
                delay_ms: Some(50.0),
                delay_jitter_ms: Some(10.0),
                delay_correlation: Some(20.0),
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: None,
                reorder_correlation: None,
                reorder_gap: None,
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: Some(50000), // 50 Mbps
                delay_enabled: true,
                duplicate_enabled: false,
                reorder_enabled: false,
                corrupt_enabled: false,
                rate_limit_enabled: true,
            },

            NetworkPreset::UnreliableConnection => PresetConfiguration {
                name: "Unreliable Connection".to_string(),
                description: "Very poor network with high loss, duplication and reordering"
                    .to_string(),
                loss: 15.0,
                correlation: Some(30.0),
                delay_ms: Some(200.0),
                delay_jitter_ms: Some(100.0),
                delay_correlation: Some(40.0),
                duplicate_percent: Some(5.0),
                duplicate_correlation: Some(25.0),
                reorder_percent: Some(10.0),
                reorder_correlation: Some(30.0),
                reorder_gap: Some(5),
                corrupt_percent: Some(2.0),
                corrupt_correlation: Some(15.0),
                rate_limit_kbps: None,
                delay_enabled: true,
                duplicate_enabled: true,
                reorder_enabled: true,
                corrupt_enabled: true,
                rate_limit_enabled: false,
            },

            NetworkPreset::HighLatencyLowBandwidth => PresetConfiguration {
                name: "High Latency + Low Bandwidth".to_string(),
                description: "High delay with severe bandwidth constraints".to_string(),
                loss: 3.0,
                correlation: Some(20.0),
                delay_ms: Some(800.0),
                delay_jitter_ms: Some(50.0),
                delay_correlation: Some(35.0),
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: None,
                reorder_correlation: None,
                reorder_gap: None,
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: Some(512), // 512 kbps
                delay_enabled: true,
                duplicate_enabled: false,
                reorder_enabled: false,
                corrupt_enabled: false,
                rate_limit_enabled: true,
            },

            NetworkPreset::TestAll => PresetConfiguration {
                name: "Test All Features".to_string(),
                description: "Testing preset with all netem features enabled".to_string(),
                loss: 5.0,
                correlation: Some(15.0),
                delay_ms: Some(100.0),
                delay_jitter_ms: Some(20.0),
                delay_correlation: Some(25.0),
                duplicate_percent: Some(2.0),
                duplicate_correlation: Some(10.0),
                reorder_percent: Some(8.0),
                reorder_correlation: Some(20.0),
                reorder_gap: Some(3),
                corrupt_percent: Some(1.0),
                corrupt_correlation: Some(8.0),
                rate_limit_kbps: Some(5000), // 5 Mbps
                delay_enabled: true,
                duplicate_enabled: true,
                reorder_enabled: true,
                corrupt_enabled: true,
                rate_limit_enabled: true,
            },
        }
    }

    /// Get all available presets
    pub fn all_presets() -> Vec<NetworkPreset> {
        vec![
            NetworkPreset::Custom,
            NetworkPreset::SatelliteLink,
            NetworkPreset::CellularNetwork,
            NetworkPreset::PoorWiFi,
            NetworkPreset::WanLink,
            NetworkPreset::UnreliableConnection,
            NetworkPreset::HighLatencyLowBandwidth,
            NetworkPreset::TestAll,
        ]
    }

    /// Get display name for this preset
    pub fn display_name(&self) -> String {
        self.get_configuration().name
    }

    /// Get description for this preset
    pub fn description(&self) -> String {
        self.get_configuration().description
    }
}

impl std::fmt::Display for NetworkPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}
