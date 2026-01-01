//! Traffic Control (TC) command execution and management.
//!
//! This module provides comprehensive traffic control command execution across
//! multiple network namespaces using nlink's typed TC API. It handles netem
//! packet loss simulation with correlation support and provides robust error
//! handling and feedback.
//!
//! # Key Features
//!
//! * **Multi-namespace support**: Execute TC commands in default and named namespaces
//! * **Netem simulation**: Packet loss, delay, duplication, reordering, corruption
//! * **Native netlink**: Uses nlink for direct kernel communication (no process spawning)
//! * **Comprehensive feedback**: Detailed success/error reporting to frontend
//! * **Robust error handling**: Graceful handling of common TC command failures

use anyhow::Result;
use nlink::netlink::tc::NetemConfig;
use nlink::netlink::{Connection, Protocol, namespace};
use std::path::Path;
use std::time::Duration;
use tracing::{info, instrument, warn};

use tcgui_shared::{TcNetemConfig, TcValidate, errors::TcguiError};

/// Traffic Control command manager for network emulation.
///
/// This struct manages the execution of Linux TC (traffic control) commands
/// across multiple network namespaces using nlink's native netlink API.
#[derive(Clone)]
pub struct TcCommandManager {
    // Stateless - connections are created per-operation for namespace isolation
}

impl Default for TcCommandManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TcCommandManager {
    /// Creates a new TcCommandManager instance.
    pub fn new() -> Self {
        Self {}
    }

    /// Check if a namespace is a container namespace (starts with "container:")
    fn is_container_namespace(namespace: &str) -> bool {
        namespace.starts_with("container:")
    }

    /// Create a connection for the appropriate namespace.
    fn create_connection(
        namespace: &str,
        namespace_path: Option<&Path>,
    ) -> Result<Connection, TcguiError> {
        if namespace == "default" {
            Connection::new(Protocol::Route).map_err(|e| TcguiError::NetworkError {
                message: format!("Failed to create nlink connection: {}", e),
            })
        } else if Self::is_container_namespace(namespace) {
            if let Some(ns_path) = namespace_path {
                Connection::new_in_namespace_path(Protocol::Route, ns_path).map_err(|e| {
                    TcguiError::NetworkError {
                        message: format!("Failed to connect to container namespace: {}", e),
                    }
                })
            } else {
                Err(TcguiError::NetworkError {
                    message: format!(
                        "Container namespace {} requires a namespace path",
                        namespace
                    ),
                })
            }
        } else {
            // Traditional named namespace
            namespace::connection_for(namespace).map_err(|e| TcguiError::NetworkError {
                message: format!("Failed to connect to namespace {}: {}", namespace, e),
            })
        }
    }

    /// Check if there's an existing qdisc on the interface and return its details.
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
        let conn = Self::create_connection(namespace, namespace_path)?;

