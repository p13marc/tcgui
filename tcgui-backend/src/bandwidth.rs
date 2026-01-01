//! Network bandwidth monitoring and statistics collection.
//!
//! This module provides comprehensive network bandwidth monitoring across multiple network namespaces.
//! It collects real-time statistics from `/proc/net/dev` and calculates bandwidth rates for all
//! tracked network interfaces.
//!
//! # Key Features
//!
//! * **Multi-namespace support**: Monitors interfaces across all network namespaces
//! * **Real-time statistics**: Collects RX/TX bytes, packets, errors, and drops
//! * **Rate calculations**: Computes bytes-per-second rates with proper counter wraparound handling
//! * **Namespace-aware messaging**: Sends updates with namespace context for proper routing
//! * **Permission handling**: Gracefully handles namespace access permission issues

use anyhow::Result;
use nlink::netlink::namespace;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use crate::container::Container;
use tcgui_shared::{
    BandwidthUpdate, NetworkBandwidthStats, NetworkInterface, errors::TcguiError, topics,
};

/// Network bandwidth monitoring service.
pub struct BandwidthMonitor {
    /// Zenoh session for sending bandwidth update messages
    session: Session,
    /// Previous bandwidth statistics keyed by "namespace/interface" for rate calculations
    previous_stats: HashMap<String, NetworkBandwidthStats>,
    /// Backend name for topic routing in multi-backend scenarios
    backend_name: String,
    /// Publishers for bandwidth updates (one per interface)
    bandwidth_publishers: HashMap<String, AdvancedPublisher<'static>>,
    /// Shared container cache for resolving container namespace paths
    container_cache: Option<Arc<RwLock<HashMap<String, Container>>>>,
}

impl BandwidthMonitor {
    /// Creates a new bandwidth monitor instance.
    pub fn new(session: Session, backend_name: String) -> Self {
        Self {
            session,
            previous_stats: HashMap::new(),
            backend_name,
            bandwidth_publishers: HashMap::new(),
            container_cache: None,
        }
    }

    /// Sets the container cache for resolving container namespace paths
    pub fn set_container_cache(&mut self, cache: Arc<RwLock<HashMap<String, Container>>>) {
        self.container_cache = Some(cache);
    }

    /// Check if a namespace is a container namespace
    fn is_container_namespace(namespace: &str) -> bool {
        namespace.starts_with("container:")
    }

    /// Get the namespace path for a container namespace from the cache
    async fn get_container_namespace_path(&self, namespace: &str) -> Option<PathBuf> {
        if let Some(cache) = &self.container_cache {
            let cache_guard = cache.read().await;
            if let Some(container) = cache_guard.get(namespace) {
                return container.namespace_path.clone();
            }
        }
        None
    }

    /// Monitors bandwidth for all provided interfaces and sends updates.
    #[instrument(skip(self, interfaces), fields(backend_name = %self.backend_name, interface_count = interfaces.len()))]
    pub async fn monitor_and_send(
        &mut self,
        interfaces: &HashMap<u32, NetworkInterface>,
    ) -> Result<()> {
        // Group interfaces by namespace for efficient processing
        let mut namespace_interfaces: std::collections::HashMap<String, Vec<&NetworkInterface>> =
            std::collections::HashMap::new();

        for interface in interfaces.values() {
            namespace_interfaces
                .entry(interface.namespace.clone())
                .or_default()
                .push(interface);
        }

        // Monitor each namespace separately
        for (namespace, ns_interfaces) in namespace_interfaces {
            if let Err(e) = self
                .monitor_namespace_bandwidth(&namespace, &ns_interfaces)
                .await
            {
                error!(
                    "Failed to monitor bandwidth for namespace {}: {}",
                    namespace, e
                );
            }
        }

        Ok(())
    }

