//! Zenoh configuration management for the TC GUI backend.
//!
//! This module handles Zenoh-specific configuration including endpoint parsing,
//! session mode configuration, and validation of Zenoh connectivity settings.

use anyhow::Result;
use std::collections::HashMap;
use tcgui_shared::errors::ZenohConfigError;
use tcgui_shared::{ZenohConfig, ZenohMode};

use super::cli::CliConfig;

/// Zenoh configuration manager
pub struct ZenohConfigManager;

impl ZenohConfigManager {
    /// Create Zenoh configuration from CLI config
    pub fn from_cli(cli_config: &CliConfig) -> Result<ZenohConfig> {
        // Parse zenoh mode
        let zenoh_mode = match cli_config.zenoh_mode.to_lowercase().as_str() {
            "peer" => ZenohMode::Peer,
            "client" => ZenohMode::Client,
            _ => {
                tracing::error!("Invalid zenoh mode: {}, using peer", cli_config.zenoh_mode);
                ZenohMode::Peer
            }
        };

        let mut zenoh_config = ZenohConfig {
            mode: zenoh_mode,
            endpoints: vec![],
            properties: HashMap::new(),
        };

        // Add default listen endpoint on localhost for local communication (peer mode only)
        // Backend listens on a fixed port so frontend can connect to it
        // Client mode cannot have listen endpoints
        if matches!(zenoh_config.mode, ZenohMode::Peer) {
            zenoh_config = zenoh_config.add_listen_endpoint("tcp/127.0.0.1:7447");
        }

        // Add connect endpoints if specified
        if let Some(connect_endpoints) = &cli_config.zenoh_connect {
            for endpoint in connect_endpoints.split(',') {
                zenoh_config = zenoh_config.add_connect_endpoint(endpoint.trim());
            }
        }

        // Add listen endpoints if specified
        if let Some(listen_endpoints) = &cli_config.zenoh_listen {
            for endpoint in listen_endpoints.split(',') {
                zenoh_config = zenoh_config.add_listen_endpoint(endpoint.trim());
            }
        }

        // Disable multicast scouting if requested
        if cli_config.no_multicast {
            zenoh_config = zenoh_config.disable_multicast_scouting();
        }

        Ok(zenoh_config)
    }

    /// Validate and handle zenoh configuration errors with detailed reporting
    pub fn validate_and_report(zenoh_config: &ZenohConfig) -> Result<()> {
        if let Err(e) = zenoh_config.validate() {
            let error_message = match e {
                ZenohConfigError::InvalidMode { mode } => {
                    format!("Invalid zenoh mode '{}'. Use 'peer' or 'client'.", mode)
                }
                ZenohConfigError::InvalidEndpoint { endpoint, reason } => {
                    format!("Invalid endpoint '{}' - {}", endpoint, reason)
                }
                ZenohConfigError::InvalidProtocol { protocol, endpoint } => {
                    format!(
                        "Unsupported protocol '{}' in endpoint '{}'. Supported: tcp, udp, tls, quic",
                        protocol, endpoint
                    )
                }
                ZenohConfigError::ModeEndpointMismatch { mode, reason } => {
                    format!("{:?} mode {}", mode, reason)
                }
                ZenohConfigError::InvalidAddress {
                    address,
                    protocol,
                    reason,
                } => {
                    format!("Invalid {} address '{}' - {}", protocol, address, reason)
                }
                _ => format!("Invalid zenoh configuration: {}", e),
            };

            tracing::error!("{}", error_message);
            return Err(anyhow::anyhow!(
                "Zenoh configuration validation failed: {}",
                error_message
            ));
        }

        Ok(())
    }

    /// Create a default peer configuration
    pub fn default_peer() -> ZenohConfig {
        ZenohConfig {
            mode: ZenohMode::Peer,
            endpoints: vec![],
            properties: HashMap::new(),
        }
    }

    /// Create a default client configuration with common router endpoints
    pub fn default_client() -> ZenohConfig {
        let mut config = ZenohConfig {
            mode: ZenohMode::Client,
            endpoints: vec![],
            properties: HashMap::new(),
        };

        // Add common localhost router endpoint
        config = config.add_connect_endpoint("tcp/127.0.0.1:7447");
        config
    }

    /// Create configuration with multicast discovery enabled
    pub fn with_multicast_discovery(mut config: ZenohConfig) -> ZenohConfig {
        // Enable multicast scouting (this is usually enabled by default in Zenoh)
        config
            .properties
            .insert("scouting/multicast/enabled".to_string(), "true".to_string());
        config
            .properties
            .insert("scouting/gossip/enabled".to_string(), "true".to_string());
        config
    }

