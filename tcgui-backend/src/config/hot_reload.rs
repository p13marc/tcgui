//! Configuration hot-reloading system for dynamic config updates.
//!
//! This module provides the ability to reload configuration changes at runtime
//! without restarting the backend service. It supports watching configuration
//! files, environment variables, and responding to reload signals.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::sync::broadcast;
use tokio::time::interval;
use tracing::{debug, error, info};

use super::{AppConfig, FeatureFlags, FeatureToggleManager};

/// Configuration reload event types
#[derive(Debug, Clone)]
pub enum ConfigReloadEvent {
    /// Feature flags were reloaded
    FeatureFlagsReloaded(FeatureFlags),
    /// Application config was reloaded
    AppConfigReloaded(AppConfig),
    /// Environment variables changed
    EnvironmentChanged,
    /// Configuration file was modified
    FileChanged(PathBuf),
    /// Manual reload was triggered
    ManualReload,
}

/// Configuration source types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigSource {
    /// Configuration from JSON file
    File(PathBuf),
    /// Configuration from environment variables
    Environment,
    /// Configuration from Zenoh key-value store
    Zenoh { key_prefix: String },
    /// Configuration from HTTP endpoint
    Http { endpoint: String },
}

/// Hot-reloadable configuration container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotReloadableConfig {
    /// Feature flags that can be hot-reloaded
    pub features: Option<FeatureFlags>,
    /// Application configuration overrides
    pub app_overrides: Option<AppConfigOverrides>,
    /// Last modification timestamp
    pub last_modified: Option<SystemTime>,
    /// Configuration source
    pub source: ConfigSource,
}

/// Application configuration overrides for hot-reloading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigOverrides {
    /// Override logging level
    pub log_level: Option<String>,
    /// Override monitoring intervals
    pub interface_monitor_interval_secs: Option<u64>,
    pub bandwidth_monitor_interval_secs: Option<u64>,
    /// Override feature-specific settings
    pub exclude_loopback: Option<bool>,
}

/// Configuration hot-reload manager
pub struct ConfigHotReloadManager {
    /// Current reloadable configuration
    config: Arc<RwLock<HotReloadableConfig>>,
    /// Feature toggle manager reference
    feature_manager: Arc<FeatureToggleManager>,
    /// Reload event broadcaster
    event_tx: broadcast::Sender<ConfigReloadEvent>,
    /// Configuration sources to watch
    sources: Vec<ConfigSource>,
    /// Reload interval
    reload_interval: Duration,
    /// Whether hot-reloading is enabled
    enabled: bool,
}

impl ConfigHotReloadManager {
    /// Create a new hot-reload manager
    pub fn new(
        feature_manager: Arc<FeatureToggleManager>,
        sources: Vec<ConfigSource>,
        reload_interval: Duration,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(100);

        let default_config = HotReloadableConfig {
            features: None,
            app_overrides: None,
            last_modified: None,
            source: sources
                .first()
                .cloned()
                .unwrap_or(ConfigSource::Environment),
        };

        Self {
            config: Arc::new(RwLock::new(default_config)),
            feature_manager,
            event_tx,
            sources,
            reload_interval,
            enabled: true,
        }
    }

    /// Start the hot-reload monitoring task
    pub async fn start(&self) -> Result<()> {
        if !self.enabled {
            info!("Configuration hot-reloading is disabled");
            return Ok(());
        }

        info!(
            "Starting configuration hot-reload monitoring with interval: {:?}",
            self.reload_interval
        );

        let config = self.config.clone();
        let feature_manager = self.feature_manager.clone();
        let event_tx = self.event_tx.clone();
        let sources = self.sources.clone();
        let reload_interval = self.reload_interval;

        tokio::spawn(async move {
            let mut interval = interval(reload_interval);

            loop {
                interval.tick().await;

                for source in &sources {
                    if let Err(e) =
                        Self::check_and_reload_source(source, &config, &feature_manager, &event_tx)
                            .await
                    {
                        error!("Failed to reload config from source {:?}: {}", source, e);
                    }
                }
            }
        });

        Ok(())
    }