    /// Monitors bandwidth statistics for interfaces within a specific namespace.
    #[instrument(skip(self, interfaces), fields(backend_name = %self.backend_name, namespace, interface_count = interfaces.len()))]
    pub async fn monitor_namespace_bandwidth(
        &mut self,
        namespace: &str,
        interfaces: &[&NetworkInterface],
    ) -> Result<()> {
        tracing::debug!("Reading bandwidth stats for namespace: {}", namespace);
        let current_stats = self.read_proc_net_dev_for_namespace(namespace).await?;
        tracing::debug!(
            "Found {} interface stats in namespace {}",
            current_stats.len(),
            namespace
        );

        for (interface_name, mut stats) in current_stats {
            // Only send stats for interfaces we're tracking in this namespace
            if let Some(tracked_interface) =
                interfaces.iter().find(|iface| iface.name == interface_name)
            {
                // Create a namespace-prefixed key for storing previous stats
                let stats_key = format!("{}/{}", namespace, interface_name);
                tracing::debug!(
                    "Processing bandwidth stats for {}: RX {} bytes, TX {} bytes",
                    stats_key,
                    stats.rx_bytes,
                    stats.tx_bytes
                );

                // Calculate rates if we have previous data
                if let Some(prev_stats) = self.previous_stats.get(&stats_key) {
                    let time_diff = stats.timestamp.saturating_sub(prev_stats.timestamp) as f64;

                    if time_diff > 0.0 {
                        let rx_bytes_diff =
                            stats.rx_bytes.saturating_sub(prev_stats.rx_bytes) as f64;
                        let tx_bytes_diff =
                            stats.tx_bytes.saturating_sub(prev_stats.tx_bytes) as f64;

                        stats.rx_bytes_per_sec = rx_bytes_diff / time_diff;
                        stats.tx_bytes_per_sec = tx_bytes_diff / time_diff;
                    } else {
                        stats.rx_bytes_per_sec = 0.0;
                        stats.tx_bytes_per_sec = 0.0;
                    }
                } else {
                    // First measurement - no rate data available
                    stats.rx_bytes_per_sec = 0.0;
                    stats.tx_bytes_per_sec = 0.0;
                }

                // Store current stats for next calculation with namespace-prefixed key
                self.previous_stats.insert(stats_key, stats.clone());

                let bandwidth_update = BandwidthUpdate {
                    namespace: tracked_interface.namespace.clone(),
                    interface: tracked_interface.name.clone(),
                    stats: stats.clone(),
                    backend_name: self.backend_name.clone(),
                };
                tracing::debug!(
                    "Sending bandwidth update for {}/{}: RX rate {:.2} B/s, TX rate {:.2} B/s",
                    namespace,
                    interface_name,
                    stats.rx_bytes_per_sec,
                    stats.tx_bytes_per_sec
                );
                self.send_bandwidth_update(bandwidth_update).await?;
            }
        }

        Ok(())
    }

