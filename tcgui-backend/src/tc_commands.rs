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
use nlink::TcHandle;
use nlink::netlink::Connection;
use nlink::netlink::Route;
use nlink::netlink::namespace::NamespaceSpec;
use nlink::netlink::tc::NetemConfig;
use nlink::netlink::tc_options::{NetemOptions, QdiscOptions};
use nlink::util::{Percent, Rate};
use std::path::Path;
use std::time::Duration;
use tracing::{info, instrument, warn};

use tcgui_shared::{TcNetemConfig, TcValidate, errors::TcguiError};

/// Build a `TcCommandError` from a failed kernel TC operation.
///
/// Logs the kernel's `NETLINK_EXT_ACK` explanation at `warn` so failed applies
/// are visible in backend logs with the precise reason (e.g. an out-of-range
/// netem parameter), using nlink's `ext_ack()` accessor (added in 0.18). The
/// returned message uses the full error Display, which folds the same ext_ack
/// text in automatically since nlink 0.16 — so the frontend sees it too.
fn tc_kernel_err(context: &str, e: &nlink::netlink::Error) -> TcguiError {
    match e.ext_ack() {
        Some(detail) => warn!("{context}: kernel rejected request: {detail}"),
        None => warn!("{context}: {e}"),
    }
    TcguiError::TcCommandError {
        message: format!("{context}: {e}"),
    }
}

