//! Native netlink-based Traffic Control (TC) operations.
//!
//! This module provides TC qdisc management using the rtnetlink crate,
//! eliminating the need to spawn external `tc` command processes.
//!
//! # Features
//!
//! * **Native netlink communication**: Direct kernel communication via netlink sockets
//! * **Namespace support**: Works with both traditional and container namespaces
//! * **netem qdisc**: Full support for network emulation (delay, loss, jitter, etc.)
//!
//! # Example
//!
//! ```rust,no_run
//! use tcgui_backend::tc_netlink::{TcNetlink, NetemConfig};
//! use tcgui_backend::netns::NamespacePath;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let tc = TcNetlink::new();
//!
//! // Apply netem with 100ms delay and 5% loss
//! let config = NetemConfig {
//!     delay_ms: Some(100.0),
//!     loss_percent: Some(5.0),
//!     ..Default::default()
//! };
//!
//! tc.apply_netem(NamespacePath::Default, "eth0", &config).await?;
//! # Ok(())
//! # }
//! ```

use futures_util::stream::TryStreamExt;
use rtnetlink::Handle;
use rtnetlink::packet_route::tc::TcHandle;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

use crate::netns::{NamespacePath, run_in_namespace};

/// Errors specific to TC netlink operations.
#[derive(Error, Debug)]
pub enum TcNetlinkError {
    /// Interface not found
    #[error("Interface '{0}' not found")]
    InterfaceNotFound(String),

    /// Failed to apply qdisc
    #[error("Failed to apply qdisc: {0}")]
    QdiscApplyFailed(String),

    /// Failed to delete qdisc
    #[error("Failed to delete qdisc: {0}")]
    QdiscDeleteFailed(String),

    /// Failed to query qdisc
    #[error("Failed to query qdisc: {0}")]
    QdiscQueryFailed(String),

    /// Namespace operation failed
    #[error("Namespace operation failed: {0}")]
    NamespaceFailed(#[from] crate::netns::NamespaceError),
}

/// Configuration for netem qdisc.
#[derive(Debug, Clone, Default)]
pub struct NetemConfig {
    /// Base delay in milliseconds
    pub delay_ms: Option<f32>,
    /// Delay jitter in milliseconds
    pub jitter_ms: Option<f32>,
    /// Delay correlation percentage (0-100)
    pub delay_correlation: Option<f32>,
    /// Packet loss percentage (0-100)
    pub loss_percent: Option<f32>,
    /// Loss correlation percentage (0-100)
    pub loss_correlation: Option<f32>,
    /// Packet duplication percentage (0-100)
    pub duplicate_percent: Option<f32>,
    /// Duplication correlation percentage (0-100)
    pub duplicate_correlation: Option<f32>,
    /// Packet reordering percentage (0-100)
    pub reorder_percent: Option<f32>,
    /// Reorder correlation percentage (0-100)
    pub reorder_correlation: Option<f32>,
    /// Reorder gap
    pub reorder_gap: Option<u32>,
    /// Packet corruption percentage (0-100)
    pub corrupt_percent: Option<f32>,
    /// Corruption correlation percentage (0-100)
    pub corrupt_correlation: Option<f32>,
    /// Rate limit in kbps
    pub rate_limit_kbps: Option<u32>,
    /// Queue limit in packets
    pub limit: Option<u32>,
}

/// Native netlink-based TC manager.
#[derive(Clone, Default)]
pub struct TcNetlink;

impl TcNetlink {
    /// Create a new TcNetlink instance.
    pub fn new() -> Self {
        Self
    }

    /// Get the interface index for an interface name.
    async fn get_interface_index(handle: &Handle, interface: &str) -> Result<i32, TcNetlinkError> {
        let mut links = handle
            .link()
            .get()
            .match_name(interface.to_string())
            .execute();

        if let Some(link) = links
            .try_next()
            .await
            .map_err(|e| TcNetlinkError::InterfaceNotFound(format!("{}: {}", interface, e)))?
        {
            Ok(link.header.index as i32)
        } else {
            Err(TcNetlinkError::InterfaceNotFound(interface.to_string()))
        }
    }

    /// Apply netem qdisc configuration to an interface.
    #[instrument(skip(self, config), fields(namespace = ?namespace, interface = %interface))]
    pub async fn apply_netem(
        &self,
        namespace: NamespacePath,
        interface: &str,
        config: &NetemConfig,
    ) -> Result<String, TcNetlinkError> {
        let interface = interface.to_string();
        let config = config.clone();

        info!(
            "Applying netem via netlink: interface={}, delay={:?}ms, loss={:?}%",
            interface, config.delay_ms, config.loss_percent
        );

        run_in_namespace(namespace, move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to create runtime: {}", e))?;

            rt.block_on(async { Self::apply_netem_inner(&interface, &config).await })
        })
        .await?
        .map_err(|e: String| TcNetlinkError::QdiscApplyFailed(e))
    }

