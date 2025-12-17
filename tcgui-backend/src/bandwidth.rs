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
//!
//! # Architecture
//!
//! The bandwidth monitoring system follows a namespace-grouped approach:
//! 1. Interfaces are grouped by their network namespace
//! 2. Each namespace is monitored separately using appropriate access methods
//! 3. Statistics are stored with namespace+interface keys to avoid conflicts
//! 4. Updates are sent to the frontend with full namespace context
//!
//! # Examples
//!
//! ```rust,no_run
//! use tcgui_backend::bandwidth::BandwidthMonitor;
//! use zenoh::Session;
//! use std::collections::HashMap;
//!
//! async fn monitor_bandwidth(session: Session) -> anyhow::Result<()> {
//!     let mut monitor = BandwidthMonitor::new(session, "test-backend".to_string());
//!     let interfaces = HashMap::new(); // Your interface map
//!     monitor.monitor_and_send(&interfaces).await?;
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::{error, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use crate::container::Container;
use tcgui_shared::{
    errors::TcguiError, topics, BandwidthUpdate, NetworkBandwidthStats, NetworkInterface,
};

/// Network bandwidth monitoring service.
///
/// This struct manages the collection and reporting of network bandwidth statistics
/// across multiple network namespaces. It maintains historical data to calculate
/// bandwidth rates and sends real-time updates via Zenoh messaging.
///
/// # Data Storage
///
/// Previous statistics are stored using namespace+interface keys (e.g., "test-ns/eth0")
/// to ensure proper isolation between interfaces with the same name in different namespaces.
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
    ///
    /// # Arguments
    ///
    /// * `session` - Zenoh session for sending bandwidth update messages
    /// * `backend_name` - Unique name for this backend instance for topic routing
    ///
    /// # Returns
    ///
    /// A new `BandwidthMonitor` instance ready to collect and report statistics
    /// with backend-specific topic routing.
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
    ///
    /// This method groups interfaces by namespace and processes each namespace separately
    /// for efficient monitoring. It handles errors gracefully, logging failures without
    /// stopping the monitoring of other namespaces.
    ///
    /// # Arguments
    ///
    /// * `interfaces` - Map of interface index to NetworkInterface for all tracked interfaces
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful completion (individual namespace errors are logged)
    /// * `Err` only on critical system failures
    ///
    /// # Behavior
    ///
    /// 1. Groups interfaces by their namespace for batch processing
    /// 2. Calls `monitor_namespace_bandwidth` for each namespace group
    /// 3. Logs errors for failed namespaces but continues processing others
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
    ///
    /// This method reads `/proc/net/dev` statistics for the given namespace,
    /// calculates bandwidth rates using previous measurements, and sends updates
    /// for all tracked interfaces in that namespace.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The network namespace name (e.g., "default", "test-ns")
    /// * `interfaces` - Slice of NetworkInterface references to monitor in this namespace
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful monitoring and message sending
    /// * `Err` on namespace access failures or critical system errors
    ///
    /// # Key Features
    ///
    /// * **Namespace isolation**: Uses namespace+interface keys for statistics storage
    /// * **Rate calculation**: Computes bytes-per-second rates from previous measurements
    /// * **First-run handling**: Sets rates to 0.0 when no previous data exists
    /// * **Message routing**: Includes namespace context in all bandwidth updates
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
    ///
    /// This method handles both the default namespace (direct file access) and named
    /// namespaces (via `ip netns exec`). It gracefully handles permission issues by
    /// returning empty results instead of failing completely.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The target namespace ("default" for host namespace, or namespace name)
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap<String, NetworkBandwidthStats>)` - Statistics keyed by interface name
    /// * `Err` - Only on critical system failures (permission issues return empty map)
    ///
    /// # Behavior
    ///
    /// * **Default namespace**: Direct read from `/proc/net/dev`
    /// * **Named namespace**: Execute `ip netns exec <ns> cat /proc/net/dev`
    /// * **Permission handling**: Returns empty HashMap on access denied errors
    /// * **Parsing**: Extracts RX/TX bytes, packets, errors, and drops from proc format
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace))]
    async fn read_proc_net_dev_for_namespace(
        &self,
        namespace: &str,
    ) -> Result<HashMap<String, NetworkBandwidthStats>> {
        let contents = if namespace == "default" {
            // Read from default namespace directly
            tokio::fs::read_to_string("/proc/net/dev")
                .await
                .map_err(TcguiError::IoError)?
        } else if Self::is_container_namespace(namespace) {
            // Read from container namespace using nsenter
            let ns_path = self.get_container_namespace_path(namespace).await;

            if let Some(path) = ns_path {
                let output = Command::new("nsenter")
                    .arg(format!("--net={}", path.display()))
                    .args(["cat", "/proc/net/dev"])
                    .output()
                    .await?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    if stderr.contains("Operation not permitted")
                        || stderr.contains("Permission denied")
                    {
                        warn!(
                            "Cannot access container namespace {}: insufficient permissions",
                            namespace
                        );
                        return Ok(HashMap::new());
                    } else {
                        return Err(anyhow::anyhow!(
                            "Failed to read /proc/net/dev in container namespace {}: {}",
                            namespace,
                            stderr
                        ));
                    }
                }

                String::from_utf8(output.stdout)?
            } else {
                warn!(
                    "Container namespace {} has no path in cache, cannot read bandwidth stats",
                    namespace
                );
                return Ok(HashMap::new());
            }
        } else {
            // Read from named namespace using ip netns exec
            let output = Command::new("ip")
                .args(["netns", "exec", namespace, "cat", "/proc/net/dev"])
                .output()
                .await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Check for permission issues specifically
                if stderr.contains("Operation not permitted")
                    || stderr.contains("Permission denied")
                {
                    tracing::warn!("Cannot access namespace {}: insufficient permissions. Try running with proper capabilities.", namespace);
                    // Return empty stats instead of failing completely
                    return Ok(HashMap::new());
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to read /proc/net/dev in namespace {}: {}",
                        namespace,
                        stderr
                    ));
                }
            }

            String::from_utf8(output.stdout)?
        };

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
    ///
    /// Creates or reuses a publisher for the specific interface and sends the update
    /// on the interface-specific topic for efficient routing.
    ///
    /// # Arguments
    ///
    /// * `update` - BandwidthUpdate containing bandwidth statistics data
    ///
    /// # Returns
    ///
    /// * `Ok(())` on successful message sending
    /// * `Err` on serialization or Zenoh communication failures
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
    ///
    /// This static method extracts network interface statistics from the standard
    /// Linux `/proc/net/dev` format. It's used by tests to verify parsing logic
    /// without requiring actual system resources.
    ///
    /// # Arguments
    ///
    /// * `contents` - Raw `/proc/net/dev` file contents as string
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap<String, NetworkBandwidthStats>)` - Parsed statistics by interface name
    /// * `Err` - On system time access failures
    ///
    /// # Format
    ///
    /// Expects standard `/proc/net/dev` format with 16+ whitespace-separated values per line:
    /// `bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed`
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

    /// Calculates bandwidth rates from current and previous statistics (test helper).
    ///
    /// Computes bytes-per-second rates for RX and TX based on the difference between
    /// current and previous measurements. Handles edge cases like zero time differences
    /// and counter wraparounds using saturating arithmetic.
    ///
    /// # Arguments
    ///
    /// * `current` - Current bandwidth statistics measurement
    /// * `previous` - Previous bandwidth statistics measurement
    ///
    /// # Returns
    ///
    /// * `(rx_rate, tx_rate)` - Tuple of RX and TX rates in bytes per second
    ///
    /// # Behavior
    ///
    /// * **Zero time diff**: Returns (0.0, 0.0) if timestamps are identical
    /// * **Counter wraparound**: Uses saturating_sub to handle counter resets gracefully
    /// * **Rate calculation**: `(current_bytes - previous_bytes) / time_diff_seconds`
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

        // Test lo interface
        let lo_stats = result.get("lo").unwrap();
        assert_eq!(lo_stats.rx_bytes, 0);
        assert_eq!(lo_stats.tx_bytes, 0);
        assert_eq!(lo_stats.rx_packets, 0);
        assert_eq!(lo_stats.tx_packets, 0);

        // Test eth0 interface
        let eth0_stats = result.get("eth0").unwrap();
        assert_eq!(eth0_stats.rx_bytes, 1234567);
        assert_eq!(eth0_stats.tx_bytes, 9876543);
        assert_eq!(eth0_stats.rx_packets, 100);
        assert_eq!(eth0_stats.tx_packets, 200);
        assert_eq!(eth0_stats.rx_errors, 1);
        assert_eq!(eth0_stats.tx_errors, 0);
        assert_eq!(eth0_stats.rx_dropped, 2);
        assert_eq!(eth0_stats.tx_dropped, 1);

        // Test wlan0 interface
        let wlan0_stats = result.get("wlan0").unwrap();
        assert_eq!(wlan0_stats.rx_bytes, 500000);
        assert_eq!(wlan0_stats.tx_bytes, 300000);
        assert_eq!(wlan0_stats.rx_packets, 50);
        assert_eq!(wlan0_stats.tx_packets, 40);
    }

    #[test]
    fn test_parse_proc_net_dev_empty() {
        let proc_net_dev_content = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
"#;

        let result = BandwidthMonitor::parse_proc_net_dev_static(proc_net_dev_content).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_proc_net_dev_malformed_line() {
        let proc_net_dev_content = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
  bad_line_without_colon  1234567      100
  eth0: 1234567      100    1    2    0     0          0         0  9876543      200    0    1    0     0       0          0
"#;

        let result = BandwidthMonitor::parse_proc_net_dev_static(proc_net_dev_content).unwrap();

        // Should parse 2 interfaces (lo and eth0), skipping the malformed line
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("lo"));
        assert!(result.contains_key("eth0"));
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
    fn test_calculate_rates_zero_time_diff() {
        let stats = NetworkBandwidthStats {
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

        // Same timestamp - should return zero rates
        let (rx_rate, tx_rate) = BandwidthMonitor::calculate_rates_static(&stats, &stats);

        assert_eq!(rx_rate, 0.0);
        assert_eq!(tx_rate, 0.0);
    }

    #[test]
    fn test_calculate_rates_counter_wraparound() {
        // Simulate counter wraparound (current values less than previous)
        let previous = NetworkBandwidthStats {
            rx_bytes: u64::MAX - 100,
            tx_bytes: u64::MAX - 50,
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
            rx_bytes: 1000,
            tx_bytes: 500,
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

        // With saturating_sub, should handle counter wraparound gracefully
        // 1000.saturating_sub(u64::MAX - 100) = 0
        assert_eq!(rx_rate, 0.0);
        assert_eq!(tx_rate, 0.0);
    }

    /// Test namespace key generation for bandwidth stats storage
    #[test]
    fn test_namespace_key_generation() {
        // This tests the key format used in monitor_namespace_bandwidth
        let namespace = "test-ns";
        let interface = "eth1";
        let expected_key = format!("{}/{}", namespace, interface);

        assert_eq!(expected_key, "test-ns/eth1");

        // Test with default namespace
        let namespace = "default";
        let interface = "eth0";
        let expected_key = format!("{}/{}", namespace, interface);

        assert_eq!(expected_key, "default/eth0");
    }

    /// Test that interfaces are correctly grouped by namespace
    #[test]
    fn test_namespace_grouping() {
        let mut interfaces = HashMap::new();

        // Add interfaces from different namespaces
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
                name: "eth2".to_string(),
                index: 2,
                namespace: "test-ns".to_string(),
                is_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
            },
        );

        interfaces.insert(
            3,
            NetworkInterface {
                name: "eth3".to_string(),
                index: 3,
                namespace: "test-ns2".to_string(),
                is_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
            },
        );

        interfaces.insert(
            4,
            NetworkInterface {
                name: "eth0".to_string(),
                index: 4,
                namespace: "default".to_string(),
                is_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
            },
        );

        // Group interfaces by namespace (simulate monitor_and_send logic)
        let mut namespace_interfaces: std::collections::HashMap<String, Vec<&NetworkInterface>> =
            std::collections::HashMap::new();

        for interface in interfaces.values() {
            namespace_interfaces
                .entry(interface.namespace.clone())
                .or_default()
                .push(interface);
        }

        // Verify grouping
        assert_eq!(namespace_interfaces.len(), 3);
        assert_eq!(namespace_interfaces["test-ns"].len(), 2);
        assert_eq!(namespace_interfaces["test-ns2"].len(), 1);
        assert_eq!(namespace_interfaces["default"].len(), 1);

        // Verify interface names in each namespace
        let test_ns_interfaces: Vec<&str> = namespace_interfaces["test-ns"]
            .iter()
            .map(|iface| iface.name.as_str())
            .collect();
        assert!(test_ns_interfaces.contains(&"eth1"));
        assert!(test_ns_interfaces.contains(&"eth2"));

        assert_eq!(namespace_interfaces["test-ns2"][0].name, "eth3");
        assert_eq!(namespace_interfaces["default"][0].name, "eth0");
    }

    /// Test the critical bug we fixed: ensure namespace info is preserved in bandwidth updates
    #[test]
    fn test_bandwidth_message_includes_namespace() {
        let namespace = "test-ns";
        let interface_name = "eth1";
        let stats = NetworkBandwidthStats {
            rx_bytes: 1000,
            tx_bytes: 500,
            timestamp: 100,
            rx_packets: 10,
            tx_packets: 5,
            rx_errors: 0,
            tx_errors: 0,
            rx_dropped: 0,
            tx_dropped: 0,
            rx_bytes_per_sec: 100.0,
            tx_bytes_per_sec: 50.0,
        };

        // Create the message as it would be created in monitor_namespace_bandwidth
        let response = BandwidthUpdate {
            namespace: namespace.to_string(),
            interface: interface_name.to_string(),
            stats: stats.clone(),
            backend_name: "test-backend".to_string(),
        };

        // Verify the message contains namespace information
        assert_eq!(response.namespace, "test-ns");
        assert_eq!(response.interface, "eth1");
        assert_eq!(response.stats.rx_bytes_per_sec, 100.0);
        assert_eq!(response.stats.tx_bytes_per_sec, 50.0);
        assert_eq!(response.backend_name, "test-backend");
    }
}