        let qdiscs =
            conn.get_qdiscs_for(interface)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to get qdiscs for {}: {}", interface, e),
                })?;

        // Look for a root qdisc
        for qdisc in qdiscs {
            // Check if this is the root qdisc by examining the parent
            if qdisc.parent() == 0xFFFFFFFF {
                // TC_H_ROOT
                let kind = qdisc.kind().unwrap_or("unknown");
                return Ok(format!("qdisc {} root", kind));
            }
        }

        Ok(String::new()) // No root qdisc found
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

        let conn = Self::create_connection(namespace, namespace_path)?;

        // Get interface index
        let link = conn
            .get_link_by_name(interface)
            .await
            .map_err(|e| TcguiError::TcCommandError {
                message: format!("Failed to get interface {}: {}", interface, e),
            })?
            .ok_or_else(|| TcguiError::TcCommandError {
                message: format!("Interface {} not found", interface),
            })?;

        let ifindex = link.ifindex();

        // Build nlink NetemConfig from TcNetemConfig
        let netem_config = self.build_netem_config(config);

        // Check if there's an existing qdisc
        let existing_qdisc = self
            .check_existing_qdisc_with_path(namespace, namespace_path, interface)
            .await
            .unwrap_or_default();

        if existing_qdisc.is_empty() {
            // No existing qdisc, add new one
            info!("Adding new netem qdisc to {}/{}", namespace, interface);
            conn.add_qdisc_by_index(ifindex, netem_config)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to add netem qdisc: {}", e),
                })?;
        } else if existing_qdisc.contains("netem") {
            // Check if we need to recreate (to remove parameters)
            let (
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
            ) = config.to_legacy_params();

            let current_config = self.parse_current_tc_config(&existing_qdisc);
            let needs_recreation = self.needs_qdisc_recreation(
                &current_config,
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

            if needs_recreation {
                info!(
                    "Recreating netem qdisc on {}/{} (removing parameters)",
                    namespace, interface
                );
                // Delete first, then add
                let _ = conn.del_qdisc_by_index(ifindex, "root").await;
                conn.add_qdisc_by_index(ifindex, netem_config)
                    .await
                    .map_err(|e| TcguiError::TcCommandError {
                        message: format!("Failed to add netem qdisc after delete: {}", e),
                    })?;
            } else {
                info!("Replacing netem qdisc on {}/{}", namespace, interface);
                conn.replace_qdisc_by_index(ifindex, netem_config)
                    .await
                    .map_err(|e| TcguiError::TcCommandError {
                        message: format!("Failed to replace netem qdisc: {}", e),
                    })?;
            }
        } else if existing_qdisc.contains("noqueue") {
            // noqueue can be replaced with add
            info!(
                "Adding netem qdisc to replace noqueue on {}/{}",
                namespace, interface
            );
            conn.add_qdisc_by_index(ifindex, netem_config)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to add netem qdisc: {}", e),
                })?;
        } else {
            // Other qdisc type - try delete and add
            info!(
                "Removing existing qdisc and adding netem on {}/{}",
                namespace, interface
            );
            let _ = conn.del_qdisc_by_index(ifindex, "root").await;
            conn.add_qdisc_by_index(ifindex, netem_config)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to add netem qdisc after delete: {}", e),
                })?;
        }

        Ok(format!(
            "TC config applied successfully to {}:{}",
            namespace, interface
        ))
    }

    /// Build nlink NetemConfig from TcNetemConfig
    fn build_netem_config(&self, config: &TcNetemConfig) -> NetemConfig {
        let mut netem = NetemConfig::new();

        // Add loss if enabled
        if config.loss.enabled && config.loss.percentage > 0.0 {
            netem = netem.loss(config.loss.percentage as f64);
            if config.loss.correlation > 0.0 {
                netem = netem.loss_correlation(config.loss.correlation as f64);
            }
        }

        // Add delay if enabled
        if config.delay.enabled && config.delay.base_ms > 0.0 {
            netem = netem.delay(Duration::from_micros(
                (config.delay.base_ms * 1000.0) as u64,
            ));
            if config.delay.jitter_ms > 0.0 {
                netem = netem.jitter(Duration::from_micros(
                    (config.delay.jitter_ms * 1000.0) as u64,
                ));
                if config.delay.correlation > 0.0 {
                    netem = netem.delay_correlation(config.delay.correlation as f64);
                }
            }
        }

        // Add duplicate if enabled
        if config.duplicate.enabled && config.duplicate.percentage > 0.0 {
            netem = netem.duplicate(config.duplicate.percentage as f64);
            if config.duplicate.correlation > 0.0 {
                netem = netem.duplicate_correlation(config.duplicate.correlation as f64);
            }
        }

        // Add reorder if enabled
        if config.reorder.enabled && config.reorder.percentage > 0.0 {
            netem = netem.reorder(config.reorder.percentage as f64);
            if config.reorder.correlation > 0.0 {
                netem = netem.reorder_correlation(config.reorder.correlation as f64);
            }
            if config.reorder.gap > 0 {
                netem = netem.gap(config.reorder.gap);
            }
        }

        // Add corrupt if enabled
        if config.corrupt.enabled && config.corrupt.percentage > 0.0 {
            netem = netem.corrupt(config.corrupt.percentage as f64);
            if config.corrupt.correlation > 0.0 {
                netem = netem.corrupt_correlation(config.corrupt.correlation as f64);
            }
        }

        // Add rate limit if enabled
        if config.rate_limit.enabled && config.rate_limit.rate_kbps > 0 {
            // Convert kbps to bytes per second
            let bytes_per_sec = (config.rate_limit.rate_kbps as u64) * 1000 / 8;
            netem = netem.rate(bytes_per_sec);
        }

        netem.build()
    }

    /// Apply TC config in default namespace (legacy method)
    #[deprecated(
        since = "0.2.0",
        note = "Use apply_tc_config_structured() with TcNetemConfig instead"
    )]
    #[allow(dead_code)]
    pub async fn apply_tc_config(
        &self,
        interface: &str,
        loss: f32,
        correlation: Option<f32>,
    ) -> Result<String> {
        #[allow(deprecated)]
        self.apply_tc_config_in_namespace(
            "default",
            interface,
            loss,
            correlation,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Applies traffic control configuration to an interface in a specific namespace (legacy).
    #[deprecated(
        since = "0.2.0",
        note = "Use apply_tc_config_structured() with TcNetemConfig instead"
    )]
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(self), fields(namespace, interface))]
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
    #[instrument(skip(self, namespace_path), fields(namespace, interface))]
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
            "Applying TC config: namespace={}, interface={}, loss={}%",
            namespace, interface, loss
        );

        let conn = Self::create_connection(namespace, namespace_path)?;

        // Get interface index
        let link = conn
            .get_link_by_name(interface)
            .await
            .map_err(|e| TcguiError::TcCommandError {
                message: format!("Failed to get interface {}: {}", interface, e),
            })?
            .ok_or_else(|| TcguiError::TcCommandError {
                message: format!("Interface {} not found", interface),
            })?;

        let ifindex = link.ifindex();

        // Build netem config
        let mut netem = NetemConfig::new();

        if loss > 0.0 {
            netem = netem.loss(loss as f64);
            if let Some(corr) = correlation
                && corr > 0.0
            {
                netem = netem.loss_correlation(corr as f64);
            }
        }

        if let Some(delay) = delay_ms
            && delay > 0.0
        {
            netem = netem.delay(Duration::from_micros((delay * 1000.0) as u64));
            if let Some(jitter) = delay_jitter_ms
                && jitter > 0.0
            {
                netem = netem.jitter(Duration::from_micros((jitter * 1000.0) as u64));
                if let Some(corr) = delay_correlation
                    && corr > 0.0
                {
                    netem = netem.delay_correlation(corr as f64);
                }
            }
        }

        if let Some(dup) = duplicate_percent
            && dup > 0.0
        {
            netem = netem.duplicate(dup as f64);
            if let Some(corr) = duplicate_correlation
                && corr > 0.0
            {
                netem = netem.duplicate_correlation(corr as f64);
            }
        }

        if let Some(reorder) = reorder_percent
            && reorder > 0.0
        {
            // Ensure delay is present for reordering
            if delay_ms.is_none_or(|d| d <= 0.0) {
                netem = netem.delay(Duration::from_millis(1));
            }
            netem = netem.reorder(reorder as f64);
            if let Some(corr) = reorder_correlation
                && corr > 0.0
            {
                netem = netem.reorder_correlation(corr as f64);
            }
            if let Some(gap) = reorder_gap
                && gap > 0
            {
                netem = netem.gap(gap);
            }
        }

        if let Some(corrupt) = corrupt_percent
            && corrupt > 0.0
        {
            netem = netem.corrupt(corrupt as f64);
            if let Some(corr) = corrupt_correlation
                && corr > 0.0
            {
                netem = netem.corrupt_correlation(corr as f64);
            }
        }

        if let Some(rate) = rate_limit_kbps
            && rate > 0
        {
            let bytes_per_sec = (rate as u64) * 1000 / 8;
            netem = netem.rate(bytes_per_sec);
        }

        let netem_config = netem.build();

        // Check existing qdisc
        let existing_qdisc = self
            .check_existing_qdisc_with_path(namespace, namespace_path, interface)
            .await
            .unwrap_or_default();

        if existing_qdisc.is_empty() {
            info!("Adding new netem qdisc to {}/{}", namespace, interface);
            conn.add_qdisc_by_index(ifindex, netem_config)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to add netem qdisc: {}", e),
                })?;
        } else if existing_qdisc.contains("netem") {
            let current_config = self.parse_current_tc_config(&existing_qdisc);
            let needs_recreation = self.needs_qdisc_recreation(
                &current_config,
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

            if needs_recreation {
                info!("Recreating netem qdisc on {}/{}", namespace, interface);
                let _ = conn.del_qdisc_by_index(ifindex, "root").await;
                conn.add_qdisc_by_index(ifindex, netem_config)
                    .await
                    .map_err(|e| TcguiError::TcCommandError {
                        message: format!("Failed to add netem qdisc: {}", e),
                    })?;
            } else {
                info!("Replacing netem qdisc on {}/{}", namespace, interface);
                conn.replace_qdisc_by_index(ifindex, netem_config)
                    .await
                    .map_err(|e| TcguiError::TcCommandError {
                        message: format!("Failed to replace netem qdisc: {}", e),
                    })?;
            }
        } else {
            info!(
                "Removing existing qdisc and adding netem on {}/{}",
                namespace, interface
            );
            let _ = conn.del_qdisc_by_index(ifindex, "root").await;
            conn.add_qdisc_by_index(ifindex, netem_config)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to add netem qdisc: {}", e),
                })?;
        }

        Ok(format!(
            "TC config applied successfully to {}:{}",
            namespace, interface
        ))
    }

    /// Remove TC config in default namespace (legacy method)
    #[allow(dead_code)]
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

        let conn = Self::create_connection(namespace, namespace_path)?;

        // Get interface index
        let link = conn
            .get_link_by_name(interface)
            .await
            .map_err(|e| TcguiError::TcCommandError {
                message: format!("Failed to get interface {}: {}", interface, e),
            })?
            .ok_or_else(|| TcguiError::TcCommandError {
                message: format!("Interface {} not found", interface),
            })?;

        let ifindex = link.ifindex();

        match conn.del_qdisc_by_index(ifindex, "root").await {
            Ok(()) => Ok("TC config removed successfully".to_string()),
            Err(e) => {
                let err_str = e.to_string();
                // "No such file or directory" means no qdisc to remove - that's fine
                if err_str.contains("No such file") || err_str.contains("ENOENT") {
                    Ok("No TC config to remove".to_string())
                } else {
                    Err(TcguiError::TcCommandError {
                        message: format!("TC command failed: {}", e),
                    }
                    .into())
                }
            }
        }
    }

    /// Parse current TC configuration from qdisc info string
    fn parse_current_tc_config(&self, qdisc_info: &str) -> CurrentTcConfig {
        CurrentTcConfig {
            has_loss: qdisc_info.contains("loss"),
            has_delay: qdisc_info.contains("delay"),
            has_duplicate: qdisc_info.contains("duplicate"),
            has_reorder: qdisc_info.contains("reorder"),
            has_corrupt: qdisc_info.contains("corrupt"),
            has_rate: qdisc_info.contains("rate"),
        }
    }

    /// Determine if we need to recreate the qdisc (delete + add) vs just replace
    #[allow(clippy::too_many_arguments)]
    fn needs_qdisc_recreation(
        &self,
        current: &CurrentTcConfig,
        loss: f32,
        _correlation: Option<f32>,
        delay_ms: Option<f32>,
        _delay_jitter_ms: Option<f32>,
        _delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        _duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        _reorder_correlation: Option<f32>,
        _reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        _corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> bool {
        let will_remove_loss = current.has_loss && loss <= 0.0;
        let will_remove_delay = current.has_delay && delay_ms.is_none_or(|d| d <= 0.0);
        let will_remove_duplicate =
            current.has_duplicate && duplicate_percent.is_none_or(|d| d <= 0.0);
        let will_remove_reorder = current.has_reorder && reorder_percent.is_none_or(|r| r <= 0.0);
        let will_remove_corrupt = current.has_corrupt && corrupt_percent.is_none_or(|c| c <= 0.0);
        let will_remove_rate = current.has_rate && rate_limit_kbps.is_none_or(|r| r == 0);

        let needs_recreation = will_remove_loss
            || will_remove_delay
            || will_remove_duplicate
            || will_remove_reorder
            || will_remove_corrupt
            || will_remove_rate;

        if needs_recreation {
            info!(
                "Qdisc recreation needed: loss={}, delay={}, dup={}, reorder={}, corrupt={}, rate={}",
                will_remove_loss,
                will_remove_delay,
                will_remove_duplicate,
                will_remove_reorder,
                will_remove_corrupt,
                will_remove_rate
            );
        }

        needs_recreation
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
                warn!(
                    "Could not capture TC state for {}/{}: {}, assuming no TC configured",
                    namespace, interface, e
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
            "Interface {}/{} restored to clean state",
            state.namespace, state.interface
        );

        Ok("TC state restored (previous config cleared)".to_string())
    }
}

/// Simple structure to track which TC parameters are currently active
#[derive(Debug)]
struct CurrentTcConfig {
    has_loss: bool,
    has_delay: bool,
    has_duplicate: bool,
    has_reorder: bool,
    has_corrupt: bool,
    has_rate: bool,
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
