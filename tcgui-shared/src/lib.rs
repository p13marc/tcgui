//! Shared types and message definitions for TC GUI.
//!
//! This crate contains all the shared data structures and message types
//! used for communication between the TC GUI frontend and backend components.
//! It provides a unified interface for network interface management, traffic control
//! operations, and real-time monitoring across multiple network namespaces.
//!
//! # New Architecture
//!
//! The communication uses separate topics and query/reply patterns with Zenoh:
//! - **Pub/Sub**: Interface discovery, bandwidth updates, health status
//! - **Query/Reply**: TC operations, interface control (request/reply pattern)
//!
//! # Key Components
//!
//! * [`topics`] - Key expressions for different communication channels
//! * [`InterfaceListUpdate`] - Interface discovery updates (pub/sub)
//! * [`BandwidthUpdate`] - Real-time bandwidth statistics (pub/sub)
//! * [`TcRequest`]/[`TcResponse`] - Traffic control operations (query/reply)
//! * [`InterfaceControlRequest`]/[`InterfaceControlResponse`] - Interface control (query/reply)
//! * [`NetworkInterface`] - Network interface representation with namespace context
//! * [`NetworkBandwidthStats`] - Real-time bandwidth statistics and rates
//! * [`NetworkNamespace`] - Network namespace grouping for interface organization
//!
//! # Communication Patterns
//!
//! ```text
//! Frontend                           Backend
//!    │ ──── Query: TcRequest ──────► │
//!    │ ◄──── Reply: TcResponse ───── │
//!    │                              │
//!    │ ──── Query: InterfaceControl ► │
//!    │ ◄──── Reply: InterfaceResponse │
//!    │                              │
//!    │ ◄─── Pub: InterfaceList ──── │
//!    │ ◄─── Pub: BandwidthUpdate ─── │
//!    │ ◄─── Pub: BackendHealth ───── │
//! ```
//!
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zenoh::config::WhatAmI;
use zenoh::key_expr::{
    format::{kedefine, keformat},
    KeyExpr, OwnedKeyExpr,
};

pub mod errors;
pub mod presets;
pub mod scenario;

/// Topic key expressions for the new communication architecture
pub mod topics {
    use super::*;

    // Define key expression patterns using kedefine! macro
    kedefine!(
        pub interface_list_keys: "tcgui/${backend:*}/interfaces/list",
        pub interface_events_keys: "tcgui/${backend:*}/interfaces/events",
        pub bandwidth_keys: "tcgui/${backend:*}/bandwidth/${namespace:*}/${interface:*}",
        pub health_keys: "tcgui/${backend:*}/health",
        pub tc_query_keys: "tcgui/${backend:*}/query/tc",
        pub interface_query_keys: "tcgui/${backend:*}/query/interface",
        pub tc_config_keys: "tcgui/${backend:*}/tc/${namespace:*}/${interface:*}",
        pub scenario_query_keys: "tcgui/${backend:*}/query/scenario",
        pub scenario_execution_query_keys: "tcgui/${backend:*}/query/scenario/execution",
        pub scenario_execution_updates_keys: "tcgui/${backend:*}/scenario/execution/${namespace:*}/${interface:*}"
    );

    kedefine!(
        pub all_interface_lists: "tcgui/${backend:*}/interfaces/list",
        pub all_interface_events: "tcgui/${backend:*}/interfaces/events",
        pub all_bandwidth: "tcgui/${backend:*}/bandwidth/${namespace:*}/${interface:*}",
        pub all_health: "tcgui/${backend:*}/health",
        pub backend_bandwidth_keys: "tcgui/${backend:*}/bandwidth/${ns:*}/${iface:*}",
        pub all_tc_configs: "tcgui/${backend:*}/tc/${namespace:*}/${interface:*}"
    );

    /// Get interface list topic key expression for a specific backend
    pub fn interface_list(backend_name: &str) -> OwnedKeyExpr {
        keformat!(interface_list_keys::formatter(), backend = backend_name)
            .expect("Failed to format interface list topic - this should never happen with valid backend name")
    }

    /// Get interface events topic key expression for a specific backend
    pub fn interface_events(backend_name: &str) -> OwnedKeyExpr {
        keformat!(interface_events_keys::formatter(), backend = backend_name)
            .expect("Failed to format interface events topic - this should never happen with valid backend name")
    }

    /// Get bandwidth updates topic key expression for a specific interface
    pub fn bandwidth_updates(backend_name: &str, namespace: &str, interface: &str) -> OwnedKeyExpr {
        keformat!(
            bandwidth_keys::formatter(),
            backend = backend_name,
            namespace = namespace,
            interface = interface
        )
        .expect("Failed to format bandwidth topic - this should never happen with valid parameters")
    }

    /// Get bandwidth updates wildcard pattern for a specific backend
    pub fn backend_bandwidth_pattern(backend_name: &str) -> OwnedKeyExpr {
        keformat!(
            backend_bandwidth_keys::formatter(),
            backend = backend_name,
            ns = "*",
            iface = "*"
        )
        .expect("Failed to format backend bandwidth pattern - this should never happen")
    }

    /// Get backend health status topic key expression
    pub fn backend_health(backend_name: &str) -> OwnedKeyExpr {
        keformat!(health_keys::formatter(), backend = backend_name).expect(
            "Failed to format health topic - this should never happen with valid backend name",
        )
    }

    /// Get TC operations query service key expression
    pub fn tc_query_service(backend_name: &str) -> OwnedKeyExpr {
        keformat!(tc_query_keys::formatter(), backend = backend_name).expect(
            "Failed to format TC query topic - this should never happen with valid backend name",
        )
    }

    /// Get interface control query service key expression
    pub fn interface_query_service(backend_name: &str) -> OwnedKeyExpr {
        keformat!(interface_query_keys::formatter(), backend = backend_name)
            .expect("Failed to format interface query topic - this should never happen with valid backend name")
    }

    /// Get TC configuration topic key expression for a specific interface
    pub fn tc_config(backend_name: &str, namespace: &str, interface: &str) -> OwnedKeyExpr {
        keformat!(
            tc_config_keys::formatter(),
            backend = backend_name,
            namespace = namespace,
            interface = interface
        )
        .expect("Failed to format TC config topic - this should never happen with valid parameters")
    }

