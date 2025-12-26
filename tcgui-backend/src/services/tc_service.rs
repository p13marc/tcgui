//! Traffic Control Service
//!
//! This service handles all TC (Traffic Control) operations including:
//! - Applying TC configurations to interfaces
//! - Removing TC configurations
//! - Publishing TC configuration updates
//! - Managing TC configuration state

use anyhow::Result;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::{errors::TcguiError, topics, TcConfigUpdate, TcConfiguration};

use super::ServiceHealth;
use crate::tc_commands::TcCommandManager;
use crate::utils::service_resilience::{execute_system_command, execute_zenoh_communication};

/// Traffic Control Service
pub struct TcService {
    /// TC command manager for executing system commands
    tc_manager: TcCommandManager,

    /// Zenoh session for messaging
    session: Session,

    /// Backend name for topic routing
    backend_name: String,

    /// TC configuration publishers per interface
    tc_config_publishers: HashMap<String, AdvancedPublisher<'static>>, // namespace/interface -> publisher

    /// Current TC configurations cache
    current_configs: HashMap<String, Option<TcConfiguration>>, // namespace/interface -> config

    /// Service health status
    health_status: ServiceHealth,
}

impl TcService {
    /// Create a new TC service
    pub fn new(session: Session, backend_name: String) -> Self {
        Self {
            tc_manager: TcCommandManager::new(),
            session,
            backend_name,
            tc_config_publishers: HashMap::new(),
            current_configs: HashMap::new(),
            health_status: ServiceHealth::Healthy,
        }
    }