    /// Inner async implementation of apply_netem.
    async fn apply_netem_inner(interface: &str, config: &NetemConfig) -> Result<String, String> {
        let (connection, handle, _) =
            rtnetlink::new_connection().map_err(|e| format!("Netlink connection failed: {}", e))?;

        tokio::spawn(connection);

        let if_index = Self::get_interface_index(&handle, interface)
            .await
            .map_err(|e| e.to_string())?;

        debug!("Interface {} has index {}", interface, if_index);

        // Delete any existing root qdisc first
        let mut del_req = handle.qdisc().del(if_index);
        del_req.message_mut().header.parent = TcHandle::ROOT;

        match del_req.execute().await {
            Ok(_) => debug!("Deleted existing root qdisc on {}", interface),
            Err(e) => {
                let err_str = e.to_string();
                if !err_str.contains("No such file") && !err_str.contains("ENOENT") {
                    debug!("No existing qdisc to delete (or error): {}", e);
                }
            }
        }

        // Build and apply netem qdisc
        let mut builder = handle.qdisc().add(if_index).netem();

        // Apply delay
        if let Some(delay) = config.delay_ms {
            if delay > 0.0 {
                builder = builder.delay_ms(delay as u32);

                if let Some(jitter) = config.jitter_ms {
                    if jitter > 0.0 {
                        builder = builder.jitter_ms(jitter as u32);
                    }
                }

                if let Some(corr) = config.delay_correlation {
                    if corr > 0.0 {
                        builder = builder.delay_correlation(corr);
                    }
                }
            }
        }

        // Apply loss
        if let Some(loss) = config.loss_percent {
            if loss > 0.0 {
                builder = builder.loss_percent(loss);

                if let Some(corr) = config.loss_correlation {
                    if corr > 0.0 {
                        builder = builder.loss_correlation(corr);
                    }
                }
            }
        }

        // Apply duplication
        if let Some(dup) = config.duplicate_percent {
            if dup > 0.0 {
                builder = builder.duplicate_percent(dup);

                if let Some(corr) = config.duplicate_correlation {
                    if corr > 0.0 {
                        builder = builder.duplicate_correlation(corr);
                    }
                }
            }
        }

        // Apply reordering
        if let Some(reorder) = config.reorder_percent {
            if reorder > 0.0 {
                builder = builder.reorder_percent(reorder);

                if let Some(corr) = config.reorder_correlation {
                    if corr > 0.0 {
                        builder = builder.reorder_correlation(corr);
                    }
                }

                if let Some(gap) = config.reorder_gap {
                    if gap > 0 {
                        builder = builder.gap(gap);
                    }
                }
            }
        }

        // Apply corruption
        if let Some(corrupt) = config.corrupt_percent {
            if corrupt > 0.0 {
                builder = builder.corrupt_percent(corrupt);

                if let Some(corr) = config.corrupt_correlation {
                    if corr > 0.0 {
                        builder = builder.corrupt_correlation(corr);
                    }
                }
            }
        }

        // Apply rate limit
        if let Some(rate) = config.rate_limit_kbps {
            if rate > 0 {
                builder = builder.rate_kbit(rate);
            }
        }

        // Apply queue limit
        if let Some(limit) = config.limit {
            builder = builder.limit(limit);
        }

        builder
            .build()
            .execute()
            .await
            .map_err(|e| format!("Failed to add netem qdisc: {}", e))?;

        info!("Successfully applied netem qdisc to {}", interface);
        Ok(format!("netem qdisc applied to {}", interface))
    }