    /// Get scenario management query service key expression
    pub fn scenario_query_service(backend_name: &str) -> OwnedKeyExpr {
        keformat!(scenario_query_keys::formatter(), backend = backend_name)
            .expect("Failed to format scenario query topic - this should never happen with valid backend name")
    }

    /// Get scenario execution query service key expression
    pub fn scenario_execution_query_service(backend_name: &str) -> OwnedKeyExpr {
        keformat!(scenario_execution_query_keys::formatter(), backend = backend_name)
            .expect("Failed to format scenario execution query topic - this should never happen with valid backend name")
    }

    /// Get scenario execution updates topic key expression for a specific interface
    pub fn scenario_execution_updates(
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> OwnedKeyExpr {
        keformat!(
            scenario_execution_updates_keys::formatter(),
            backend = backend_name,
            namespace = namespace,
            interface = interface
        )
        .expect("Failed to format scenario execution updates topic - this should never happen with valid parameters")
    }

    /// Extract backend name from a topic key expression
    pub fn extract_backend_name(key_expr: &KeyExpr) -> Option<String> {
        let key_str = key_expr.as_str();
        let parts: Vec<&str> = key_str.split('/').collect();
        if parts.len() >= 2 && parts[0] == "tcgui" {
            Some(parts[1].to_string())
        } else {
            None
        }
    }

    /// Extract namespace and interface from bandwidth topic
    pub fn extract_bandwidth_target(key_expr: &KeyExpr) -> Option<(String, String, String)> {
        let key_str = key_expr.as_str();
        let parts: Vec<&str> = key_str.split('/').collect();
        if parts.len() >= 5 && parts[0] == "tcgui" && parts[2] == "bandwidth" {
            Some((
                parts[1].to_string(),
                parts[3].to_string(),
                parts[4].to_string(),
            ))
        } else {
            None
        }
    }
}

/// Interface list update message (pub/sub)
/// Topic: tcgui/{backend_name}/interfaces/list
/// QoS: Reliable delivery, history depth=1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceListUpdate {
    /// List of network namespaces with their interfaces
    pub namespaces: Vec<NetworkNamespace>,
    /// Unix timestamp when this list was generated
    pub timestamp: u64,
    /// Backend name that generated this list
    pub backend_name: String,
}

/// Real-time bandwidth statistics (pub/sub)
/// Topic: tcgui/{backend_name}/bandwidth/{namespace}/{interface}
/// QoS: Best effort, no history (high frequency updates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthUpdate {
    /// Network namespace name
    pub namespace: String,
    /// Interface name
    pub interface: String,
    /// Current bandwidth statistics
    pub stats: NetworkBandwidthStats,
    /// Backend name that generated this update
    pub backend_name: String,
}

/// Interface state change event (pub/sub)
/// Topic: tcgui/{backend_name}/interfaces/events
/// QoS: Reliable delivery, history depth=10
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceStateEvent {
    /// Network namespace name
    pub namespace: String,
    /// Interface details
    pub interface: NetworkInterface,
    /// Type of state change that occurred
    pub event_type: InterfaceEventType,
    /// Unix timestamp when event occurred
    pub timestamp: u64,
    /// Backend name that detected this event
    pub backend_name: String,
}

/// Backend health and status information (pub/sub)
/// Topic: tcgui/{backend_name}/health
/// QoS: Reliable delivery, history depth=1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealthStatus {
    /// Backend identifier
    pub backend_name: String,
    /// Current status description
    pub status: String,
    /// Unix timestamp of this status update
    pub timestamp: u64,
    /// Backend metadata and capabilities
    pub metadata: BackendMetadata,
    /// Number of managed namespaces
    pub namespace_count: usize,
    /// Number of managed interfaces across all namespaces
    pub interface_count: usize,
}

/// Traffic Control configuration status (pub/sub)
/// Topic: tcgui/{backend_name}/tc/{namespace}/{interface}
/// QoS: Reliable delivery, history depth=1
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcConfigUpdate {
    /// Network namespace name
    pub namespace: String,
    /// Interface name
    pub interface: String,
    /// Backend name that manages this interface
    pub backend_name: String,
    /// Unix timestamp when this configuration was applied
    pub timestamp: u64,
    /// Current TC configuration (None if no TC configured)
    pub configuration: Option<TcConfiguration>,
    /// Whether the interface has any TC qdisc configured
    pub has_tc: bool,
}

/// Traffic control configuration request (Query)
/// Query Service: tcgui/{backend_name}/query/tc
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcRequest {
    /// Target network namespace
    pub namespace: String,
    /// Target interface name
    pub interface: String,
    /// TC operation to perform
    pub operation: TcOperation,
}

/// Structured TC configuration for all netem features
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TcNetemConfig {
    pub loss: TcLossConfig,
    pub delay: TcDelayConfig,
    pub duplicate: TcDuplicateConfig,
    pub reorder: TcReorderConfig,
    pub corrupt: TcCorruptConfig,
    pub rate_limit: TcRateLimitConfig,
}

/// Packet loss configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TcLossConfig {
    pub enabled: bool,
    pub percentage: f32,  // 0.0-100.0
    pub correlation: f32, // 0.0-100.0
}

/// Network delay configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TcDelayConfig {
    pub enabled: bool,
    pub base_ms: f32,     // 0.0-5000.0
    pub jitter_ms: f32,   // 0.0-1000.0
    pub correlation: f32, // 0.0-100.0
}

/// Packet duplication configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TcDuplicateConfig {
    pub enabled: bool,
    pub percentage: f32,  // 0.0-100.0
    pub correlation: f32, // 0.0-100.0
}

/// Packet reordering configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TcReorderConfig {
    pub enabled: bool,
    pub percentage: f32,  // 0.0-100.0
    pub correlation: f32, // 0.0-100.0
    pub gap: u32,         // 1-10
}

/// Packet corruption configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TcCorruptConfig {
    pub enabled: bool,
    pub percentage: f32,  // 0.0-100.0
    pub correlation: f32, // 0.0-100.0
}

/// Rate limiting configuration
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TcRateLimitConfig {
    pub enabled: bool,
    pub rate_kbps: u32, // 1-1000000
}

/// Validation trait for TC configuration structs
pub trait TcValidate {
    type Error: std::fmt::Display + std::fmt::Debug;
    fn validate(&self) -> Result<(), Self::Error>;
}