/// TC statistics result containing basic, queue, and rate estimator stats.
#[derive(Debug, Clone)]
pub struct TcStatisticsResult {
    /// Basic statistics (bytes/packets transmitted)
    pub basic: tcgui_shared::TcStatsBasic,
    /// Queue statistics (drops/overlimits)
    pub queue: tcgui_shared::TcStatsQueue,
    /// Rate estimator (bps/pps from kernel, if available)
    pub rate_est: Option<tcgui_shared::TcStatsRateEst>,
}

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

    /// Create a NamespaceSpec for the given namespace configuration.
    fn namespace_spec<'a>(
        namespace: &'a str,
        namespace_path: Option<&'a Path>,
    ) -> Result<NamespaceSpec<'a>, TcguiError> {
        if namespace == "default" {
            Ok(NamespaceSpec::Default)
        } else if Self::is_container_namespace(namespace) {
            namespace_path
                .map(NamespaceSpec::Path)
                .ok_or_else(|| TcguiError::NetworkError {
                    message: format!(
                        "Container namespace {} requires a namespace path",
                        namespace
                    ),
                })
        } else {
            // Traditional named namespace
            Ok(NamespaceSpec::Named(namespace))
        }
    }

    /// Create a connection for the appropriate namespace.
    fn create_connection(
        namespace: &str,
        namespace_path: Option<&Path>,
    ) -> Result<Connection<Route>, TcguiError> {
        // Container namespaces are reached through a bind-mount path. A path
        // left behind by an unclean container shutdown is a *stale marker*, not
        // a live netns - nlink's `is_namespace_path` (0.25) tells the two apart
        // via an nsfs `statfs` check. Reject a dead path up front with a clear
        // message instead of surfacing a raw connection failure.
        if Self::is_container_namespace(namespace)
            && let Some(path) = namespace_path
            && !nlink::netlink::namespace::is_namespace_path(path)
        {
            return Err(TcguiError::NetworkError {
                message: format!(
                    "Container namespace '{}' is no longer live (stale namespace path {})",
                    namespace,
                    path.display()
                ),
            });
        }

        let spec = Self::namespace_spec(namespace, namespace_path)?;
        spec.connection().map_err(|e| TcguiError::NetworkError {
            message: format!("Failed to connect to namespace '{}': {}", namespace, e),
        })
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
            conn.get_qdiscs_by_name(interface)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to get qdiscs for {}: {}", interface, e),
                })?;

        // Look for a root qdisc
        for qdisc in qdiscs {
            // Check if this is the root qdisc by examining the parent
            if qdisc.parent().is_root() {
                let kind = qdisc.kind().unwrap_or("unknown");
                return Ok(format!("qdisc {} root", kind));
            }
        }

        Ok(String::new()) // No root qdisc found
    }

    /// Get netem options for an interface if it has a netem qdisc configured.
    /// Returns None if no netem qdisc is found.
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn get_netem_options(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<Option<NetemOptions>> {
        self.get_netem_options_with_path(namespace, None, interface)
            .await
    }

    /// Get netem options for an interface, with optional namespace path for containers.
    #[instrument(skip(self, namespace_path), fields(namespace, interface))]
    pub async fn get_netem_options_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
    ) -> Result<Option<NetemOptions>> {
        let conn = Self::create_connection(namespace, namespace_path)?;

        let qdiscs =
            conn.get_qdiscs_by_name(interface)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to get qdiscs for {}: {}", interface, e),
                })?;

        // Look for a root netem qdisc
        for qdisc in qdiscs {
            // Check if this is the root qdisc by examining the parent
            if qdisc.parent().is_root()
                && let Some(QdiscOptions::Netem(netem_opts)) = qdisc.options()
            {
                let loss_pct = netem_opts.loss().unwrap_or(0.0);
                let delay_ms = netem_opts
                    .delay()
                    .map(|d| d.as_secs_f64() * 1000.0)
                    .unwrap_or(0.0);
                info!(
                    "Found netem qdisc on {}:{} with loss={:.1}%, delay={:.2}ms",
                    namespace, interface, loss_pct, delay_ms
                );
                return Ok(Some(netem_opts));
            }
        }

        Ok(None) // No netem qdisc found
    }

    /// Get TC statistics for an interface if it has a netem qdisc configured.
    /// Returns basic stats (bytes/packets), queue stats (drops/overlimits), and rate estimator.
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn get_tc_statistics(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<Option<TcStatisticsResult>> {
        self.get_tc_statistics_with_path(namespace, None, interface)
            .await
    }

    /// Get TC statistics for an interface, with optional namespace path for containers.
    #[instrument(skip(self, namespace_path), fields(namespace, interface))]
    pub async fn get_tc_statistics_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
    ) -> Result<Option<TcStatisticsResult>> {
        let conn = Self::create_connection(namespace, namespace_path)?;

        let qdiscs =
            conn.get_qdiscs_by_name(interface)
                .await
                .map_err(|e| TcguiError::TcCommandError {
                    message: format!("Failed to get qdiscs for {}: {}", interface, e),
                })?;

        // Look for the root qdisc and extract statistics
        for qdisc in qdiscs {
            if qdisc.parent().is_root() {
                // Only return stats if this is a netem qdisc
                if qdisc.kind() == Some("netem") {
                    let basic = tcgui_shared::TcStatsBasic {
                        bytes: qdisc.bytes(),
                        packets: qdisc.packets(),
                    };
                    let queue = tcgui_shared::TcStatsQueue {
                        qlen: qdisc.qlen(),
                        backlog: qdisc.backlog(),
                        drops: qdisc.drops(),
                        requeues: qdisc.requeues(),
                        overlimits: qdisc.overlimits(),
                    };
                    // Use nlink's bps() and pps() convenience methods for rate estimator
                    let rate_est = if qdisc.bps() > 0 || qdisc.pps() > 0 {
                        Some(tcgui_shared::TcStatsRateEst {
                            bps: qdisc.bps(),
                            pps: qdisc.pps(),
                        })
                    } else {
                        None
                    };
                    return Ok(Some(TcStatisticsResult {
                        basic,
                        queue,
                        rate_est,
                    }));
                }
            }
        }

        Ok(None) // No netem qdisc found
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

        // Check for existing netem options using nlink's typed API
        let existing_netem = self
            .get_netem_options_with_path(namespace, namespace_path, interface)
            .await
            .ok()
            .flatten();

        match existing_netem {
            Some(current_opts) => {
                // Use nlink's requires_recreation_for() to determine if we need delete+add
                if current_opts.requires_recreation_for(&netem_config) {
                    info!(
                        "Recreating netem qdisc on {}/{} (removing parameters)",
                        namespace, interface
                    );
                    let _ = conn.del_qdisc_by_index(ifindex, TcHandle::ROOT).await;
                    conn.add_qdisc_by_index(ifindex, netem_config)
                        .await
                        .map_err(|e| tc_kernel_err("Failed to add netem qdisc after delete", &e))?;
                } else {
                    info!("Replacing netem qdisc on {}/{}", namespace, interface);
                    conn.replace_qdisc_by_index(ifindex, netem_config)
                        .await
                        .map_err(|e| tc_kernel_err("Failed to replace netem qdisc", &e))?;
                }
            }
            None => {
                // No existing netem qdisc - check if there's any other qdisc
                let existing_qdisc = self
                    .check_existing_qdisc_with_path(namespace, namespace_path, interface)
                    .await
                    .unwrap_or_default();

                if existing_qdisc.is_empty() || existing_qdisc.contains("noqueue") {
                    // No qdisc or noqueue - just add
                    info!("Adding new netem qdisc to {}/{}", namespace, interface);
                    conn.add_qdisc_by_index(ifindex, netem_config)
                        .await
                        .map_err(|e| tc_kernel_err("Failed to add netem qdisc", &e))?;
                } else {
                    // Other qdisc type - delete and add
                    info!(
                        "Removing existing qdisc and adding netem on {}/{}",
                        namespace, interface
                    );
                    let _ = conn.del_qdisc_by_index(ifindex, TcHandle::ROOT).await;
                    conn.add_qdisc_by_index(ifindex, netem_config)
                        .await
                        .map_err(|e| tc_kernel_err("Failed to add netem qdisc after delete", &e))?;
                }
            }
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
            netem = netem.loss(Percent::new(config.loss.percentage as f64));
            if config.loss.correlation > 0.0 {
                netem = netem.loss_correlation(Percent::new(config.loss.correlation as f64));
            }
        }

        // Add delay if enabled
        if config.delay.enabled && config.delay.base_ms > 0.0 {
            netem = netem.delay(Duration::from_millis(config.delay.base_ms as u64));
            if config.delay.jitter_ms > 0.0 {
                netem = netem.jitter(Duration::from_millis(config.delay.jitter_ms as u64));
                if config.delay.correlation > 0.0 {
                    netem = netem.delay_correlation(Percent::new(config.delay.correlation as f64));
                }
            }
        }

        // Add duplicate if enabled
        if config.duplicate.enabled && config.duplicate.percentage > 0.0 {
            netem = netem.duplicate(Percent::new(config.duplicate.percentage as f64));
            if config.duplicate.correlation > 0.0 {
                netem =
                    netem.duplicate_correlation(Percent::new(config.duplicate.correlation as f64));
            }
        }

        // Add reorder if enabled
        if config.reorder.enabled && config.reorder.percentage > 0.0 {
            netem = netem.reorder(Percent::new(config.reorder.percentage as f64));
            if config.reorder.correlation > 0.0 {
                netem = netem.reorder_correlation(Percent::new(config.reorder.correlation as f64));
            }
            if config.reorder.gap > 0 {
                netem = netem.gap(config.reorder.gap);
            }
        }

        // Add corrupt if enabled
        if config.corrupt.enabled && config.corrupt.percentage > 0.0 {
            netem = netem.corrupt(Percent::new(config.corrupt.percentage as f64));
            if config.corrupt.correlation > 0.0 {
                netem = netem.corrupt_correlation(Percent::new(config.corrupt.correlation as f64));
            }
        }

        // Add rate limit if enabled
        if config.rate_limit.enabled && config.rate_limit.rate_kbps > 0 {
            netem = netem.rate(Rate::kbit(config.rate_limit.rate_kbps.into()));
        }

        netem.build()
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

        // The connection is already namespace-bound, so nlink resolves the
        // interface name in the correct netns. `del_qdisc_if_exists` (nlink
        // 0.25) returns Ok(false) when there's no root qdisc to remove - it
        // folds the ENOENT/ENODEV "nothing there" cases (and the undeletable
        // default-qdisc EINVAL) into a clean bool, so we no longer resolve the
        // ifindex or match on error predicates by hand.
        match conn.del_qdisc_if_exists(interface, TcHandle::ROOT).await {
            Ok(true) => Ok("TC config removed successfully".to_string()),
            Ok(false) => Ok("No TC config to remove".to_string()),
            Err(e) => Err(TcguiError::TcCommandError {
                message: format!("TC command failed: {}", e),
            }
            .into()),
        }
    }

    /// Capture the current TC state for an interface (for rollback purposes)
    /// Now captures the actual netem configuration for proper restoration.
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

        // Capture the actual netem configuration if present
        let netem_config = if had_netem {
            match self.get_netem_options(namespace, interface).await {
                Ok(Some(opts)) => {
                    // Convert NetemOptions to TcNetemConfig for storage
                    Some(TcNetemConfig {
                        loss: tcgui_shared::TcLossConfig {
                            enabled: opts.loss().unwrap_or(0.0) > 0.0,
                            percentage: opts.loss().unwrap_or(0.0) as f32,
                            correlation: opts.loss_correlation().unwrap_or(0.0) as f32,
                        },
                        delay: tcgui_shared::TcDelayConfig {
                            enabled: opts.delay().map(|d| d.as_millis() > 0).unwrap_or(false),
                            base_ms: opts.delay().map(|d| d.as_millis() as f32).unwrap_or(0.0),
                            jitter_ms: opts.jitter().map(|d| d.as_millis() as f32).unwrap_or(0.0),
                            correlation: opts.delay_correlation().unwrap_or(0.0) as f32,
                        },
                        duplicate: tcgui_shared::TcDuplicateConfig {
                            enabled: opts.duplicate().unwrap_or(0.0) > 0.0,
                            percentage: opts.duplicate().unwrap_or(0.0) as f32,
                            correlation: opts.duplicate_correlation().unwrap_or(0.0) as f32,
                        },
                        reorder: tcgui_shared::TcReorderConfig {
                            enabled: opts.reorder().unwrap_or(0.0) > 0.0,
                            percentage: opts.reorder().unwrap_or(0.0) as f32,
                            correlation: opts.reorder_correlation().unwrap_or(0.0) as f32,
                            gap: opts.gap().unwrap_or(5),
                        },
                        corrupt: tcgui_shared::TcCorruptConfig {
                            enabled: opts.corrupt().unwrap_or(0.0) > 0.0,
                            percentage: opts.corrupt().unwrap_or(0.0) as f32,
                            correlation: opts.corrupt_correlation().unwrap_or(0.0) as f32,
                        },
                        rate_limit: tcgui_shared::TcRateLimitConfig {
                            enabled: opts.rate_bps().map(|r| r > 0).unwrap_or(false),
                            rate_kbps: opts.rate_bps().map(|r| (r / 1000) as u32).unwrap_or(0),
                        },
                    })
                }
                Ok(None) => None,
                Err(e) => {
                    warn!("Could not capture netem options: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let state = CapturedTcState {
            namespace: namespace.to_string(),
            interface: interface.to_string(),
            qdisc_info: qdisc_info.clone(),
            had_netem,
            netem_config,
        };

        info!(
            "Captured TC state: had_netem={}, has_config={}, qdisc_info='{}'",
            had_netem,
            state.netem_config.is_some(),
            qdisc_info.trim()
        );

        Ok(state)
    }

    /// Restore TC state from a previously captured state.
    /// If the captured state includes netem configuration, it will be reapplied.
    #[instrument(skip(self, state), fields(namespace = %state.namespace, interface = %state.interface))]
    pub async fn restore_tc_state(&self, state: &CapturedTcState) -> Result<String> {
        info!(
            "Restoring TC state for {}/{}: had_netem={}, has_config={}",
            state.namespace,
            state.interface,
            state.had_netem,
            state.netem_config.is_some()
        );

        // Remove any current TC configuration first
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

        // If we have a captured netem configuration, reapply it
        if let Some(ref config) = state.netem_config {
            info!(
                "Reapplying captured TC config for {}/{}",
                state.namespace, state.interface
            );

            match self
                .apply_tc_config_structured(&state.namespace, &state.interface, config)
                .await
            {
                Ok(msg) => {
                    info!("Restored TC config: {}", msg);
                    return Ok("TC state restored with original configuration".to_string());
                }
                Err(e) => {
                    warn!("Failed to restore TC config: {}", e);
                    return Ok(format!(
                        "TC state partially restored (config reapply failed: {})",
                        e
                    ));
                }
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
    /// The captured netem configuration (if any) for proper restoration
    pub netem_config: Option<TcNetemConfig>,
}

impl CapturedTcState {
    /// Check if there was any TC configuration
    pub fn had_tc_config(&self) -> bool {
        !self.qdisc_info.is_empty() && self.had_netem
    }
}