    /// Apply TC configuration to an interface
    #[allow(clippy::too_many_arguments)] // Legacy API - will be refactored to use config struct
    #[allow(deprecated)] // Uses deprecated apply_tc_config_in_namespace internally
    #[instrument(skip(self), fields(service = "tc", namespace, interface))]
    pub async fn apply_tc_config(
        &mut self,
        namespace: &str,
        interface: &str,
        loss: f32,
        correlation: Option<f32>,
        delay_ms: Option<f32>,
        delay_jitter_ms: Option<f32>,
        delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        reorder_correlation: Option<f32>,
        reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> Result<TcConfiguration> {
        info!("Applying TC config to {}:{}", namespace, interface);

        // Check if any meaningful TC parameters are present
        let has_meaningful_params = loss > 0.0
            || delay_ms.is_some_and(|d| d > 0.0)
            || duplicate_percent.is_some_and(|d| d > 0.0)
            || reorder_percent.is_some_and(|r| r > 0.0)
            || corrupt_percent.is_some_and(|c| c > 0.0)
            || rate_limit_kbps.is_some_and(|r| r > 0);

        if !has_meaningful_params {
            // No meaningful parameters - remove TC qdisc entirely
            return self.remove_tc_config(namespace, interface).await;
        }

        // Apply TC configuration with resilience
        let tc_manager = self.tc_manager.clone();
        let namespace_str = namespace.to_string();
        let interface_str = interface.to_string();

        execute_system_command(
            move || {
                let tc_manager = tc_manager.clone();
                let namespace = namespace_str.clone();
                let interface = interface_str.clone();
                async move {
                    tc_manager
                        .apply_tc_config_in_namespace(
                            &namespace,
                            &interface,
                            loss,
                            correlation,
                            delay_ms,
                            delay_jitter_ms,
                            delay_correlation,
                            duplicate_percent,
                            duplicate_correlation,
                            reorder_percent,
                            reorder_correlation,
                            reorder_gap,
                            corrupt_percent,
                            corrupt_correlation,
                            rate_limit_kbps,
                        )
                        .await
                }
            },
            "apply_tc_config",
            "tc_service",
        )
        .await
        .map_err(|e| {
            error!("Failed to apply TC config: {}", e);
            self.health_status = ServiceHealth::Degraded {
                reason: format!("TC apply failed: {}", e),
            };
            e
        })?;

        // Build configuration object for response and caching
        let config = self.build_tc_configuration(
            interface,
            loss,
            correlation,
            delay_ms,
            delay_jitter_ms,
            delay_correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps,
        );

        // Cache the configuration
        let key = format!("{}/{}", namespace, interface);
        self.current_configs
            .insert(key.clone(), Some(config.clone()));

        // Publish TC configuration update
        if let Err(e) = self
            .publish_tc_config(namespace, interface, Some(config.clone()))
            .await
        {
            warn!("Failed to publish TC config update: {}", e);
        }

        info!(
            "Successfully applied TC config to {}:{}",
            namespace, interface
        );
        Ok(config)
    }

    /// Remove TC configuration from an interface
    #[instrument(skip(self), fields(service = "tc", namespace, interface))]
    pub async fn remove_tc_config(
        &mut self,
        namespace: &str,
        interface: &str,
    ) -> Result<TcConfiguration> {
        info!("Removing TC config from {}:{}", namespace, interface);

        // Remove TC configuration with resilience
        let tc_manager = self.tc_manager.clone();
        let namespace_str = namespace.to_string();
        let interface_str = interface.to_string();

        execute_system_command(
            move || {
                let tc_manager = tc_manager.clone();
                let namespace = namespace_str.clone();
                let interface = interface_str.clone();
                async move {
                    tc_manager
                        .remove_tc_config_in_namespace(&namespace, &interface)
                        .await
                }
            },
            "remove_tc_config",
            "tc_service",
        )
        .await
        .map_err(|e| {
            error!("Failed to remove TC config: {}", e);
            self.health_status = ServiceHealth::Degraded {
                reason: format!("TC remove failed: {}", e),
            };
            e
        })?;

        // Update cache
        let key = format!("{}/{}", namespace, interface);
        self.current_configs.insert(key, None);

        // Publish TC configuration removal
        if let Err(e) = self.publish_tc_config(namespace, interface, None).await {
            warn!("Failed to publish TC config removal: {}", e);
        }

        info!(
            "Successfully removed TC config from {}:{}",
            namespace, interface
        );

        // Return empty configuration to indicate removal
        Ok(TcConfiguration {
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
            command: format!("tc qdisc del dev {} root", interface),
        })
    }

    /// Apply structured TC configuration
    #[instrument(skip(self, config), fields(service = "tc", namespace, interface))]
    pub async fn apply_structured_config(
        &mut self,
        namespace: &str,
        interface: &str,
        config: &tcgui_shared::TcNetemConfig,
    ) -> Result<()> {
        info!(
            "Applying structured TC config to {}:{}",
            namespace, interface
        );

        // Apply structured TC configuration with resilience
        let tc_manager = self.tc_manager.clone();
        let namespace_str = namespace.to_string();
        let interface_str = interface.to_string();
        let config = config.clone();

        execute_system_command(
            move || {
                let tc_manager = tc_manager.clone();
                let namespace = namespace_str.clone();
                let interface = interface_str.clone();
                let config = config.clone();
                async move {
                    tc_manager
                        .apply_tc_config_structured(&namespace, &interface, &config)
                        .await
                }
            },
            "apply_structured_tc_config",
            "tc_service",
        )
        .await
        .map_err(|e| {
            error!("Failed to apply structured TC config: {}", e);
            self.health_status = ServiceHealth::Degraded {
                reason: format!("Structured TC apply failed: {}", e),
            };
            e
        })?;

        // Publish configuration update (simplified for structured config)
        if let Err(e) = self.publish_tc_config(namespace, interface, None).await {
            warn!("Failed to publish structured TC config update: {}", e);
        }

        info!(
            "Successfully applied structured TC config to {}:{}",
            namespace, interface
        );
        Ok(())
    }

    /// Detect current TC configuration on an interface
    #[instrument(skip(self), fields(service = "tc", namespace, interface))]
    pub async fn detect_current_config(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Option<TcConfiguration> {
        // Use tc_manager with resilience to check if there's an existing qdisc on the interface
        let tc_manager = self.tc_manager.clone();
        let namespace_str = namespace.to_string();
        let interface_str = interface.to_string();

        let qdisc_result = execute_system_command(
            move || {
                let tc_manager = tc_manager.clone();
                let namespace = namespace_str.clone();
                let interface = interface_str.clone();
                async move {
                    tc_manager
                        .check_existing_qdisc(&namespace, &interface)
                        .await
                }
            },
            "detect_current_config",
            "tc_service",
        )
        .await;

        match qdisc_result {
            Ok(qdisc_info) if !qdisc_info.is_empty() => {
                // Check if it's a netem qdisc (which is what we're interested in)
                if qdisc_info.contains("netem") {
                    info!(
                        "Detected existing netem qdisc on {}:{}: {}",
                        namespace,
                        interface,
                        qdisc_info.trim()
                    );

                    // For now, return a basic configuration indicating TC is active
                    // TODO: Parse the actual TC parameters from the qdisc output
                    Some(TcConfiguration {
                        loss: 0.0, // Will be parsed later
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
                        command: format!("# Detected: {}", qdisc_info.trim()),
                    })
                } else {
                    // Non-netem qdisc (e.g., noqueue, mq, etc.) - not a TC configuration
                    None
                }
            }
            Ok(_) => {
                // Empty qdisc info - no qdisc found
                None
            }
            Err(e) => {
                warn!(
                    "Failed to detect TC configuration on {}:{}: {}",
                    namespace, interface, e
                );
                None
            }
        }
    }

    /// Get cached TC configuration for an interface
    pub fn get_cached_config(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Option<&Option<TcConfiguration>> {
        let key = format!("{}/{}", namespace, interface);
        self.current_configs.get(&key)
    }

    /// Get or create a TC configuration publisher for a specific interface
    #[instrument(skip(self), fields(service = "tc", namespace, interface))]
    async fn get_tc_config_publisher(
        &mut self,
        namespace: &str,
        interface: &str,
    ) -> Result<&AdvancedPublisher<'static>> {
        let key = format!("{}/{}", namespace, interface);

        if !self.tc_config_publishers.contains_key(&key) {
            let tc_config_topic = topics::tc_config(&self.backend_name, namespace, interface);
            info!(
                "Creating TC config publisher for {}/{} on: {}",
                namespace,
                interface,
                tc_config_topic.as_str()
            );

            let session = self.session.clone();
            let topic = tc_config_topic.clone();

            let publisher = execute_zenoh_communication(
                move || {
                    let session = session.clone();
                    let topic = topic.clone();
                    async move {
                        session
                            .declare_publisher(topic)
                            .cache(CacheConfig::default().max_samples(1))
                            .sample_miss_detection(
                                MissDetectionConfig::default()
                                    .heartbeat(Duration::from_millis(1000)),
                            )
                            .publisher_detection()
                            .await
                            .map_err(|e| {
                                anyhow::Error::from(TcguiError::ZenohError {
                                    message: format!(
                                        "Failed to declare TC config publisher: {}",
                                        e
                                    ),
                                })
                            })
                    }
                },
                "get_tc_config_publisher",
                "tc_service",
            )
            .await?;

            self.tc_config_publishers.insert(key.clone(), publisher);
        }

        Ok(self.tc_config_publishers.get(&key).unwrap())
    }

    /// Publish current TC configuration for an interface
    #[instrument(
        skip(self, configuration),
        fields(service = "tc", namespace, interface)
    )]
    pub async fn publish_tc_config(
        &mut self,
        namespace: &str,
        interface: &str,
        configuration: Option<TcConfiguration>,
    ) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let backend_name = self.backend_name.clone();
        let publisher = self.get_tc_config_publisher(namespace, interface).await?;