/// Validation error for TC configurations
#[derive(Debug, Clone)]
pub struct TcValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for TcValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Validation error in field '{}': {}",
            self.field, self.message
        )
    }
}

impl std::error::Error for TcValidationError {}

impl TcValidate for TcLossConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.percentage < 0.0 || self.percentage > 100.0 {
            return Err(TcValidationError {
                field: "percentage".to_string(),
                message: format!("Loss percentage must be 0.0-100.0, got {}", self.percentage),
            });
        }
        if self.correlation < 0.0 || self.correlation > 100.0 {
            return Err(TcValidationError {
                field: "correlation".to_string(),
                message: format!(
                    "Loss correlation must be 0.0-100.0, got {}",
                    self.correlation
                ),
            });
        }
        Ok(())
    }
}

impl TcValidate for TcDelayConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.base_ms < 0.0 || self.base_ms > 5000.0 {
            return Err(TcValidationError {
                field: "base_ms".to_string(),
                message: format!("Delay base must be 0.0-5000.0ms, got {}", self.base_ms),
            });
        }
        if self.jitter_ms < 0.0 || self.jitter_ms > 1000.0 {
            return Err(TcValidationError {
                field: "jitter_ms".to_string(),
                message: format!("Delay jitter must be 0.0-1000.0ms, got {}", self.jitter_ms),
            });
        }
        if self.correlation < 0.0 || self.correlation > 100.0 {
            return Err(TcValidationError {
                field: "correlation".to_string(),
                message: format!(
                    "Delay correlation must be 0.0-100.0, got {}",
                    self.correlation
                ),
            });
        }
        Ok(())
    }
}

impl TcValidate for TcDuplicateConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.percentage < 0.0 || self.percentage > 100.0 {
            return Err(TcValidationError {
                field: "percentage".to_string(),
                message: format!(
                    "Duplicate percentage must be 0.0-100.0, got {}",
                    self.percentage
                ),
            });
        }
        if self.correlation < 0.0 || self.correlation > 100.0 {
            return Err(TcValidationError {
                field: "correlation".to_string(),
                message: format!(
                    "Duplicate correlation must be 0.0-100.0, got {}",
                    self.correlation
                ),
            });
        }
        Ok(())
    }
}

impl TcValidate for TcReorderConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.enabled {
            if self.percentage < 0.0 || self.percentage > 100.0 {
                return Err(TcValidationError {
                    field: "percentage".to_string(),
                    message: format!(
                        "Reorder percentage must be 0.0-100.0, got {}",
                        self.percentage
                    ),
                });
            }
            if self.correlation < 0.0 || self.correlation > 100.0 {
                return Err(TcValidationError {
                    field: "correlation".to_string(),
                    message: format!(
                        "Reorder correlation must be 0.0-100.0, got {}",
                        self.correlation
                    ),
                });
            }
            if self.gap == 0 || self.gap > 10 {
                return Err(TcValidationError {
                    field: "gap".to_string(),
                    message: format!("Reorder gap must be 1-10, got {}", self.gap),
                });
            }
        }
        Ok(())
    }
}

impl TcValidate for TcCorruptConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.percentage < 0.0 || self.percentage > 100.0 {
            return Err(TcValidationError {
                field: "percentage".to_string(),
                message: format!(
                    "Corrupt percentage must be 0.0-100.0, got {}",
                    self.percentage
                ),
            });
        }
        if self.correlation < 0.0 || self.correlation > 100.0 {
            return Err(TcValidationError {
                field: "correlation".to_string(),
                message: format!(
                    "Corrupt correlation must be 0.0-100.0, got {}",
                    self.correlation
                ),
            });
        }
        Ok(())
    }
}

impl TcValidate for TcRateLimitConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.enabled && (self.rate_kbps == 0 || self.rate_kbps > 1000000) {
            return Err(TcValidationError {
                field: "rate_kbps".to_string(),
                message: format!(
                    "Rate limit must be 1-1000000 kbps when enabled, got {}",
                    self.rate_kbps
                ),
            });
        }
        Ok(())
    }
}

impl TcValidate for TcNetemConfig {
    type Error = TcValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.loss.validate()?;
        self.delay.validate()?;
        self.duplicate.validate()?;
        self.reorder.validate()?;
        self.corrupt.validate()?;
        self.rate_limit.validate()?;
        Ok(())
    }
}

/// Feature state wrapper for UI components
#[derive(Debug, Clone)]
pub struct FeatureState<T> {
    /// Whether the feature is enabled
    pub enabled: bool,
    /// Current configuration
    pub config: T,
    /// Whether an operation is pending
    pub pending: bool,
    /// Last successfully applied configuration
    pub last_applied: Option<T>,
}

impl<T: Default> Default for FeatureState<T> {
    fn default() -> Self {
        Self {
            enabled: false,
            config: T::default(),
            pending: false,
            last_applied: None,
        }
    }
}

impl<T: Clone> FeatureState<T> {
    /// Create new feature state with given config
    pub fn new(config: T) -> Self {
        Self {
            enabled: false,
            config,
            pending: false,
            last_applied: None,
        }
    }

    /// Enable the feature
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable the feature
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Set pending state
    pub fn set_pending(&mut self, pending: bool) {
        self.pending = pending;
    }

    /// Mark configuration as successfully applied
    pub fn mark_applied(&mut self) {
        self.last_applied = Some(self.config.clone());
        self.pending = false;
    }

    /// Check if current config differs from last applied
    pub fn has_pending_changes(&self) -> bool
    where
        T: PartialEq,
    {
        match &self.last_applied {
            Some(last) => &self.config != last,
            None => self.enabled,
        }
    }
}

/// Consolidated feature states for interface components
#[derive(Debug, Clone, Default)]
pub struct InterfaceFeatureStates {
    pub loss: FeatureState<TcLossConfig>,
    pub delay: FeatureState<TcDelayConfig>,
    pub duplicate: FeatureState<TcDuplicateConfig>,
    pub reorder: FeatureState<TcReorderConfig>,
    pub corrupt: FeatureState<TcCorruptConfig>,
    pub rate_limit: FeatureState<TcRateLimitConfig>,
}