    /// Subscribe to configuration reload events
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<ConfigReloadEvent> {
        self.event_tx.subscribe()
    }

    /// Manually trigger a configuration reload
    pub async fn trigger_reload(&self) -> Result<()> {
        info!("Manual configuration reload triggered");

        for source in &self.sources {
            Self::check_and_reload_source(
                source,
                &self.config,
                &self.feature_manager,
                &self.event_tx,
            )
            .await?;
        }

        let _ = self.event_tx.send(ConfigReloadEvent::ManualReload);
        Ok(())
    }

    /// Get current hot-reloadable configuration
    pub fn get_current_config(&self) -> HotReloadableConfig {
        self.config.read().unwrap().clone()
    }

    /// Enable or disable hot-reloading
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            info!("Configuration hot-reloading enabled");
        } else {
            info!("Configuration hot-reloading disabled");
        }
    }

    /// Check and reload configuration from a specific source
    async fn check_and_reload_source(
        source: &ConfigSource,
        config: &Arc<RwLock<HotReloadableConfig>>,
        feature_manager: &FeatureToggleManager,
        event_tx: &broadcast::Sender<ConfigReloadEvent>,
    ) -> Result<()> {
        match source {
            ConfigSource::File(path) => {
                Self::reload_from_file(path, config, feature_manager, event_tx).await
            }
            ConfigSource::Environment => {
                Self::reload_from_environment(config, feature_manager, event_tx).await
            }
            ConfigSource::Zenoh { key_prefix } => {
                Self::reload_from_zenoh(key_prefix, config, feature_manager, event_tx).await
            }
            ConfigSource::Http { endpoint } => {
                Self::reload_from_http(endpoint, config, feature_manager, event_tx).await
            }
        }
    }

    /// Reload configuration from file
    async fn reload_from_file(
        path: &Path,
        config: &Arc<RwLock<HotReloadableConfig>>,
        feature_manager: &FeatureToggleManager,
        event_tx: &broadcast::Sender<ConfigReloadEvent>,
    ) -> Result<()> {
        // Check if file exists and get modification time
        let metadata = match fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(_) => {
                debug!("Config file {} does not exist, skipping", path.display());
                return Ok(());
            }
        };

        let last_modified = metadata.modified().ok();

        // Check if file was modified since last reload
        {
            let current_config = config.read().unwrap();
            if let Some(current_modified) = current_config.last_modified {
                if let Some(file_modified) = last_modified {
                    if file_modified <= current_modified {
                        return Ok(()); // No changes
                    }
                }
            }
        }

        // Read and parse the configuration file
        let content = fs::read_to_string(path).await?;
        let new_config: HotReloadableConfig = serde_json::from_str(&content)?;

        // Apply the new configuration
        Self::apply_reloaded_config(&new_config, config, feature_manager, event_tx).await?;

        info!("Reloaded configuration from file: {}", path.display());
        let _ = event_tx.send(ConfigReloadEvent::FileChanged(path.to_path_buf()));

        Ok(())
    }

    /// Reload configuration from environment variables
    async fn reload_from_environment(
        config: &Arc<RwLock<HotReloadableConfig>>,
        feature_manager: &FeatureToggleManager,
        event_tx: &broadcast::Sender<ConfigReloadEvent>,
    ) -> Result<()> {
        // Check for specific environment variables that might have changed
        let mut env_config = HotReloadableConfig {
            features: None,
            app_overrides: None,
            last_modified: Some(SystemTime::now()),
            source: ConfigSource::Environment,
        };

        // Check for feature flag overrides
        if let Ok(features_json) = std::env::var("TCGUI_FEATURE_FLAGS") {
            if let Ok(features) = serde_json::from_str::<FeatureFlags>(&features_json) {
                env_config.features = Some(features);
            }
        }

        // Check for app configuration overrides
        let app_overrides = AppConfigOverrides {
            log_level: std::env::var("TCGUI_LOG_LEVEL").ok(),
            interface_monitor_interval_secs: std::env::var("TCGUI_INTERFACE_MONITOR_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok()),
            bandwidth_monitor_interval_secs: std::env::var("TCGUI_BANDWIDTH_MONITOR_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok()),
            exclude_loopback: std::env::var("TCGUI_EXCLUDE_LOOPBACK")
                .ok()
                .and_then(|s| s.parse().ok()),
        };

        // Only include overrides if there are actual values
        if app_overrides.log_level.is_some()
            || app_overrides.interface_monitor_interval_secs.is_some()
            || app_overrides.bandwidth_monitor_interval_secs.is_some()
            || app_overrides.exclude_loopback.is_some()
        {
            env_config.app_overrides = Some(app_overrides);
        }

        // Only apply if there are actual environment overrides
        if env_config.features.is_some() || env_config.app_overrides.is_some() {
            Self::apply_reloaded_config(&env_config, config, feature_manager, event_tx).await?;
            debug!("Reloaded configuration from environment variables");
            let _ = event_tx.send(ConfigReloadEvent::EnvironmentChanged);
        }

        Ok(())
    }

    /// Reload configuration from Zenoh key-value store
    async fn reload_from_zenoh(
        _key_prefix: &str,
        _config: &Arc<RwLock<HotReloadableConfig>>,
        _feature_manager: &FeatureToggleManager,
        _event_tx: &broadcast::Sender<ConfigReloadEvent>,
    ) -> Result<()> {
        // TODO: Implement Zenoh-based configuration reloading
        // This would query Zenoh storage for configuration updates
        debug!("Zenoh configuration reloading not yet implemented");
        Ok(())
    }

    /// Reload configuration from HTTP endpoint
    async fn reload_from_http(
        _endpoint: &str,
        _config: &Arc<RwLock<HotReloadableConfig>>,
        _feature_manager: &FeatureToggleManager,
        _event_tx: &broadcast::Sender<ConfigReloadEvent>,
    ) -> Result<()> {
        // TODO: Implement HTTP-based configuration reloading
        // This would make HTTP requests to fetch configuration updates
        debug!("HTTP configuration reloading not yet implemented");
        Ok(())
    }

    /// Apply a reloaded configuration
    async fn apply_reloaded_config(
        new_config: &HotReloadableConfig,
        config: &Arc<RwLock<HotReloadableConfig>>,
        feature_manager: &FeatureToggleManager,
        event_tx: &broadcast::Sender<ConfigReloadEvent>,
    ) -> Result<()> {
        // Update stored configuration
        {
            let mut current_config = config.write().unwrap();
            *current_config = new_config.clone();
        }

        // Apply feature flag changes
        if let Some(ref new_features) = new_config.features {
            // Get current feature flags
            let current_features = feature_manager.get_all_flags();

            // Apply changes for each feature flag
            if new_features.bandwidth_monitoring != current_features.bandwidth_monitoring {
                if new_features.bandwidth_monitoring {
                    feature_manager.enable_feature(&crate::config::Feature::BandwidthMonitoring)?
                } else {
                    feature_manager.disable_feature(&crate::config::Feature::BandwidthMonitoring)?
                }
            }

            if new_features.interface_hotplug != current_features.interface_hotplug {
                if new_features.interface_hotplug {
                    feature_manager.enable_feature(&crate::config::Feature::InterfaceHotplug)?
                } else {
                    feature_manager.disable_feature(&crate::config::Feature::InterfaceHotplug)?
                }
            }

            if new_features.tc_command_caching != current_features.tc_command_caching {
                if new_features.tc_command_caching {
                    feature_manager.enable_feature(&crate::config::Feature::TcCommandCaching)?
                } else {
                    feature_manager.disable_feature(&crate::config::Feature::TcCommandCaching)?
                }
            }

            if new_features.metrics_collection != current_features.metrics_collection {
                if new_features.metrics_collection {
                    feature_manager.enable_feature(&crate::config::Feature::MetricsCollection)?
                } else {
                    feature_manager.disable_feature(&crate::config::Feature::MetricsCollection)?
                }
            }

            if new_features.namespace_monitoring != current_features.namespace_monitoring {
                if new_features.namespace_monitoring {
                    feature_manager.enable_feature(&crate::config::Feature::NamespaceMonitoring)?
                } else {
                    feature_manager.disable_feature(&crate::config::Feature::NamespaceMonitoring)?
                }
            }

            if new_features.experimental_features != current_features.experimental_features {
                if new_features.experimental_features {
                    feature_manager.enable_feature(&crate::config::Feature::ExperimentalFeatures)?
                } else {
                    feature_manager
                        .disable_feature(&crate::config::Feature::ExperimentalFeatures)?
                }
            }

            // Handle custom features
            for (name, enabled) in &new_features.custom {
                let current_enabled = current_features.custom.get(name).copied().unwrap_or(false);
                if *enabled != current_enabled {
                    if *enabled {
                        feature_manager.set_custom_feature(name.clone(), true)?;
                    } else {
                        feature_manager.remove_custom_feature(name)?;
                    }
                }
            }

            info!("Applied feature flag changes from hot-reload");
            let _ = event_tx.send(ConfigReloadEvent::FeatureFlagsReloaded(
                new_features.clone(),
            ));
        }

        // TODO: Apply application configuration overrides
        if let Some(ref app_overrides) = new_config.app_overrides {
            debug!("Application config overrides: {:?}", app_overrides);
            // This would require updating the running application configuration
            // which might need additional infrastructure to propagate changes
        }

        Ok(())
    }

    /// Create a default configuration file template
    pub async fn create_config_template(path: &Path) -> Result<()> {
        let template_config = HotReloadableConfig {
            features: Some(FeatureFlags::default()),
            app_overrides: Some(AppConfigOverrides {
                log_level: Some("info".to_string()),
                interface_monitor_interval_secs: Some(5),
                bandwidth_monitor_interval_secs: Some(2),
                exclude_loopback: Some(false),
            }),
            last_modified: Some(SystemTime::now()),
            source: ConfigSource::File(path.to_path_buf()),
        };

        let content = serde_json::to_string_pretty(&template_config)?;
        fs::write(path, content).await?;

        info!("Created configuration template at: {}", path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[test]
    fn test_hot_reloadable_config_serialization() {
        let config = HotReloadableConfig {
            features: Some(FeatureFlags::default()),
            app_overrides: Some(AppConfigOverrides {
                log_level: Some("debug".to_string()),
                interface_monitor_interval_secs: Some(10),
                bandwidth_monitor_interval_secs: Some(5),
                exclude_loopback: Some(true),
            }),
            last_modified: Some(SystemTime::now()),
            source: ConfigSource::File(PathBuf::from("/tmp/test.json")),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        assert!(json.contains("features"));
        assert!(json.contains("app_overrides"));

        let deserialized: HotReloadableConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.features.is_some());
        assert!(deserialized.app_overrides.is_some());
    }

    #[tokio::test]
    async fn test_config_template_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.json");

        ConfigHotReloadManager::create_config_template(&config_path)
            .await
            .unwrap();

        assert!(config_path.exists());

        let content = fs::read_to_string(&config_path).await.unwrap();
        let config: HotReloadableConfig = serde_json::from_str(&content).unwrap();

        assert!(config.features.is_some());
        assert!(config.app_overrides.is_some());
    }

    #[tokio::test]
    async fn test_hot_reload_manager_creation() {
        let feature_manager = Arc::new(FeatureToggleManager::new());
        let sources = vec![ConfigSource::Environment];
        let reload_interval = Duration::from_secs(5);

        let manager = ConfigHotReloadManager::new(feature_manager, sources, reload_interval);

        assert_eq!(manager.reload_interval, reload_interval);
        assert!(manager.enabled);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let feature_manager = Arc::new(FeatureToggleManager::new());
        let sources = vec![ConfigSource::Environment];
        let reload_interval = Duration::from_secs(5);

        let manager = ConfigHotReloadManager::new(feature_manager, sources, reload_interval);

        let mut receiver = manager.subscribe_to_events();

        // Trigger a manual reload
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = manager.trigger_reload().await;
        });

        // Should receive the manual reload event
        let event = receiver.recv().await.unwrap();
        matches!(event, ConfigReloadEvent::ManualReload);
    }
}