    /// Reads network interface statistics from `/proc/net/dev` for a specific namespace.
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace))]
    async fn read_proc_net_dev_for_namespace(
        &self,
        namespace: &str,
    ) -> Result<HashMap<String, NetworkBandwidthStats>> {
        let contents: String = if namespace == "default" {
            // Default namespace: read directly
            match tokio::fs::read_to_string("/proc/net/dev").await {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to read /proc/net/dev: {}", e);
                    return Ok(HashMap::new());
                }
            }
        } else if Self::is_container_namespace(namespace) {
            // Container namespace: use the cached path
            match self.get_container_namespace_path(namespace).await {
                Some(path) => match self.read_proc_net_dev_in_namespace_path(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(
                            "Failed to read /proc/net/dev in container namespace {}: {}",
                            namespace, e
                        );
                        return Ok(HashMap::new());
                    }
                },
                None => {
                    warn!(
                        "Container namespace {} has no path in cache, cannot read bandwidth stats",
                        namespace
                    );
                    return Ok(HashMap::new());
                }
            }
        } else {
            // Traditional named namespace
            match self.read_proc_net_dev_in_named_namespace(namespace).await {
                Ok(c) => c,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("EPERM")
                        || err_str.contains("Operation not permitted")
                        || err_str.contains("Permission denied")
                    {
                        warn!(
                            "Cannot access namespace {}: insufficient permissions",
                            namespace
                        );
                        return Ok(HashMap::new());
                    }
                    warn!(
                        "Failed to read /proc/net/dev in namespace {}: {}",
                        namespace, e
                    );
                    return Ok(HashMap::new());
                }
            }
        };

        debug!(
            "Read {} bytes from /proc/net/dev in namespace {}",
            contents.len(),
            namespace
        );

        self.parse_proc_net_dev(&contents)
    }

    /// Read /proc/net/dev in a named namespace using nlink's namespace utilities
    async fn read_proc_net_dev_in_named_namespace(&self, namespace: &str) -> Result<String> {
        let ns_name = namespace.to_string();

        // Use spawn_blocking to run the namespace operation in a separate thread
        tokio::task::spawn_blocking(move || {
            // Enter the namespace
            let _guard = namespace::enter(&ns_name)
                .map_err(|e| anyhow::anyhow!("Failed to enter namespace {}: {}", ns_name, e))?;

            // Read /proc/net/dev while in the namespace
            std::fs::read_to_string("/proc/net/dev")
                .map_err(|e| anyhow::anyhow!("Failed to read /proc/net/dev: {}", e))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    /// Read /proc/net/dev in a namespace by path (for containers)
    async fn read_proc_net_dev_in_namespace_path(&self, ns_path: &Path) -> Result<String> {
        let path = ns_path.to_path_buf();

        // Use spawn_blocking to run the namespace operation in a separate thread
        tokio::task::spawn_blocking(move || {
            // Enter the namespace by path
            let _guard = namespace::enter_path(&path)
                .map_err(|e| anyhow::anyhow!("Failed to enter namespace {:?}: {}", path, e))?;

            // Read /proc/net/dev while in the namespace
            std::fs::read_to_string("/proc/net/dev")
                .map_err(|e| anyhow::anyhow!("Failed to read /proc/net/dev: {}", e))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    /// Parse /proc/net/dev contents into bandwidth statistics
    fn parse_proc_net_dev(&self, contents: &str) -> Result<HashMap<String, NetworkBandwidthStats>> {
        let mut stats = HashMap::new();
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // Skip first two header lines
        for line in contents.lines().skip(2) {
            if let Some((interface_part, stats_part)) = line.split_once(':') {
                let interface_name = interface_part.trim().to_string();
                let stats_values: Vec<&str> = stats_part.split_whitespace().collect();

                // /proc/net/dev format:
                // bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed
                if stats_values.len() >= 16 {
                    let bandwidth_stats = NetworkBandwidthStats {
                        rx_bytes: stats_values[0].parse().unwrap_or(0),
                        rx_packets: stats_values[1].parse().unwrap_or(0),
                        rx_errors: stats_values[2].parse().unwrap_or(0),
                        rx_dropped: stats_values[3].parse().unwrap_or(0),
                        tx_bytes: stats_values[8].parse().unwrap_or(0),
                        tx_packets: stats_values[9].parse().unwrap_or(0),
                        tx_errors: stats_values[10].parse().unwrap_or(0),
                        tx_dropped: stats_values[11].parse().unwrap_or(0),
                        timestamp,
                        rx_bytes_per_sec: 0.0, // Will be calculated later
                        tx_bytes_per_sec: 0.0, // Will be calculated later
                    };

                    stats.insert(interface_name, bandwidth_stats);
                }
            }
        }

        Ok(stats)
    }

    /// Sends a bandwidth update message via Zenoh to the frontend.
    #[instrument(skip(self, update), fields(backend_name = %self.backend_name, namespace = %update.namespace, interface = %update.interface))]
    async fn send_bandwidth_update(&mut self, update: BandwidthUpdate) -> Result<()> {
        let payload = serde_json::to_string(&update).map_err(TcguiError::SerializationError)?;

        // Create publisher key for this specific interface
        let publisher_key = format!("{}/{}", update.namespace, update.interface);

        // Get or create publisher for this interface
        if !self.bandwidth_publishers.contains_key(&publisher_key) {
            let bandwidth_topic =
                topics::bandwidth_updates(&self.backend_name, &update.namespace, &update.interface);
            tracing::debug!(
                "Creating bandwidth publisher for {}: {}",
                publisher_key,
                bandwidth_topic.as_str()
            );

            let publisher = self
                .session
                .declare_publisher(bandwidth_topic)
                .cache(CacheConfig::default().max_samples(1))
                .sample_miss_detection(
                    MissDetectionConfig::default().heartbeat(Duration::from_millis(500)),
                )
                .publisher_detection()
                .await
                .map_err(|e| TcguiError::ZenohError {
                    message: format!("Failed to declare bandwidth advanced publisher: {}", e),
                })?;

            self.bandwidth_publishers
                .insert(publisher_key.clone(), publisher);
        }

        if let Some(publisher) = self.bandwidth_publishers.get(&publisher_key) {
            publisher
                .put(payload)
                .await
                .map_err(|e| TcguiError::ZenohError {
                    message: format!("Failed to send bandwidth update: {}", e),
                })?;

            tracing::debug!(
                "Sent bandwidth update for {}: RX rate {:.2} B/s, TX rate {:.2} B/s",
                publisher_key,
                update.stats.rx_bytes_per_sec,
                update.stats.tx_bytes_per_sec
            );
        }

        Ok(())
    }

    /// Parses `/proc/net/dev` file contents into bandwidth statistics (test helper).
    #[cfg(test)]
    pub fn parse_proc_net_dev_static(
        contents: &str,
    ) -> Result<HashMap<String, NetworkBandwidthStats>> {
        let mut stats = HashMap::new();
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        // Skip first two header lines
        for line in contents.lines().skip(2) {
            if let Some((interface_part, stats_part)) = line.split_once(':') {
                let interface_name = interface_part.trim().to_string();
                let stats_values: Vec<&str> = stats_part.split_whitespace().collect();

                if stats_values.len() >= 16 {
                    let bandwidth_stats = NetworkBandwidthStats {
                        rx_bytes: stats_values[0].parse().unwrap_or(0),
                        rx_packets: stats_values[1].parse().unwrap_or(0),
                        rx_errors: stats_values[2].parse().unwrap_or(0),
                        rx_dropped: stats_values[3].parse().unwrap_or(0),
                        tx_bytes: stats_values[8].parse().unwrap_or(0),
                        tx_packets: stats_values[9].parse().unwrap_or(0),
                        tx_errors: stats_values[10].parse().unwrap_or(0),
                        tx_dropped: stats_values[11].parse().unwrap_or(0),
                        timestamp,
                        rx_bytes_per_sec: 0.0,
                        tx_bytes_per_sec: 0.0,
                    };

                    stats.insert(interface_name, bandwidth_stats);
                }
            }
        }

        Ok(stats)
    }

    /// Calculates bandwidth rates from current and previous statistics (test helper).
    #[cfg(test)]
    pub fn calculate_rates_static(
        current: &NetworkBandwidthStats,
        previous: &NetworkBandwidthStats,
    ) -> (f64, f64) {
        let time_diff = current.timestamp.saturating_sub(previous.timestamp) as f64;

        if time_diff > 0.0 {
            let rx_bytes_diff = current.rx_bytes.saturating_sub(previous.rx_bytes) as f64;
            let tx_bytes_diff = current.tx_bytes.saturating_sub(previous.tx_bytes) as f64;

            (rx_bytes_diff / time_diff, tx_bytes_diff / time_diff)
        } else {
            (0.0, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::{InterfaceType, NetworkInterface};

    #[test]
    fn test_parse_proc_net_dev() {
        let proc_net_dev_content = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
  eth0: 1234567      100    1    2    0     0          0         0  9876543      200    0    1    0     0       0          0
  wlan0:  500000       50    0    0    0     0          0         0   300000       40    0    0    0     0       0          0
"#;

        let result = BandwidthMonitor::parse_proc_net_dev_static(proc_net_dev_content).unwrap();

        assert_eq!(result.len(), 3);

        // Test eth0 interface
        let eth0_stats = result.get("eth0").unwrap();
        assert_eq!(eth0_stats.rx_bytes, 1234567);
        assert_eq!(eth0_stats.tx_bytes, 9876543);
    }

    #[test]
    fn test_calculate_rates() {
        let previous = NetworkBandwidthStats {
            rx_bytes: 1000,
            tx_bytes: 500,
            timestamp: 100,
            rx_packets: 10,
            tx_packets: 5,
            rx_errors: 0,
            tx_errors: 0,
            rx_dropped: 0,
            tx_dropped: 0,
            rx_bytes_per_sec: 0.0,
            tx_bytes_per_sec: 0.0,
        };

        let current = NetworkBandwidthStats {
            rx_bytes: 3000,
            tx_bytes: 1500,
            timestamp: 102, // 2 seconds later
            rx_packets: 30,
            tx_packets: 15,
            rx_errors: 0,
            tx_errors: 0,
            rx_dropped: 0,
            tx_dropped: 0,
            rx_bytes_per_sec: 0.0,
            tx_bytes_per_sec: 0.0,
        };

        let (rx_rate, tx_rate) = BandwidthMonitor::calculate_rates_static(&current, &previous);

        // (3000 - 1000) / (102 - 100) = 2000 / 2 = 1000 B/s
        assert_eq!(rx_rate, 1000.0);
        // (1500 - 500) / (102 - 100) = 1000 / 2 = 500 B/s
        assert_eq!(tx_rate, 500.0);
    }

    #[test]
    fn test_namespace_grouping() {
        let mut interfaces = HashMap::new();

        interfaces.insert(
            1,
            NetworkInterface {
                name: "eth1".to_string(),
                index: 1,
                namespace: "test-ns".to_string(),
                is_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
            },
        );

        interfaces.insert(
            2,
            NetworkInterface {
                name: "eth0".to_string(),
                index: 2,
                namespace: "default".to_string(),
                is_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
            },
        );

        // Group interfaces by namespace
        let mut namespace_interfaces: std::collections::HashMap<String, Vec<&NetworkInterface>> =
            std::collections::HashMap::new();

        for interface in interfaces.values() {
            namespace_interfaces
                .entry(interface.namespace.clone())
                .or_default()
                .push(interface);
        }

        assert_eq!(namespace_interfaces.len(), 2);
        assert_eq!(namespace_interfaces["test-ns"].len(), 1);
        assert_eq!(namespace_interfaces["default"].len(), 1);
    }
}