impl InterfaceFeatureStates {
    /// Create new feature states with sensible default configurations
    pub fn new() -> Self {
        Self {
            loss: FeatureState::new(TcLossConfig::default()),
            delay: FeatureState::new(TcDelayConfig::default()),
            duplicate: FeatureState::new(TcDuplicateConfig::default()),
            reorder: FeatureState::new(TcReorderConfig {
                enabled: false,
                percentage: 0.0,
                correlation: 0.0,
                gap: 5, // Sensible default gap value (not 0)
            }),
            corrupt: FeatureState::new(TcCorruptConfig::default()),
            rate_limit: FeatureState::new(TcRateLimitConfig {
                enabled: false,
                rate_kbps: 1000, // Sensible default rate (not 0)
            }),
        }
    }

    /// Convert to TcNetemConfig
    pub fn to_config(&self) -> TcNetemConfig {
        let mut config = TcNetemConfig {
            loss: self.loss.config.clone(),
            delay: self.delay.config.clone(),
            duplicate: self.duplicate.config.clone(),
            reorder: self.reorder.config.clone(),
            corrupt: self.corrupt.config.clone(),
            rate_limit: self.rate_limit.config.clone(),
        };

        // Set enabled flags based on FeatureState enabled status
        config.loss.enabled = self.loss.enabled;
        config.delay.enabled = self.delay.enabled;
        config.duplicate.enabled = self.duplicate.enabled;
        config.reorder.enabled = self.reorder.enabled;
        config.corrupt.enabled = self.corrupt.enabled;
        config.rate_limit.enabled = self.rate_limit.enabled;

        config
    }

    /// Check if any feature is enabled
    pub fn has_any_enabled(&self) -> bool {
        self.loss.enabled
            || self.delay.enabled
            || self.duplicate.enabled
            || self.reorder.enabled
            || self.corrupt.enabled
            || self.rate_limit.enabled
    }

    /// Check if any feature has pending changes
    pub fn has_any_pending_changes(&self) -> bool {
        self.loss.has_pending_changes()
            || self.delay.has_pending_changes()
            || self.duplicate.has_pending_changes()
            || self.reorder.has_pending_changes()
            || self.corrupt.has_pending_changes()
            || self.rate_limit.has_pending_changes()
    }

    /// Mark all features as applied
    pub fn mark_all_applied(&mut self) {
        self.loss.mark_applied();
        self.delay.mark_applied();
        self.duplicate.mark_applied();
        self.reorder.mark_applied();
        self.corrupt.mark_applied();
        self.rate_limit.mark_applied();
    }
}

impl TcNetemConfig {
    /// Create a new config with sensible defaults and validation
    pub fn new() -> Self {
        Self {
            loss: TcLossConfig::default(),
            delay: TcDelayConfig::default(),
            duplicate: TcDuplicateConfig::default(),
            reorder: TcReorderConfig {
                enabled: false,
                percentage: 0.0,
                correlation: 0.0,
                gap: 5, // Default gap value
            },
            corrupt: TcCorruptConfig::default(),
            rate_limit: TcRateLimitConfig {
                enabled: false,
                rate_kbps: 1000, // Default 1 Mbps
            },
        }
    }

    /// Check if any features are enabled
    pub fn has_any_enabled(&self) -> bool {
        self.loss.enabled
            || self.delay.enabled
            || self.duplicate.enabled
            || self.reorder.enabled
            || self.corrupt.enabled
            || self.rate_limit.enabled
    }

    /// Convert to legacy parameter format for backward compatibility
    #[allow(clippy::type_complexity)] // Acceptable for legacy compatibility
    pub fn to_legacy_params(
        &self,
    ) -> (
        f32,
        Option<f32>,
        Option<f32>,
        Option<f32>,
        Option<f32>,
        Option<f32>,
        Option<f32>,
        Option<f32>,
        Option<f32>,
        Option<u32>,
        Option<f32>,
        Option<f32>,
        Option<u32>,
    ) {
        (
            if self.loss.enabled {
                self.loss.percentage
            } else {
                0.0
            },
            if self.loss.enabled && self.loss.correlation > 0.0 {
                Some(self.loss.correlation)
            } else {
                None
            },
            if self.delay.enabled && self.delay.base_ms > 0.0 {
                Some(self.delay.base_ms)
            } else {
                None
            },
            if self.delay.enabled && self.delay.jitter_ms > 0.0 {
                Some(self.delay.jitter_ms)
            } else {
                None
            },
            if self.delay.enabled && self.delay.correlation > 0.0 {
                Some(self.delay.correlation)
            } else {
                None
            },
            if self.duplicate.enabled && self.duplicate.percentage > 0.0 {
                Some(self.duplicate.percentage)
            } else {
                None
            },
            if self.duplicate.enabled && self.duplicate.correlation > 0.0 {
                Some(self.duplicate.correlation)
            } else {
                None
            },
            if self.reorder.enabled && self.reorder.percentage > 0.0 {
                Some(self.reorder.percentage)
            } else {
                None
            },
            if self.reorder.enabled && self.reorder.correlation > 0.0 {
                Some(self.reorder.correlation)
            } else {
                None
            },
            if self.reorder.enabled && self.reorder.percentage > 0.0 {
                Some(self.reorder.gap)
            } else {
                None
            },
            if self.corrupt.enabled && self.corrupt.percentage > 0.0 {
                Some(self.corrupt.percentage)
            } else {
                None
            },
            if self.corrupt.enabled && self.corrupt.correlation > 0.0 {
                Some(self.corrupt.correlation)
            } else {
                None
            },
            if self.rate_limit.enabled {
                Some(self.rate_limit.rate_kbps)
            } else {
                None
            },
        )
    }
}

/// Traffic control operations with structured configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TcOperation {
    /// Apply comprehensive netem configuration using structured config
    ApplyConfig { config: TcNetemConfig },
    /// Apply comprehensive netem configuration (legacy - for backward compatibility)
    Apply {
        loss: f32,
        correlation: Option<f32>,
        delay_ms: Option<f32>,        // Base delay in milliseconds (0.0-5000.0)
        delay_jitter_ms: Option<f32>, // Delay jitter/variation (0.0-1000.0)
        delay_correlation: Option<f32>, // Delay correlation (0.0-100.0)
        duplicate_percent: Option<f32>, // Packet duplication percentage (0.0-100.0)
        duplicate_correlation: Option<f32>, // Duplication correlation (0.0-100.0)
        reorder_percent: Option<f32>, // Packet reordering percentage (0.0-100.0)
        reorder_correlation: Option<f32>, // Reordering correlation (0.0-100.0)
        reorder_gap: Option<u32>,     // Reordering gap parameter (1-10)
        corrupt_percent: Option<f32>, // NEW: Packet corruption percentage (0.0-100.0)
        corrupt_correlation: Option<f32>, // NEW: Corruption correlation (0.0-100.0)
        rate_limit_kbps: Option<u32>, // NEW: Rate limiting in kbps (1-1000000)
    },
    /// Remove all traffic control configuration
    Remove,
}