    /// Create configuration optimized for local development
    pub fn for_local_development() -> ZenohConfig {
        let mut config = Self::default_peer();
        config = config.add_listen_endpoint("tcp/127.0.0.1:7447");
        config = Self::with_multicast_discovery(config);

        // Enable local development optimizations
        config
            .properties
            .insert("transport/unicast/qos".to_string(), "false".to_string());
        config
            .properties
            .insert("transport/multicast/qos".to_string(), "false".to_string());

        config
    }

    /// Create configuration optimized for production
    pub fn for_production() -> ZenohConfig {
        let mut config = Self::default_peer();

        // Enable production optimizations
        config
            .properties
            .insert("transport/unicast/qos".to_string(), "true".to_string());
        config
            .properties
            .insert("transport/multicast/qos".to_string(), "true".to_string());
        config
            .properties
            .insert("transport/compression".to_string(), "true".to_string());

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zenoh_config_from_cli_peer_mode() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "test".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        let zenoh_config = ZenohConfigManager::from_cli(&cli_config).unwrap();
        assert!(matches!(zenoh_config.mode, ZenohMode::Peer));
        // Should have default localhost listen endpoint
        assert_eq!(zenoh_config.endpoints.len(), 1);
        assert!(zenoh_config.endpoints[0].contains("127.0.0.1"));
    }

    #[test]
    fn test_zenoh_config_from_cli_client_mode() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "test".to_string(),
            zenoh_mode: "client".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        let zenoh_config = ZenohConfigManager::from_cli(&cli_config).unwrap();
        assert!(matches!(zenoh_config.mode, ZenohMode::Client));
    }

    #[test]
    fn test_zenoh_config_with_endpoints() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "test".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: Some("tcp/192.168.1.1:7447,udp/192.168.1.2:7447".to_string()),
            zenoh_listen: Some("tcp/0.0.0.0:7447".to_string()),
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        let zenoh_config = ZenohConfigManager::from_cli(&cli_config).unwrap();
        assert_eq!(zenoh_config.endpoints.len(), 4); // 1 default localhost + 2 connect + 1 listen
    }

    #[test]
    fn test_zenoh_config_invalid_mode_fallback() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "test".to_string(),
            zenoh_mode: "invalid-mode".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        let zenoh_config = ZenohConfigManager::from_cli(&cli_config).unwrap();
        assert!(matches!(zenoh_config.mode, ZenohMode::Peer)); // Should fallback to peer
    }

    #[test]
    fn test_zenoh_config_no_multicast() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "test".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: true,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        let zenoh_config = ZenohConfigManager::from_cli(&cli_config).unwrap();
        assert_eq!(
            zenoh_config.properties.get("scouting/multicast/enabled"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn test_default_configurations() {
        let peer_config = ZenohConfigManager::default_peer();
        assert!(matches!(peer_config.mode, ZenohMode::Peer));
        assert!(peer_config.endpoints.is_empty());

        let client_config = ZenohConfigManager::default_client();
        assert!(matches!(client_config.mode, ZenohMode::Client));
        assert!(!client_config.endpoints.is_empty()); // Should have localhost endpoint
    }

    #[test]
    fn test_multicast_discovery() {
        let config = ZenohConfigManager::default_peer();
        let config = ZenohConfigManager::with_multicast_discovery(config);

        assert!(config.properties.contains_key("scouting/multicast/enabled"));
        assert!(config.properties.contains_key("scouting/gossip/enabled"));
    }

    #[test]
    fn test_local_development_config() {
        let config = ZenohConfigManager::for_local_development();
        assert!(matches!(config.mode, ZenohMode::Peer));
        assert!(!config.endpoints.is_empty()); // Should have listen endpoint
        assert!(config.properties.contains_key("transport/unicast/qos"));
    }

    #[test]
    fn test_production_config() {
        let config = ZenohConfigManager::for_production();
        assert!(matches!(config.mode, ZenohMode::Peer));
        assert_eq!(
            config.properties.get("transport/compression"),
            Some(&"true".to_string())
        );
        assert_eq!(
            config.properties.get("transport/unicast/qos"),
            Some(&"true".to_string())
        );
    }

    #[test]
    fn test_validation_success() {
        let config = ZenohConfigManager::default_peer();
        assert!(ZenohConfigManager::validate_and_report(&config).is_ok());
    }

    #[test]
    fn test_validation_with_valid_endpoints() {
        let mut config = ZenohConfigManager::default_peer();
        config = config.add_listen_endpoint("tcp/127.0.0.1:7447");
        config = config.add_connect_endpoint("tcp/192.168.1.1:7447");

        assert!(ZenohConfigManager::validate_and_report(&config).is_ok());
    }
}
