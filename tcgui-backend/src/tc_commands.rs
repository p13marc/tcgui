//! Traffic Control (TC) command execution and management.
//!
//! This module provides comprehensive traffic control using native netlink
//! communication across multiple network namespaces. It handles netem packet
//! loss simulation with correlation support and provides robust error handling.
//!
//! # Key Features
//!
//! * **Native netlink**: Direct kernel communication via rtnetlink (no process spawning)
//! * **Multi-namespace support**: Execute TC commands in default and named namespaces
//! * **Container support**: Works with container network namespaces via setns
//! * **Netem simulation**: Full support for delay, loss, jitter, corruption, reorder, rate
//! * **Comprehensive feedback**: Detailed success/error reporting
//!
//! # Examples
//!
//! ```rust,no_run
//! use tcgui_backend::tc_commands::TcCommandManager;
//! use tcgui_shared::TcNetemConfig;
//!
//! async fn apply_tc_config() -> anyhow::Result<()> {
//!     let tc_manager = TcCommandManager::new();
//!     let config = TcNetemConfig::default();
//!     let result = tc_manager.apply_tc_config_structured("default", "eth0", &config).await?;
//!     println!("TC result: {}", result);
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use std::path::Path;
use tracing::{info, instrument};

use tcgui_shared::{TcNetemConfig, TcValidate, errors::TcguiError};

use crate::tc_netlink::{NetemConfig, TcNetlink};

/// Traffic Control command manager for network emulation.
///
/// This struct manages traffic control using native netlink communication
/// across multiple network namespaces. It provides netem-based network emulation
/// with support for packet loss, delay, jitter, and other network impairments.
///
/// # Architecture
///
/// * **Native netlink**: Uses rtnetlink for direct kernel communication
/// * **No external dependencies**: Does not require `tc` or `ip` commands
/// * **Namespace support**: Works with default, named, and container namespaces
///
/// # Supported Parameters
///
/// * **Packet loss**: 0.0 to 100.0 percent packet loss simulation
/// * **Correlation**: Optional consecutive packet loss correlation (0.0 to 100.0)
/// * **Delay**: Network latency simulation with optional jitter
/// * **Duplication**: Packet duplication percentage
/// * **Reordering**: Packet reordering with gap configuration
/// * **Corruption**: Packet corruption percentage
/// * **Rate limiting**: Bandwidth throttling in kbps
#[derive(Clone)]
pub struct TcCommandManager {
    /// Native netlink-based TC manager
    tc_netlink: TcNetlink,
}

impl Default for TcCommandManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TcCommandManager {
    /// Creates a new TcCommandManager instance.
    ///
    /// # Returns
    ///
    /// A new `TcCommandManager` ready to execute traffic control commands
    /// across multiple network namespaces using native netlink.
    pub fn new() -> Self {
        Self {
            tc_netlink: TcNetlink::new(),
        }
    }

    /// Check if there's an existing qdisc on the interface and return its details.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Target namespace ("default" for host namespace)
    /// * `interface` - Network interface name (e.g., "eth0", "lo")
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Qdisc information (empty string if no qdisc)
    /// * `Err` - On command execution failures
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn check_existing_qdisc(&self, namespace: &str, interface: &str) -> Result<String> {
        self.check_existing_qdisc_with_path(namespace, None, interface)
            .await
    }