/// Traffic control configuration that was applied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcConfiguration {
    /// Applied packet loss percentage
    pub loss: f32,
    /// Applied correlation percentage (if any)
    pub correlation: Option<f32>,
    /// Applied delay in milliseconds (if any)
    pub delay_ms: Option<f32>,
    /// Applied delay jitter in milliseconds (if any)
    pub delay_jitter_ms: Option<f32>,
    /// Applied delay correlation (if any)
    pub delay_correlation: Option<f32>,
    /// Applied packet duplication percentage (if any)
    pub duplicate_percent: Option<f32>,
    /// Applied duplication correlation (if any)
    pub duplicate_correlation: Option<f32>,
    /// Applied packet reordering percentage (if any)
    pub reorder_percent: Option<f32>,
    /// Applied reordering correlation (if any)
    pub reorder_correlation: Option<f32>,
    /// Applied reordering gap parameter (if any)
    pub reorder_gap: Option<u32>,
    /// Applied packet corruption percentage (if any)
    pub corrupt_percent: Option<f32>,
    /// Applied corruption correlation (if any)
    pub corrupt_correlation: Option<f32>,
    /// Applied rate limiting in kbps (if any)
    pub rate_limit_kbps: Option<u32>,
    /// Full tc command that was executed
    pub command: String,
}

/// Traffic control operation response (Reply)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Detailed message about the operation result
    pub message: String,
    /// Configuration that was applied (if successful)
    pub applied_config: Option<TcConfiguration>,
    /// Error details (if failed)
    pub error_code: Option<i32>,
}

/// Interface control request (enable/disable) (Query)
/// Query Service: tcgui/{backend_name}/query/interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceControlRequest {
    /// Target network namespace
    pub namespace: String,
    /// Target interface name
    pub interface: String,
    /// Interface control operation
    pub operation: InterfaceControlOperation,
}

/// Interface control operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterfaceControlOperation {
    /// Bring interface UP
    Enable,
    /// Bring interface DOWN
    Disable,
}

/// Interface control operation response (Reply)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceControlResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Detailed message about the operation result
    pub message: String,
    /// New interface state after operation (true = up, false = down)
    pub new_state: bool,
    /// Error details (if failed)
    pub error_code: Option<i32>,
}

/// Quality of Service configuration for different message types
pub mod qos {
    use zenoh::qos::{CongestionControl, Reliability};

    /// QoS configuration tuple: (reliability, congestion_control, history_depth)
    pub type QosConfig = (Reliability, CongestionControl, Option<usize>);

    /// QoS for interface list updates - reliable, keep last 1
    pub const INTERFACE_LIST: QosConfig =
        (Reliability::Reliable, CongestionControl::Block, Some(1));

    /// QoS for bandwidth updates - best effort, no history, drop on congestion
    pub const BANDWIDTH_UPDATES: QosConfig =
        (Reliability::BestEffort, CongestionControl::Drop, None);

    /// QoS for interface events - reliable, keep last 10
    pub const INTERFACE_EVENTS: QosConfig =
        (Reliability::Reliable, CongestionControl::Block, Some(10));

    /// QoS for backend health - reliable, keep last 1
    pub const BACKEND_HEALTH: QosConfig =
        (Reliability::Reliable, CongestionControl::Block, Some(1));

    /// Query/Reply timeout in milliseconds
    pub const QUERY_TIMEOUT_MS: u64 = 5000;
}

/// Zenoh session configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZenohConfig {
    /// Session mode (Peer or Client)
    pub mode: ZenohMode,
    /// Connection endpoints (listen addresses for peers, connect addresses for clients)
    pub endpoints: Vec<String>,
    /// Additional zenoh configuration properties
    pub properties: HashMap<String, String>,
}

impl std::hash::Hash for ZenohConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.mode.hash(state);
        self.endpoints.hash(state);
        // Hash properties by sorting keys to ensure consistent ordering
        let mut sorted_props: Vec<_> = self.properties.iter().collect();
        sorted_props.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in sorted_props {
            k.hash(state);
            v.hash(state);
        }
    }
}

/// Zenoh session modes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ZenohMode {
    /// Peer mode - can connect to and be connected from other nodes
    Peer,
    /// Client mode - only connects to other nodes, cannot accept connections
    Client,
}

impl Default for ZenohConfig {
    fn default() -> Self {
        Self {
            mode: ZenohMode::Peer,
            endpoints: vec![],
            properties: HashMap::new(),
        }
    }
}

impl ZenohConfig {
    /// Create a new peer mode configuration
    pub fn new_peer() -> Self {
        Self {
            mode: ZenohMode::Peer,
            endpoints: vec![],
            properties: HashMap::new(),
        }
    }

    /// Create a new client mode configuration
    pub fn new_client() -> Self {
        Self {
            mode: ZenohMode::Client,
            endpoints: vec![],
            properties: HashMap::new(),
        }
    }

    /// Disable multicast scouting discovery
    pub fn disable_multicast_scouting(mut self) -> Self {
        self.properties.insert(
            "scouting/multicast/enabled".to_string(),
            "false".to_string(),
        );
        self
    }

    /// Enable multicast scouting discovery (enabled by default in Zenoh)
    pub fn enable_multicast_scouting(mut self) -> Self {
        self.properties
            .insert("scouting/multicast/enabled".to_string(), "true".to_string());
        self
    }

