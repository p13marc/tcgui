//! Configuration management module for TC GUI backend.
//!
//! This module provides centralized configuration management with support for:
//! - CLI argument parsing
//! - Environment variable configuration
//! - Configuration validation
//! - Builder pattern for configuration construction
//! - Hot reloading capabilities

pub mod app_config;
pub mod cli;
pub mod feature_flags;
pub mod hot_reload;
pub mod zenoh_config;

pub use app_config::{AppConfig, AppConfigBuilder, LogLevel};
pub use cli::CliConfig;
pub use feature_flags::{Feature, FeatureFlags, FeatureProfile, FeatureToggleManager};
pub use hot_reload::{
    ConfigHotReloadManager, ConfigReloadEvent, ConfigSource, HotReloadableConfig,
};
pub use zenoh_config::ZenohConfigManager;

use anyhow::Result;
use tcgui_shared::ZenohConfig;

/// Main configuration manager that combines all configuration sources
#[derive(Debug, Clone)]
pub struct ConfigManager {
    pub app: AppConfig,
    pub zenoh: ZenohConfig,
    pub features: FeatureToggleManager,
}

impl ConfigManager {
    /// Creates a new configuration manager from CLI arguments and environment
    pub fn from_cli_and_env() -> Result<Self> {
        let cli_config = CliConfig::from_args()?;
        let app_config = AppConfig::from_cli(&cli_config)?;
        let zenoh_config = ZenohConfigManager::from_cli(&cli_config)?;
        let feature_manager = FeatureToggleManager::from_env()?;

        Ok(Self {
            app: app_config,
            zenoh: zenoh_config,
            features: feature_manager,
        })
    }

    /// Validates the entire configuration
    pub fn validate(&self) -> Result<()> {
        self.app.validate()?;
        self.zenoh
            .validate()
            .map_err(|e| anyhow::anyhow!("Zenoh configuration error: {}", e))?;
        Ok(())
    }

    /// Initialize logging based on configuration
    pub fn init_logging(&self) -> Result<()> {
        self.app.init_logging()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_manager_validation() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "test".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
        };

        let app_config = AppConfig::from_cli(&cli_config).unwrap();
        let zenoh_config = ZenohConfigManager::from_cli(&cli_config).unwrap();
        let feature_manager = FeatureToggleManager::new();

        let config_manager = ConfigManager {
            app: app_config,
            zenoh: zenoh_config,
            features: feature_manager,
        };

        assert!(config_manager.validate().is_ok());
    }
}
