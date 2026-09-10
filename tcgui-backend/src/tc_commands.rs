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

        // A plug grafted under this netem does not survive a delete+add: the
        // child goes with the parent, taking its buffer. Rather than stall
        // silently or drop packets behind the operator's back, refuse the
        // recreation and name the plug. The `replace` branch is safe — same
        // kind, no NLM_F_EXCL, so the kernel takes the `qdisc_change` path and
        // the grafted child survives (asserted in live_tc.rs).
        let plugged = Self::qdisc_layout(&conn, interface)
            .await
            .ok()
            .and_then(|(_, _, p)| p)
            .is_some();

        match existing_netem {
            Some(current_opts) => {
                // Use nlink's requires_recreation_for() to determine if we need delete+add
                if current_opts.requires_recreation_for(&netem_config) {
                    if plugged {
                        return Err(TcguiError::TcCommandError {
                            message: format!(
                                "{}/{} is plugged; release the plug before removing netem \
                                 parameters (this change needs the qdisc recreated, which \
                                 would drop the buffered packets)",
                                namespace, interface
                            ),
                        }
                        .into());
                    }
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
        // A plug grafted under the root netem holds packets. Deleting the root
        // tears the child down with it and `qdisc_reset_queue` frees whatever
        // it was holding — a silent packet loss with nothing in the logs. So
        // release and let the backlog drain first; the removal below then
        // takes down an empty plug.
        if let Ok(Some((parent, backlog, _))) = Self::qdisc_layout(&conn, interface)
            .await
            .map(|(_, _, p)| p)
            && backlog > 0
        {
            warn!(
                "Releasing a plug holding {} bytes on {}/{} before removing TC config",
                backlog, namespace, interface
            );
            if let Ok(ifindex) = Self::resolve_ifindex(&conn, interface).await {
                let dev = nlink::netlink::InterfaceRef::index(ifindex);
                let _ = conn.plug_release_indefinite(dev, parent).await;
                for _ in 0..25 {
                    match Self::qdisc_layout(&conn, interface).await {
                        Ok((_, _, Some((_, b, _)))) if b > 0 => {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                        _ => break,
                    }
                }
            }
        }

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

// ---------------------------------------------------------------------------
// Plug qdisc (stall / release)
// ---------------------------------------------------------------------------

/// Where a plug qdisc sits and what the backend knows about it.
///
/// `sch_plug` has **no kernel dump op** — `QdiscOptions` has no `Plug` variant
/// because there is nothing to parse. So presence, backlog and qlen are
/// readable from the kernel, but the epoch (buffering vs released) is not.
/// That half is backend-owned truth, which is why it is carried here and
/// published rather than re-derived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlugSnapshot {
    /// The handle the plug is grafted under, i.e. netem's `<major>:1`.
    pub parent: TcHandle,
    /// Whether the backend believes packets are being held.
    pub buffering: bool,
    /// Buffer ceiling in bytes. Always concrete: the kernel cannot be asked
    /// for its own default through nlink (see `plug_begin`), so when the caller
    /// supplies none the backend computes `txqueuelen × MTU` itself.
    pub limit_bytes: u32,
    /// Whether the parent netem exists only to host this plug, and should be
    /// removed with it.
    pub netem_synthesized: bool,
    /// Bytes currently held.
    pub buffered_bytes: u32,
    /// Packets currently held.
    pub buffered_packets: u32,
}

impl TcCommandManager {
    /// The handle a plug is grafted at, given the root netem's handle.
    ///
    /// netem is classful with exactly one leaf, so the plug lives at
    /// `<netem major>:1`. **The major is not always 1**: `add_qdisc_by_index`
    /// passes `handle = None`, so unless a handle is pinned the kernel assigns
    /// one (`8001:` and up). Reading it back from the dump is what keeps this
    /// correct for qdiscs this backend did not create.
    fn plug_parent(root_handle: TcHandle) -> Result<TcHandle, TcguiError> {
        let major = root_handle.major();
        if major == 0 {
            return Err(TcguiError::TcCommandError {
                message: "root qdisc has no handle major; cannot address a child".to_string(),
            });
        }
        Ok(TcHandle::new(major, 1))
    }

    /// Find the root qdisc and any plug child in a single dump.
    ///
    /// Returns `(root_kind, root_handle, plug_child)`.
    async fn qdisc_layout(
        conn: &Connection<Route>,
        interface: &str,
    ) -> Result<
        (
            Option<String>,
            Option<TcHandle>,
            Option<(TcHandle, u32, u32)>,
        ),
        TcguiError,
    > {
        let qdiscs = conn
            .get_qdiscs_by_name(interface)
            .await
            .map_err(|e| tc_kernel_err("Failed to read qdiscs", &e))?;

        let mut root_kind = None;
        let mut root_handle = None;
        let mut plug = None;
        for q in &qdiscs {
            if q.parent().is_root() {
                root_kind = q.kind().map(str::to_string);
                root_handle = Some(q.handle());
            } else if q.kind() == Some("plug") {
                plug = Some((q.parent(), q.backlog(), q.qlen()));
            }
        }
        Ok((root_kind, root_handle, plug))
    }

    /// Probe an interface's plug, if it has one.
    ///
    /// Kernel truth only: presence and how much is held. The epoch is not
    /// readable, so the caller supplies it.
    pub async fn plug_probe(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
    ) -> Result<Option<(TcHandle, u32, u32)>, TcguiError> {
        let conn = Self::create_connection(namespace, namespace_path)?;
        let (_, _, plug) = Self::qdisc_layout(&conn, interface).await?;
        Ok(plug)
    }

    /// Install the plug (if absent) and begin a buffering epoch.
    ///
    /// **This stalls the interface.** Installing a plug qdisc starts buffering
    /// immediately; on an already-installed plug, `TCQ_PLUG_BUFFER` starts a
    /// fresh epoch.
    ///
    /// netem stays at the root and keeps every impairment working — the plug is
    /// its single leaf, so packets are held *after* being impaired. If the
    /// interface has no qdisc (or only a kernel default), a bare netem is
    /// synthesized to host the plug and recorded as such so removal can undo it.
    /// A root qdisc that is neither is refused rather than destroyed: replacing
    /// someone's `cake` or `htb` to install a stall is not this tool's call.
    pub async fn plug_begin(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
        limit_bytes: Option<u32>,
    ) -> Result<PlugSnapshot, TcguiError> {
        let conn = Self::create_connection(namespace, namespace_path)?;
        let link = Self::resolve_link(&conn, interface).await?;
        let ifindex = link.ifindex();
        let (root_kind, root_handle, existing_plug) = Self::qdisc_layout(&conn, interface).await?;

        // nlink's `PlugConfig::write_options` emits nothing when no limit is
        // set, but `add_qdisc_by_index_full` still opens and closes the
        // TCA_OPTIONS nest — so the kernel sees a *present, zero-length*
        // attribute. `plug_init` reads that as `opt != NULL` with
        // `nla_len(opt) < sizeof(struct tc_plug_qopt)` and returns EINVAL, so
        // the plain `PlugConfig::new().build()` case cannot install a plug at
        // all. Reported upstream; until it is fixed there is no way to ask the
        // kernel for its own default, so compute the same value the kernel
        // would have (`txqueuelen × MTU`) and always send a concrete limit.
        let limit = limit_bytes.unwrap_or_else(|| {
            link.txqlen()
                .unwrap_or(1000)
                .saturating_mul(link.mtu().unwrap_or(1500))
                .max(64 * 1024)
        });

        let (major, netem_synthesized) = match root_kind.as_deref() {
            Some("netem") => (
                Self::plug_parent(root_handle.unwrap_or(TcHandle::ROOT))?.major(),
                false,
            ),
            None | Some("noqueue") | Some("pfifo_fast") | Some("mq") | Some("pfifo")
            | Some("bfifo") => {
                // `replace`, not `add`: an `mq` root is present-but-replaceable,
                // and replace covers the absent case too.
                info!(
                    "Synthesizing a netem root on {}/{} to host the plug",
                    namespace, interface
                );
                conn.replace_qdisc_by_index_full(
                    ifindex,
                    TcHandle::ROOT,
                    Some(TcHandle::major_only(1)),
                    NetemConfig::new().build(),
                )
                .await
                .map_err(|e| tc_kernel_err("Failed to add netem root for plug", &e))?;
                (1, true)
            }
            Some(other) => {
                return Err(TcguiError::TcCommandError {
                    message: format!(
                        "{}/{} has a '{}' root qdisc; refusing to replace it to install a plug",
                        namespace, interface, other
                    ),
                });
            }
        };

        let parent = TcHandle::new(major, 1);
        let dev = nlink::netlink::InterfaceRef::index(ifindex);

        match existing_plug {
            Some((existing_parent, _, _)) if existing_parent == parent => {
                // Already grafted — start a new epoch rather than re-adding.
                conn.plug_buffer(dev, parent)
                    .await
                    .map_err(|e| tc_kernel_err("Failed to start a plug buffering epoch", &e))?;
                conn.plug_set_limit(nlink::netlink::InterfaceRef::index(ifindex), parent, limit)
                    .await
                    .map_err(|e| tc_kernel_err("Failed to set the plug limit", &e))?;
            }
            Some((existing_parent, _, _)) => {
                return Err(TcguiError::TcCommandError {
                    message: format!(
                        "{}/{} already has a plug at {} but the root netem is at {}:",
                        namespace, interface, existing_parent, major
                    ),
                });
            }
            None => {
                let cfg = nlink::netlink::tc::PlugConfig::new().limit(limit).build();
                conn.add_qdisc_by_index_full(ifindex, parent, None, cfg)
                    .await
                    .map_err(|e| tc_kernel_err("Failed to graft the plug onto netem", &e))?;
            }
        }

        let (_, _, plug) = Self::qdisc_layout(&conn, interface).await?;
        let (buffered_bytes, buffered_packets) = plug.map(|(_, b, q)| (b, q)).unwrap_or((0, 0));

        info!(
            "Plug buffering on {}/{} at {} (limit {} bytes)",
            namespace, interface, parent, limit
        );
        Ok(PlugSnapshot {
            parent,
            buffering: true,
            limit_bytes: limit,
            netem_synthesized,
            buffered_bytes,
            buffered_packets,
        })
    }

    /// Release what is buffered now, then keep buffering.
    pub async fn plug_release_one(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
        parent: TcHandle,
    ) -> Result<(), TcguiError> {
        let conn = Self::create_connection(namespace, namespace_path)?;
        let ifindex = Self::resolve_ifindex(&conn, interface).await?;
        conn.plug_release_one(nlink::netlink::InterfaceRef::index(ifindex), parent)
            .await
            .map_err(|e| tc_kernel_err("Failed to release one plug epoch", &e))
    }

    /// Stop buffering and let everything through, leaving the qdisc installed.
    pub async fn plug_release(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
        parent: TcHandle,
    ) -> Result<(), TcguiError> {
        let conn = Self::create_connection(namespace, namespace_path)?;
        let ifindex = Self::resolve_ifindex(&conn, interface).await?;
        conn.plug_release_indefinite(nlink::netlink::InterfaceRef::index(ifindex), parent)
            .await
            .map_err(|e| tc_kernel_err("Failed to release the plug", &e))
    }

    /// Change the buffer ceiling without touching the epoch.
    pub async fn plug_set_limit(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
        parent: TcHandle,
        limit_bytes: u32,
    ) -> Result<(), TcguiError> {
        let conn = Self::create_connection(namespace, namespace_path)?;
        let ifindex = Self::resolve_ifindex(&conn, interface).await?;
        conn.plug_set_limit(
            nlink::netlink::InterfaceRef::index(ifindex),
            parent,
            limit_bytes,
        )
        .await
        .map_err(|e| tc_kernel_err("Failed to set the plug limit", &e))
    }

    /// Release, drain, and remove the plug.
    ///
    /// Release comes **first**, so the held packets are delivered rather than
    /// freed: deleting a qdisc resets its queue, which would drop the buffer
    /// with nothing in the logs to say so. The drain is bounded — a link that
    /// cannot absorb its own backlog must not block the RPC forever.
    pub async fn plug_remove(
        &self,
        namespace: &str,
        namespace_path: Option<&Path>,
        interface: &str,
        parent: TcHandle,
        netem_synthesized: bool,
    ) -> Result<(), TcguiError> {
        let conn = Self::create_connection(namespace, namespace_path)?;
        let ifindex = Self::resolve_ifindex(&conn, interface).await?;
        let dev = nlink::netlink::InterfaceRef::index(ifindex);

        conn.plug_release_indefinite(dev, parent)
            .await
            .map_err(|e| tc_kernel_err("Failed to release the plug before removing it", &e))?;

        // Bounded drain: ~250ms, then remove regardless.
        for _ in 0..25 {
            match Self::qdisc_layout(&conn, interface).await {
                Ok((_, _, Some((_, backlog, _)))) if backlog > 0 => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                _ => break,
            }
        }

        conn.del_qdisc_by_index_full(ifindex, parent, None)
            .await
            .map_err(|e| tc_kernel_err("Failed to remove the plug qdisc", &e))?;

        if netem_synthesized {
            // The netem existed only to host the plug; leaving it behind would
            // report a TC config on an interface the operator never configured.
            let _ = conn.del_qdisc_if_exists(interface, TcHandle::ROOT).await;
        }

        info!("Plug removed from {}/{}", namespace, interface);
        Ok(())
    }

    /// Resolve an interface name to its index on an already namespace-bound
    /// connection.
    async fn resolve_ifindex(conn: &Connection<Route>, interface: &str) -> Result<u32, TcguiError> {
        Ok(Self::resolve_link(conn, interface).await?.ifindex())
    }

    /// Resolve an interface name to its link message on an already
    /// namespace-bound connection.
    async fn resolve_link(
        conn: &Connection<Route>,
        interface: &str,
    ) -> Result<nlink::netlink::messages::LinkMessage, TcguiError> {
        conn.get_link_by_name(interface)
            .await
            .map_err(|e| TcguiError::TcCommandError {
                message: format!("Interface '{}' lookup failed: {}", interface, e),
            })?
            .ok_or_else(|| TcguiError::TcCommandError {
                message: format!("Interface '{}' not found", interface),
            })
    }
}