    /// Add a listen endpoint (for peer mode)
    pub fn add_listen_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoints.push(format!("listen/{}", endpoint));
        self
    }

    /// Add a connect endpoint (for both peer and client modes)
    pub fn add_connect_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoints.push(format!("connect/{}", endpoint));
        self
    }

    /// Add a custom property
    pub fn add_property(mut self, key: &str, value: &str) -> Self {
        self.properties.insert(key.to_string(), value.to_string());
        self
    }

    /// Validate the zenoh configuration comprehensively
    pub fn validate(&self) -> Result<(), errors::ZenohConfigError> {
        use errors::ZenohConfigError;

        // Validate mode-endpoint compatibility
        if matches!(self.mode, ZenohMode::Client) {
            let has_listen = self.endpoints.iter().any(|e| e.starts_with("listen/"));
            if has_listen {
                return Err(ZenohConfigError::client_cannot_listen());
            }
        }

        // Check for duplicate endpoints
        let mut seen_endpoints = std::collections::HashSet::new();
        for endpoint in &self.endpoints {
            if !seen_endpoints.insert(endpoint) {
                return Err(ZenohConfigError::ValidationError {
                    message: format!("Duplicate endpoint found: '{}'.", endpoint),
                });
            }
        }

        // Validate endpoint formats and addresses
        for endpoint in &self.endpoints {
            self.validate_endpoint(endpoint)?;
        }

        // Validate properties if any constraints exist
        self.validate_properties()?;

        Ok(())
    }

    /// Validate a single endpoint format with comprehensive IP and port validation
    fn validate_endpoint(&self, endpoint: &str) -> Result<(), errors::ZenohConfigError> {
        use errors::ZenohConfigError;

        if let Some(addr) = endpoint.strip_prefix("tcp/") {
            self.validate_socket_address(addr, "tcp")?;
        } else if let Some(addr) = endpoint.strip_prefix("udp/") {
            self.validate_socket_address(addr, "udp")?;
        } else if let Some(addr) = endpoint.strip_prefix("tls/") {
            self.validate_socket_address(addr, "tls")?;
        } else if let Some(addr) = endpoint.strip_prefix("quic/") {
            self.validate_socket_address(addr, "quic")?;
        } else if endpoint.starts_with("connect/") || endpoint.starts_with("listen/") {
            // Extract protocol from connect/ or listen/ prefix
            let addr_part = if let Some(addr) = endpoint.strip_prefix("connect/") {
                addr
            } else if let Some(addr) = endpoint.strip_prefix("listen/") {
                addr
            } else {
                return Err(ZenohConfigError::unsupported_endpoint_format(endpoint));
            };

            // Validate the address part after connect/ or listen/
            if let Some(addr) = addr_part.strip_prefix("tcp/") {
                self.validate_socket_address(addr, "tcp")?;
            } else if let Some(addr) = addr_part.strip_prefix("udp/") {
                self.validate_socket_address(addr, "udp")?;
            } else if let Some(addr) = addr_part.strip_prefix("tls/") {
                self.validate_socket_address(addr, "tls")?;
            } else if let Some(addr) = addr_part.strip_prefix("quic/") {
                self.validate_socket_address(addr, "quic")?;
            } else {
                // Find the protocol part to give a specific error
                let protocol = addr_part.split('/').next().unwrap_or("unknown");
                return Err(ZenohConfigError::InvalidProtocol {
                    protocol: protocol.to_string(),
                    endpoint: endpoint.to_string(),
                });
            }
        } else {
            return Err(ZenohConfigError::unsupported_endpoint_format(endpoint));
        }

        Ok(())
    }

    /// Validate socket address with comprehensive port and IP validation
    fn validate_socket_address(
        &self,
        addr: &str,
        protocol: &str,
    ) -> Result<(), errors::ZenohConfigError> {
        use errors::ZenohConfigError;

        match addr.parse::<std::net::SocketAddr>() {
            Ok(socket_addr) => {
                // Validate port range (1-65535, port 0 is reserved)
                let port = socket_addr.port();
                if port == 0 {
                    return Err(ZenohConfigError::InvalidAddress {
                        address: addr.to_string(),
                        protocol: protocol.to_string(),
                        reason: "Port 0 is reserved and not allowed".to_string(),
                    });
                }

                // Additional protocol-specific validations
                match protocol {
                    "tcp" | "tls" => {
                        // TCP and TLS should not use multicast addresses
                        if socket_addr.ip().is_multicast() {
                            return Err(ZenohConfigError::InvalidAddress {
                                address: addr.to_string(),
                                protocol: protocol.to_string(),
                                reason:
                                    "Multicast addresses are not supported for TCP/TLS protocols"
                                        .to_string(),
                            });
                        }
                    }
                    "udp" | "quic" => {
                        // UDP and QUIC are more flexible but still validate basic constraints
                        if socket_addr.ip().is_unspecified() && port < 1024 {
                            return Err(ZenohConfigError::InvalidAddress {
                                address: addr.to_string(),
                                protocol: protocol.to_string(),
                                reason: "Well-known ports (< 1024) with unspecified address may require special privileges".to_string(),
                            });
                        }
                    }
                    _ => {} // Unknown protocols pass through
                }

                Ok(())
            }
            Err(e) => Err(ZenohConfigError::invalid_address_from_parse_error(
                addr, protocol, &e,
            )),
        }
    }

    /// Validate configuration properties for known constraints
    fn validate_properties(&self) -> Result<(), errors::ZenohConfigError> {
        use errors::ZenohConfigError;

        for (key, value) in &self.properties {
            match key.as_str() {
                "connect/timeout_ms" | "queries/timeout_ms" => {
                    // Validate timeout values are reasonable (1ms to 5 minutes)
                    if let Ok(timeout) = value.parse::<u64>() {
                        if timeout == 0 {
                            return Err(ZenohConfigError::PropertyError {
                                key: key.clone(),
                                value: value.clone(),
                                reason: "Timeout cannot be 0ms".to_string(),
                            });
                        }
                        if timeout > 300_000 {
                            return Err(ZenohConfigError::PropertyError {
                                key: key.clone(),
                                value: value.clone(),
                                reason: "Timeout should not exceed 5 minutes (300000ms)"
                                    .to_string(),
                            });
                        }
                    } else {
                        return Err(ZenohConfigError::PropertyError {
                            key: key.clone(),
                            value: value.clone(),
                            reason: "Timeout value must be a valid number".to_string(),
                        });
                    }
                }
                "scouting/multicast/enabled" => {
                    // Validate boolean values
                    if !matches!(value.as_str(), "true" | "false") {
                        return Err(ZenohConfigError::PropertyError {
                            key: key.clone(),
                            value: value.clone(),
                            reason: "Boolean property must be 'true' or 'false'".to_string(),
                        });
                    }
                }
                // Add more property validations as needed
                _ => {} // Unknown properties are allowed
            }
        }

        Ok(())
    }

    /// Convert to zenoh::Config
    pub fn to_zenoh_config(&self) -> Result<zenoh::Config, errors::ZenohConfigError> {
        use errors::ZenohConfigError;

        // Validate configuration first
        self.validate()?;

        let mut config = zenoh::Config::default();

        // Set mode
        match self.mode {
            ZenohMode::Peer => {
                config.set_mode(Some(WhatAmI::Peer)).map_err(|e| {
                    ZenohConfigError::ZenohConfigCreationError {
                        reason: format!("Failed to set peer mode: {:?}", e),
                    }
                })?;
            }
            ZenohMode::Client => {
                config.set_mode(Some(WhatAmI::Client)).map_err(|e| {
                    ZenohConfigError::ZenohConfigCreationError {
                        reason: format!("Failed to set client mode: {:?}", e),
                    }
                })?;
            }
        }

        // Set endpoints
        if !self.endpoints.is_empty() {
            let mut connect_endpoints = Vec::new();
            let mut listen_endpoints = Vec::new();

            for endpoint in &self.endpoints {
                if let Some(addr) = endpoint.strip_prefix("connect/") {
                    connect_endpoints.push(addr);
                } else if let Some(addr) = endpoint.strip_prefix("listen/") {
                    listen_endpoints.push(addr);
                }
            }

            if !connect_endpoints.is_empty() {
                let endpoints_json = connect_endpoints
                    .iter()
                    .map(|e| format!("\"{}\"", e))
                    .collect::<Vec<_>>()
                    .join(",");
                config
                    .insert_json5("connect/endpoints", &format!("[{}]", endpoints_json))
                    .map_err(|e| ZenohConfigError::ZenohConfigCreationError {
                        reason: format!("Failed to set connect endpoints: {}", e),
                    })?;
            }

            if !listen_endpoints.is_empty() {
                let endpoints_json = listen_endpoints
                    .iter()
                    .map(|e| format!("\"{}\"", e))
                    .collect::<Vec<_>>()
                    .join(",");
                config
                    .insert_json5("listen/endpoints", &format!("[{}]", endpoints_json))
                    .map_err(|e| ZenohConfigError::ZenohConfigCreationError {
                        reason: format!("Failed to set listen endpoints: {}", e),
                    })?;
            }
        }

        // Set additional properties
        for (key, value) in &self.properties {
            // Don't quote boolean values or numbers, only strings
            let json_value = if value == "true" || value == "false" {
                value.to_string() // Boolean without quotes
            } else if value.parse::<f64>().is_ok() {
                value.to_string() // Number without quotes
            } else {
                format!("\"{}\"", value) // String with quotes
            };

            config
                .insert_json5(key, &json_value)
                .map_err(|e| ZenohConfigError::PropertyError {
                    key: key.clone(),
                    value: value.clone(),
                    reason: e.to_string(),
                })?;
        }

        Ok(config)
    }
}