    /// Check if there's an existing qdisc on the interface, with optional namespace path for containers.
    #[instrument(skip(self, namespace_path), fields(namespace, interface))]
    pub async fn check_existing_qdisc_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
    ) -> Result<String> {
        match self
            .tc_netlink
            .check_qdisc_with_path(namespace, namespace_path, interface)
            .await
        {
            Ok(Some(kind)) => {
                // Return a formatted string similar to tc output
                Ok(format!("qdisc {} root", kind))
            }
            Ok(None) => {
                Ok(String::new()) // No root qdisc found
            }
            Err(e) => Err(TcguiError::TcCommandError {
                message: format!("Failed to check qdisc: {}", e),
            }
            .into()),
        }
    }

    /// Apply TC config using structured configuration (recommended)
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn apply_tc_config_structured(
        &self,
        namespace: &str,
        interface: &str,
        config: &TcNetemConfig,
    ) -> Result<String> {
        self.apply_tc_config_structured_with_path(namespace, None, interface, config)
            .await
    }

    /// Apply TC config using structured configuration with optional namespace path for containers
    #[instrument(skip(self, namespace_path), fields(namespace, interface))]
    pub async fn apply_tc_config_structured_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
        config: &TcNetemConfig,
    ) -> Result<String> {
        // Validate configuration first
        config.validate().map_err(|e| TcguiError::TcCommandError {
            message: format!("TC configuration validation failed: {}", e),
        })?;

        info!(
            "Applying structured TC config: namespace={}, interface={}, config={:?}",
            namespace, interface, config
        );

        let netem_config = Self::tc_netem_to_netlink_config(config);

        self.tc_netlink
            .apply_netem_with_path(namespace, namespace_path, interface, &netem_config)
            .await
            .map_err(|e| {
                TcguiError::TcCommandError {
                    message: format!("Failed to apply netem qdisc: {}", e),
                }
                .into()
            })
    }

    /// Convert TcNetemConfig to NetemConfig for native netlink
    fn tc_netem_to_netlink_config(config: &TcNetemConfig) -> NetemConfig {
        NetemConfig {
            delay_ms: if config.delay.enabled && config.delay.base_ms > 0.0 {
                Some(config.delay.base_ms)
            } else {
                None
            },
            jitter_ms: if config.delay.enabled && config.delay.jitter_ms > 0.0 {
                Some(config.delay.jitter_ms)
            } else {
                None
            },
            delay_correlation: if config.delay.enabled && config.delay.correlation > 0.0 {
                Some(config.delay.correlation)
            } else {
                None
            },
            loss_percent: if config.loss.enabled && config.loss.percentage > 0.0 {
                Some(config.loss.percentage)
            } else {
                None
            },
            loss_correlation: if config.loss.enabled && config.loss.correlation > 0.0 {
                Some(config.loss.correlation)
            } else {
                None
            },
            duplicate_percent: if config.duplicate.enabled && config.duplicate.percentage > 0.0 {
                Some(config.duplicate.percentage)
            } else {
                None
            },
            duplicate_correlation: if config.duplicate.enabled && config.duplicate.correlation > 0.0
            {
                Some(config.duplicate.correlation)
            } else {
                None
            },
            reorder_percent: if config.reorder.enabled && config.reorder.percentage > 0.0 {
                Some(config.reorder.percentage)
            } else {
                None
            },
            reorder_correlation: if config.reorder.enabled && config.reorder.correlation > 0.0 {
                Some(config.reorder.correlation)
            } else {
                None
            },
            reorder_gap: if config.reorder.enabled && config.reorder.gap > 0 {
                Some(config.reorder.gap)
            } else {
                None
            },
            corrupt_percent: if config.corrupt.enabled && config.corrupt.percentage > 0.0 {
                Some(config.corrupt.percentage)
            } else {
                None
            },
            corrupt_correlation: if config.corrupt.enabled && config.corrupt.correlation > 0.0 {
                Some(config.corrupt.correlation)
            } else {
                None
            },
            rate_limit_kbps: if config.rate_limit.enabled && config.rate_limit.rate_kbps > 0 {
                Some(config.rate_limit.rate_kbps)
            } else {
                None
            },
            limit: None,
        }
    }

    /// Applies traffic control configuration to an interface in a specific namespace.
    ///
    /// This is a legacy method that converts individual parameters to TcNetemConfig.
    /// Consider using `apply_tc_config_structured` instead.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        skip(self),
        fields(
            namespace,
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
            rate_limit_kbps
        )
    )]
    pub async fn apply_tc_config_in_namespace(
        &self,
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
    ) -> Result<String> {
        self.apply_tc_config_in_namespace_with_path(
            namespace,
            None,
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
        )
        .await
    }

    /// Applies traffic control configuration with optional namespace path for containers
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        skip(self, namespace_path),
        fields(
            namespace,
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
            rate_limit_kbps
        )
    )]
    pub async fn apply_tc_config_in_namespace_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
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
    ) -> Result<String> {
        info!(
            "Applying TC config: namespace={}, interface={}, loss={}%, delay={:?}ms",
            namespace, interface, loss, delay_ms
        );

        // Convert legacy parameters to NetemConfig
        let netem_config = NetemConfig {
            delay_ms,
            jitter_ms: delay_jitter_ms,
            delay_correlation,
            loss_percent: if loss > 0.0 { Some(loss) } else { None },
            loss_correlation: correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps,
            limit: None,
        };

        self.tc_netlink
            .apply_netem_with_path(namespace, namespace_path, interface, &netem_config)
            .await
            .map_err(|e| {
                TcguiError::TcCommandError {
                    message: format!("Failed to apply netem qdisc: {}", e),
                }
                .into()
            })
    }

    /// Remove TC config in default namespace
    pub async fn remove_tc_config(&self, interface: &str) -> Result<String> {
        self.remove_tc_config_in_namespace("default", interface)
            .await
    }

    /// Removes traffic control configuration from an interface in a specific namespace.
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn remove_tc_config_in_namespace(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<String> {
        self.remove_tc_config_in_namespace_with_path(namespace, None, interface)
            .await
    }

    /// Removes traffic control configuration with optional namespace path for containers
    #[instrument(skip(self, namespace_path), fields(namespace, interface))]
    pub async fn remove_tc_config_in_namespace_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
    ) -> Result<String> {
        info!(
            "Removing TC config for interface: {} in namespace: {}",
            interface, namespace
        );

        self.tc_netlink
            .remove_qdisc_with_path(namespace, namespace_path, interface)
            .await
            .map_err(|e| {
                TcguiError::TcCommandError {
                    message: format!("Failed to remove qdisc: {}", e),
                }
                .into()
            })
    }

    /// Capture the current TC state for an interface (for rollback purposes)
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn capture_tc_state(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<CapturedTcState> {
        info!(
            "Capturing TC state for rollback: namespace={}, interface={}",
            namespace, interface
        );

        let qdisc_info = match self.check_existing_qdisc(namespace, interface).await {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!(
                    "Could not capture TC state for {}/{}: {}, assuming no TC configured",
                    namespace,
                    interface,
                    e
                );
                String::new()
            }
        };

        let had_netem = qdisc_info.contains("netem");

        let state = CapturedTcState {
            namespace: namespace.to_string(),
            interface: interface.to_string(),
            qdisc_info: qdisc_info.clone(),
            had_netem,
        };

        info!(
            "Captured TC state: had_netem={}, qdisc_info='{}'",
            had_netem,
            qdisc_info.trim()
        );

        Ok(state)
    }

    /// Restore TC state from a previously captured state
    #[instrument(skip(self, state), fields(namespace = %state.namespace, interface = %state.interface))]
    pub async fn restore_tc_state(&self, state: &CapturedTcState) -> Result<String> {
        info!(
            "Restoring TC state for {}/{}: had_netem={}",
            state.namespace, state.interface, state.had_netem
        );

        // Remove any current TC configuration
        match self
            .remove_tc_config_in_namespace(&state.namespace, &state.interface)
            .await
        {
            Ok(msg) => {
                info!("Removed current TC config: {}", msg);
            }
            Err(e) => {
                info!("Note while removing TC config: {}", e);
            }
        }

        if !state.had_tc_config() {
            info!(
                "Original state had no TC config, interface {}/{} restored to clean state",
                state.namespace, state.interface
            );
            return Ok("TC state restored (no previous configuration)".to_string());
        }

        info!(
            "Interface {}/{} restored to clean state (previous TC config was present but not re-applied)",
            state.namespace, state.interface
        );

        Ok("TC state restored (previous config cleared)".to_string())
    }
}

/// Captured TC state for rollback purposes
#[derive(Debug, Clone)]
pub struct CapturedTcState {
    /// The namespace of the interface
    pub namespace: String,
    /// The interface name
    pub interface: String,
    /// Raw qdisc info string (empty if no qdisc was configured)
    pub qdisc_info: String,
    /// Whether there was a netem qdisc configured
    pub had_netem: bool,
}

impl CapturedTcState {
    /// Check if there was any TC configuration
    pub fn had_tc_config(&self) -> bool {
        !self.qdisc_info.is_empty() && self.had_netem
    }
}