    /// Remove qdisc from an interface.
    #[instrument(skip(self), fields(namespace = ?namespace, interface = %interface))]
    pub async fn remove_qdisc(
        &self,
        namespace: NamespacePath,
        interface: &str,
    ) -> Result<String, TcNetlinkError> {
        let interface = interface.to_string();

        info!("Removing qdisc via netlink: interface={}", interface);

        run_in_namespace(namespace, move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to create runtime: {}", e))?;

            rt.block_on(async { Self::remove_qdisc_inner(&interface).await })
        })
        .await?
        .map_err(|e: String| TcNetlinkError::QdiscDeleteFailed(e))
    }

    /// Inner async implementation of remove_qdisc.
    async fn remove_qdisc_inner(interface: &str) -> Result<String, String> {
        let (connection, handle, _) =
            rtnetlink::new_connection().map_err(|e| format!("Netlink connection failed: {}", e))?;

        tokio::spawn(connection);

        let if_index = Self::get_interface_index(&handle, interface)
            .await
            .map_err(|e| e.to_string())?;

        let mut del_req = handle.qdisc().del(if_index);
        del_req.message_mut().header.parent = TcHandle::ROOT;

        match del_req.execute().await {
            Ok(_) => {
                info!("Successfully removed qdisc from {}", interface);
                Ok(format!("qdisc removed from {}", interface))
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("No such file") || err_str.contains("ENOENT") {
                    Ok("No qdisc to remove".to_string())
                } else {
                    Err(format!("Failed to delete qdisc: {}", e))
                }
            }
        }
    }

    /// Check if an interface has a qdisc configured.
    #[instrument(skip(self), fields(namespace = ?namespace, interface = %interface))]
    pub async fn check_qdisc(
        &self,
        namespace: NamespacePath,
        interface: &str,
    ) -> Result<Option<String>, TcNetlinkError> {
        let interface = interface.to_string();

        run_in_namespace(namespace, move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to create runtime: {}", e))?;

            rt.block_on(async { Self::check_qdisc_inner(&interface).await })
        })
        .await?
        .map_err(|e: String| TcNetlinkError::QdiscQueryFailed(e))
    }

    /// Inner async implementation of check_qdisc.
    async fn check_qdisc_inner(interface: &str) -> Result<Option<String>, String> {
        let (connection, handle, _) =
            rtnetlink::new_connection().map_err(|e| format!("Netlink connection failed: {}", e))?;

        tokio::spawn(connection);

        let if_index = Self::get_interface_index(&handle, interface)
            .await
            .map_err(|e| e.to_string())?;

        let mut qdiscs = handle.qdisc().get().index(if_index).execute();

        while let Some(qdisc) = qdiscs.try_next().await.map_err(|e| e.to_string())? {
            if qdisc.header.parent == rtnetlink::packet_route::tc::TcHandle::ROOT {
                for attr in &qdisc.attributes {
                    if let rtnetlink::packet_route::tc::TcAttribute::Kind(kind) = attr {
                        return Ok(Some(kind.clone()));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Apply netem configuration with namespace path support (for containers).
    #[instrument(skip(self, config), fields(namespace = %namespace, interface = %interface))]
    pub async fn apply_netem_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&std::path::Path>,
        interface: &str,
        config: &NetemConfig,
    ) -> Result<String, TcNetlinkError> {
        let ns = Self::resolve_namespace(namespace, namespace_path);
        self.apply_netem(ns, interface, config).await
    }

    /// Remove qdisc with namespace path support (for containers).
    #[instrument(skip(self), fields(namespace = %namespace, interface = %interface))]
    pub async fn remove_qdisc_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&std::path::Path>,
        interface: &str,
    ) -> Result<String, TcNetlinkError> {
        let ns = Self::resolve_namespace(namespace, namespace_path);
        self.remove_qdisc(ns, interface).await
    }

    /// Check qdisc with namespace path support (for containers).
    #[instrument(skip(self), fields(namespace = %namespace, interface = %interface))]
    pub async fn check_qdisc_with_path(
        &self,
        namespace: &str,
        namespace_path: Option<&std::path::Path>,
        interface: &str,
    ) -> Result<Option<String>, TcNetlinkError> {
        let ns = Self::resolve_namespace(namespace, namespace_path);
        self.check_qdisc(ns, interface).await
    }

    /// Helper to resolve namespace specification from string and optional path.
    fn resolve_namespace(
        namespace: &str,
        namespace_path: Option<&std::path::Path>,
    ) -> NamespacePath {
        if namespace == "default" {
            NamespacePath::Default
        } else if namespace.starts_with("container:") {
            if let Some(path) = namespace_path {
                NamespacePath::Path(path.to_path_buf())
            } else {
                warn!(
                    "Container namespace {} without path, falling back to named",
                    namespace
                );
                NamespacePath::Named(namespace.to_string())
            }
        } else {
            NamespacePath::Named(namespace.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_netem_config_default() {
        let config = NetemConfig::default();
        assert!(config.delay_ms.is_none());
        assert!(config.loss_percent.is_none());
    }

    #[test]
    fn test_netem_config_with_effects() {
        let config = NetemConfig {
            delay_ms: Some(100.0),
            loss_percent: Some(5.0),
            ..Default::default()
        };
        assert_eq!(config.delay_ms, Some(100.0));
        assert_eq!(config.loss_percent, Some(5.0));
    }

    #[test]
    fn test_resolve_namespace_default() {
        let ns = TcNetlink::resolve_namespace("default", None);
        assert!(matches!(ns, NamespacePath::Default));
    }

    #[test]
    fn test_resolve_namespace_named() {
        let ns = TcNetlink::resolve_namespace("my-ns", None);
        assert!(matches!(ns, NamespacePath::Named(name) if name == "my-ns"));
    }

    #[test]
    fn test_resolve_namespace_container_with_path() {
        let path = PathBuf::from("/proc/12345/ns/net");
        let ns = TcNetlink::resolve_namespace("container:test", Some(&path));
        assert!(matches!(ns, NamespacePath::Path(p) if p == path));
    }

    #[tokio::test]
    async fn test_check_qdisc_default_namespace() {
        // Skip if not root
        if std::fs::metadata("/proc/1/root").is_err() {
            eprintln!("Skipping test_check_qdisc_default_namespace: requires root");
            return;
        }

        let tc = TcNetlink::new();
        let result = tc.check_qdisc(NamespacePath::Default, "lo").await;
        assert!(result.is_ok());
    }
}