/// Backend instance information with contained namespaces.
///
/// Represents a single backend instance (uniquely identified by name) and all
/// the network namespaces it manages. Used for organizing multi-backend display
/// in the GUI with a hierarchical Backend -> Namespace -> Interface structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    /// Unique backend identifier (e.g., "server1", "edge-node-01")
    pub name: String,
    /// Backend connection/health status
    pub is_connected: bool,
    /// When this backend was last seen (for connection timeout detection)
    pub last_seen: u64,
    /// List of network namespaces managed by this backend
    pub namespaces: Vec<NetworkNamespace>,
    /// Backend-specific metadata (version, capabilities, etc.)
    pub metadata: BackendMetadata,
}

/// Backend metadata and capabilities information.
///
/// Contains backend-specific information that may be useful for display
/// or operational decisions in the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendMetadata {
    /// Backend software version (if available)
    pub version: Option<String>,
    /// Hostname or system identifier where backend is running
    pub hostname: Option<String>,
    /// Backend startup timestamp
    pub started_at: Option<u64>,
    /// List of supported features or capabilities
    pub capabilities: Vec<String>,
}

impl Default for BackendMetadata {
    fn default() -> Self {
        Self {
            version: None,
            hostname: None,
            started_at: None,
            capabilities: vec!["tc_netem".to_string(), "interface_control".to_string()],
        }
    }
}

/// Network namespace information with contained interfaces.
///
/// Represents a Linux network namespace and all the network interfaces
/// it contains. Used for organizing interface display in the GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNamespace {
    /// Namespace name ("default" for host namespace, or custom name)
    pub name: String,
    /// Optional namespace ID for system identification
    pub id: Option<u32>,
    /// Whether the namespace is currently active and accessible
    pub is_active: bool,
    /// List of network interfaces within this namespace
    pub interfaces: Vec<NetworkInterface>,
}

/// Network interface information with namespace context.
///
/// Represents a network interface within a specific namespace, including
/// its current state and traffic control configuration status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkInterface {
    /// Interface name (e.g., "eth0", "fo", "wlan0")
    pub name: String,
    /// System interface index (unique within namespace)
    pub index: u32,
    /// Parent network namespace name
    pub namespace: String,
    /// Whether the interface is currently UP (enabled)
    pub is_up: bool,
    /// Whether traffic control qdisc is configured on this interface
    pub has_tc_qdisc: bool,
    /// Type classification of the network interface
    pub interface_type: InterfaceType,
}

/// Classification of network interface types.
///
/// Used to categorize interfaces for display and operational purposes
/// in the GUI. Different interface types may have different capabilities
/// or restrictions for traffic control operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InterfaceType {
    /// Physical hardware network interface (e.g., Ethernet, WiFi)
    Physical,
    /// Virtual interface created by software
    Virtual,
    /// Virtual Ethernet pair interface (veth)
    Veth,
    /// Network bridge interface
    Bridge,
    /// TUN (Layer 3) tunnel interface
    Tun,
    /// TAP (Layer 2) tunnel interface
    Tap,
    /// Loopback interface (typically "lo")
    Loopback,
}