        let tc_update = TcConfigUpdate {
            namespace: namespace.to_string(),
            interface: interface.to_string(),
            backend_name,
            timestamp,
            configuration: configuration.clone(),
            has_tc: configuration.is_some(),
        };

        let payload = serde_json::to_string(&tc_update)?;

        // Use resilient Zenoh communication for publishing
        let publisher_clone = publisher;
        execute_zenoh_communication(
            move || {
                let publisher = publisher_clone;
                let payload = payload.clone();
                async move {
                    publisher.put(payload).await.map_err(|e| {
                        anyhow::Error::from(TcguiError::ZenohError {
                            message: format!("Failed to publish TC config update: {}", e),
                        })
                    })
                }
            },
            "publish_tc_config",
            "tc_service",
        )
        .await?;

        info!(
            "Published TC config update for {}/{}: has_tc={}",
            namespace, interface, tc_update.has_tc
        );
        Ok(())
    }

    /// Build TC configuration object for response
    #[allow(clippy::too_many_arguments)] // Legacy API - will be refactored to use config struct
    fn build_tc_configuration(
        &self,
        interface: &str,
        loss: f32,
        correlation: Option<f32>,
        delay_ms: Option<f32>,
        delay_jitter_ms: Option<f32>,
        delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        reorder_correlation: Option<f32>,
        reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> TcConfiguration {
        // Build command string for display
        let mut cmd_parts = vec![format!("tc qdisc replace dev {} root netem", interface)];

        if loss > 0.0 {
            let loss_part = if let Some(corr) = correlation {
                if corr > 0.0 {
                    format!("loss {}% correlation {}%", loss, corr)
                } else {
                    format!("loss {}%", loss)
                }
            } else {
                format!("loss {}%", loss)
            };
            cmd_parts.push(loss_part);
        }

        if let Some(delay) = delay_ms
            && delay > 0.0 {
                let mut delay_part = format!("delay {}ms", delay);
                if let Some(jitter) = delay_jitter_ms
                    && jitter > 0.0 {
                        delay_part.push_str(&format!(" {}ms", jitter));
                        if let Some(delay_corr) = delay_correlation
                            && delay_corr > 0.0 {
                                delay_part.push_str(&format!(" {}%", delay_corr));
                            }
                    }
                cmd_parts.push(delay_part);
            }

        if let Some(duplicate) = duplicate_percent
            && duplicate > 0.0 {
                let mut duplicate_part = format!("duplicate {}%", duplicate);
                if let Some(dup_corr) = duplicate_correlation
                    && dup_corr > 0.0 {
                        duplicate_part.push_str(&format!(" {}%", dup_corr));
                    }
                cmd_parts.push(duplicate_part);
            }

        if let Some(reorder) = reorder_percent
            && reorder > 0.0 {
                let mut reorder_part = format!("reorder {}%", reorder);
                if let Some(reorder_corr) = reorder_correlation
                    && reorder_corr > 0.0 {
                        reorder_part.push_str(&format!(" {}%", reorder_corr));
                    }
                if let Some(gap) = reorder_gap
                    && gap > 0 {
                        reorder_part.push_str(&format!(" gap {}", gap));
                    }
                cmd_parts.push(reorder_part);
            }

        if let Some(corrupt) = corrupt_percent
            && corrupt > 0.0 {
                let mut corrupt_part = format!("corrupt {}%", corrupt);
                if let Some(corrupt_corr) = corrupt_correlation
                    && corrupt_corr > 0.0 {
                        corrupt_part.push_str(&format!(" {}%", corrupt_corr));
                    }
                cmd_parts.push(corrupt_part);
            }

        if let Some(rate) = rate_limit_kbps
            && rate > 0 {
                let rate_part = if rate >= 1000 {
                    format!("rate {}mbit", rate / 1000)
                } else {
                    format!("rate {}kbit", rate)
                };
                cmd_parts.push(rate_part);
            }

        TcConfiguration {
            loss,
            correlation,
            delay_ms,
            delay_jitter_ms,
            delay_correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps,
            command: cmd_parts.join(" "),
        }
    }

    /// Service name
    pub fn name(&self) -> &'static str {
        "tc_service"
    }

    /// Initialize the service
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing TC service");
        self.health_status = ServiceHealth::Healthy;
        Ok(())
    }

    /// Shutdown the service
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down TC service");
        self.tc_config_publishers.clear();
        self.current_configs.clear();
        Ok(())
    }

    /// Get service health status
    pub async fn health_check(&self) -> Result<ServiceHealth> {
        Ok(self.health_status.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenoh::Wait;

    #[test]
    fn test_service_name() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let service = TcService::new(session, "test".to_string());
        assert_eq!(service.name(), "tc_service");
    }

    #[test]
    fn test_build_tc_configuration() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let service = TcService::new(session, "test".to_string());

        let config = service.build_tc_configuration(
            "eth0",
            5.0,
            Some(25.0),
            Some(100.0),
            Some(10.0),
            Some(30.0),
            Some(2.0),
            Some(15.0),
            Some(20.0),
            Some(50.0),
            Some(5),
            Some(1.0),
            Some(10.0),
            Some(1000),
        );

        assert_eq!(config.loss, 5.0);
        assert_eq!(config.correlation, Some(25.0));
        assert_eq!(config.delay_ms, Some(100.0));
        assert_eq!(config.rate_limit_kbps, Some(1000));
        assert!(config
            .command
            .contains("tc qdisc replace dev eth0 root netem"));
        assert!(config.command.contains("loss 5% correlation 25%"));
        assert!(config.command.contains("delay 100ms 10ms 30%"));
    }

    #[test]
    fn test_get_cached_config() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let mut service = TcService::new(session, "test".to_string());

        // Initially no config
        assert!(service.get_cached_config("default", "eth0").is_none());

        // Cache a config
        let config = TcConfiguration {
            loss: 5.0,
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
            command: "test".to_string(),
        };

        service
            .current_configs
            .insert("default/eth0".to_string(), Some(config.clone()));

        // Should return cached config
        let cached = service.get_cached_config("default", "eth0");
        assert!(cached.is_some());
        assert!(cached.unwrap().is_some());
        assert_eq!(cached.unwrap().as_ref().unwrap().loss, 5.0);
    }
}