/// Comprehensive network bandwidth statistics and rates.
///
/// Contains both cumulative counters (total bytes/packets since interface creation)
/// and calculated rates (bytes per second) for real-time monitoring. Statistics
/// are collected from `/proc/net/dev` with additional rate calculations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkBandwidthStats {
    /// Total bytes received since interface creation
    pub rx_bytes: u64,
    /// Total packets received since interface creation
    pub rx_packets: u64,
    /// Total receive errors (checksum, frame, etc.)
    pub rx_errors: u64,
    /// Total received packets dropped (buffer full, etc.)
    pub rx_dropped: u64,
    /// Total bytes transmitted since interface creation
    pub tx_bytes: u64,
    /// Total packets transmitted since interface creation
    pub tx_packets: u64,
    /// Total transmit errors (collision, carrier, etc.)
    pub tx_errors: u64,
    /// Total transmitted packets dropped
    pub tx_dropped: u64,
    /// Unix timestamp when these statistics were collected
    pub timestamp: u64,
    /// Current receive rate in bytes per second (calculated from deltas)
    pub rx_bytes_per_sec: f64,
    /// Current transmit rate in bytes per second (calculated from deltas)
    pub tx_bytes_per_sec: f64,
}

/// Type of network interface state change event.
///
/// Used to categorize different types of interface updates sent from
/// the backend to frontend for real-time interface monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterfaceEventType {
    /// New network interface was detected
    Added,
    /// Network interface was removed/deleted
    Removed,
    /// Interface state changed (UP/DOWN, IP address, etc.)
    StateChanged,
    /// Traffic control qdisc was added to interface
    QdiscAdded,
    /// Traffic control qdisc was removed from interface
    QdiscRemoved,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tc_loss_config_validation() {
        let mut config = TcLossConfig {
            enabled: true,
            percentage: 150.0, // Invalid
            correlation: 50.0,
        };

        assert!(config.validate().is_err());

        config.percentage = 50.0; // Valid
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tc_netem_config_validation() {
        let mut config = TcNetemConfig::new();

        // Valid configuration
        config.loss.enabled = true;
        config.loss.percentage = 10.0;
        assert!(config.validate().is_ok());

        // Invalid configuration
        config.delay.enabled = true;
        config.delay.base_ms = -100.0; // Invalid negative delay
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_feature_state_pending_changes() {
        let mut feature_state = FeatureState::new(TcLossConfig {
            enabled: true,
            percentage: 10.0,
            correlation: 5.0,
        });

        // Should have pending changes when enabled but not applied
        feature_state.enable();
        assert!(feature_state.has_pending_changes());

        // Should not have pending changes after marking as applied
        feature_state.mark_applied();
        assert!(!feature_state.has_pending_changes());

        // Should have pending changes when config is modified
        feature_state.config.percentage = 20.0;
        assert!(feature_state.has_pending_changes());
    }

    #[test]
    fn test_interface_feature_states_conversion() {
        let mut states = InterfaceFeatureStates::new();

        // Enable loss with configuration
        states.loss.enable();
        states.loss.config.percentage = 15.0;
        states.loss.config.correlation = 10.0;

        // Enable delay with configuration
        states.delay.enable();
        states.delay.config.base_ms = 100.0;
        states.delay.config.jitter_ms = 10.0;

        let config = states.to_config();

        assert!(config.loss.enabled);
        assert_eq!(config.loss.percentage, 15.0);
        assert_eq!(config.loss.correlation, 10.0);

        assert!(config.delay.enabled);
        assert_eq!(config.delay.base_ms, 100.0);
        assert_eq!(config.delay.jitter_ms, 10.0);

        // Disabled features should have default config
        assert!(!config.duplicate.enabled);
        assert_eq!(config.duplicate.percentage, 0.0);
    }

    #[test]
    fn test_zenoh_config_validation_duplicate_endpoints() {
        let config = ZenohConfig::new_peer()
            .add_listen_endpoint("tcp/127.0.0.1:7447")
            .add_listen_endpoint("tcp/127.0.0.1:7447"); // Duplicate

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Duplicate endpoint"));
        }
    }

    #[test]
    fn test_zenoh_config_validation_port_zero() {
        let config = ZenohConfig::new_peer().add_listen_endpoint("tcp/127.0.0.1:0");

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Port 0 is reserved"));
        }
    }

    #[test]
    fn test_zenoh_config_validation_multicast_tcp() {
        let config = ZenohConfig::new_peer().add_listen_endpoint("tcp/224.0.0.1:7447"); // Multicast address

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e
                .to_string()
                .contains("Multicast addresses are not supported for TCP"));
        }
    }

    #[test]
    fn test_zenoh_config_validation_client_listen() {
        let config = ZenohConfig::new_client().add_listen_endpoint("tcp/127.0.0.1:7447"); // Client with listen

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("cannot have listen endpoints"));
        }
    }

    #[test]
    fn test_zenoh_config_validation_timeout_property() {
        let mut config = ZenohConfig::new_peer();
        config
            .properties
            .insert("connect/timeout_ms".to_string(), "0".to_string());

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Timeout cannot be 0ms"));
        }
    }

    #[test]
    fn test_zenoh_config_validation_boolean_property() {
        let mut config = ZenohConfig::new_peer();
        config.properties.insert(
            "scouting/multicast/enabled".to_string(),
            "maybe".to_string(),
        );

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e
                .to_string()
                .contains("Boolean property must be 'true' or 'false'"));
        }
    }

    #[test]
    fn test_zenoh_config_validation_valid_config() {
        let config = ZenohConfig::new_peer()
            .add_listen_endpoint("tcp/127.0.0.1:7447")
            .add_connect_endpoint("tcp/192.168.1.100:7448")
            .add_property("connect/timeout_ms", "5000");

        let result = config.validate();
        assert!(
            result.is_ok(),
            "Valid config should pass validation: {:?}",
            result
        );
    }

    #[test]
    fn test_zenoh_config_validation_ipv6() {
        let config = ZenohConfig::new_peer().add_listen_endpoint("tcp/[::1]:7447"); // IPv6 localhost

        let result = config.validate();
        assert!(
            result.is_ok(),
            "IPv6 addresses should be valid: {:?}",
            result
        );
    }

    #[test]
    fn test_zenoh_config_validation_udp_privileges_warning() {
        let config = ZenohConfig::new_peer().add_listen_endpoint("udp/0.0.0.0:80"); // Well-known port with unspecified address

        let result = config.validate();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("may require special privileges"));
        }
    }
}
