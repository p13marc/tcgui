//! Network bandwidth monitoring and statistics collection.
//!
//! This module provides comprehensive network bandwidth monitoring across multiple network namespaces.
//! It collects real-time statistics via netlink and calculates bandwidth rates for all
//! tracked network interfaces.
//!
//! # Key Features
//!
//! * **Multi-namespace support**: Monitors interfaces across all network namespaces
//! * **Real-time statistics**: Collects RX/TX bytes, packets, errors, and drops via netlink
//! * **Rate calculations**: Uses nlink's StatsTracker for automatic rate calculation
//! * **Namespace-aware messaging**: Sends updates with namespace context for proper routing
//! * **Permission handling**: Gracefully handles namespace access permission issues

use anyhow::Result;
use nlink::netlink::messages::LinkMessage;
use nlink::netlink::stats::{LinkStats as NlinkLinkStats, StatsSnapshot, StatsTracker};
use nlink::netlink::{Connection, Route};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, error, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use crate::container::Container;
use tcgui_shared::registry::tc;
use tcgui_shared::{
    BandwidthUpdate, NetworkBandwidthStats, NetworkInterface, errors::TcguiError,
    identity::LocalOrigin,
};

/// Returns true if the error chain contains an nlink permission-denied error.
///
/// Walks the `anyhow` source chain looking for a typed `nlink::netlink::Error`
/// and asks it via `is_permission_denied()` (which, as of nlink 0.19, matches
/// both the `Error::Kernel(EPERM/EACCES)` and `Error::Io(EPERM)` shapes) rather
/// than matching on the Display string.
fn is_permission_denied(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|cause| cause.downcast_ref::<nlink::netlink::Error>())
        .any(|e| e.is_permission_denied())
}

/// A cached netlink connection bound to a specific network namespace.
///
/// Caching avoids rebuilding the connection (an `open(netns)` + `setns` dance
/// that, since nlink 0.19's N1 fix, runs on a freshly spawned thread) on every
/// poll cycle. Holding the connection across polls is only safe because that
/// N1 fix stops namespace membership from bleeding into tokio worker threads.
struct CachedConnection {
    /// The live connection.
    conn: Connection<Route>,
    /// Namespace path this connection is bound to (`None` = default netns).
    /// Compared on every poll so a container netns change (the `/proc/<pid>/`
    /// path changes when a container restarts) forces a rebuild.
    path: Option<PathBuf>,
}

/// Per-namespace statistics tracker
struct NamespaceStatsTracker {
    /// nlink StatsTracker for automatic rate calculation
    tracker: StatsTracker,
    /// Last known rates by interface index
    last_rates: HashMap<u32, (f64, f64)>, // (rx_bps, tx_bps)
}

impl NamespaceStatsTracker {
    fn new() -> Self {
        Self {
            tracker: StatsTracker::new(),
            last_rates: HashMap::new(),
        }
    }
}

/// Network bandwidth monitoring service.
pub struct BandwidthMonitor {
    /// Zenoh session for sending bandwidth update messages
    session: Session,
    /// This host's origin — every telemetry key is built from it.
    local_origin: LocalOrigin,
    /// Operator-chosen display label, carried in the payload and used for logging.
    backend_name: String,
    /// Publishers for bandwidth updates (one per interface)
    bandwidth_publishers: HashMap<String, AdvancedPublisher<'static>>,
    /// Shared container cache for resolving container namespace paths
    container_cache: Option<Arc<RwLock<HashMap<String, Container>>>>,
    /// Per-namespace stats trackers using nlink's StatsTracker
    namespace_trackers: HashMap<String, NamespaceStatsTracker>,
    /// Cached connections per namespace (keyed by namespace name) to avoid
    /// rebuilding the netns-bound connection on every poll cycle.
    namespace_connections: HashMap<String, CachedConnection>,
}

impl BandwidthMonitor {
    /// Creates a new bandwidth monitor instance.
    pub fn new(session: Session, local_origin: LocalOrigin, backend_name: String) -> Self {
        Self {
            session,
            local_origin,
            backend_name,
            bandwidth_publishers: HashMap::new(),
            container_cache: None,
            namespace_trackers: HashMap::new(),
            namespace_connections: HashMap::new(),
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

    /// Monitors bandwidth statistics for interfaces within a specific namespace using netlink.
    #[instrument(skip(self, interfaces), fields(backend_name = %self.backend_name, namespace, interface_count = interfaces.len()))]
    pub async fn monitor_namespace_bandwidth(
        &mut self,
        namespace: &str,
        interfaces: &[&NetworkInterface],
    ) -> Result<()> {
        debug!(
            "Reading bandwidth stats via netlink for namespace: {}",
            namespace
        );

        // Get stats via netlink
        let stats_result = self.get_netlink_stats(namespace).await;

        let (snapshot, interface_stats) = match stats_result {
            Ok(result) => result,
            Err(e) => {
                // Use nlink's typed predicate (downcast through the anyhow chain)
                // instead of matching on the Display string. is_permission_denied()
                // also catches the Error::Io(EPERM) shape as of nlink 0.19.
                if is_permission_denied(&e) {
                    warn!(
                        "Cannot access namespace {}: insufficient permissions",
                        namespace
                    );
                } else {
                    warn!(
                        "Failed to get netlink stats for namespace {}: {}",
                        namespace, e
                    );
                }
                return Ok(());
            }
        };

        debug!(
            "Got {} interface stats from netlink in namespace {}",
            interface_stats.len(),
            namespace
        );

        // Get current timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Collect all updates first, then send them (to avoid borrow issues)
        let updates: Vec<BandwidthUpdate> = {
            // Get or create tracker for this namespace
            let tracker = self
                .namespace_trackers
                .entry(namespace.to_string())
                .or_insert_with(NamespaceStatsTracker::new);

            // Update tracker and get rates
            let rates_snapshot = tracker.tracker.update(snapshot);

            // Process each tracked interface
            interfaces
                .iter()
                .filter_map(|tracked_interface| {
                    let ifindex = tracked_interface.index;

                    // Find the stats for this interface
                    interface_stats.get(&ifindex).map(|nlink_stats| {
                        // Get rates from the snapshot or use cached values
                        let (rx_bps, tx_bps) = if let Some(ref rates) = rates_snapshot {
                            if let Some(link_rates) = rates.links.get(&ifindex) {
                                let rx = link_rates.rx_bytes_per_sec;
                                let tx = link_rates.tx_bytes_per_sec;
                                // Cache the rates
                                tracker.last_rates.insert(ifindex, (rx, tx));
                                (rx, tx)
                            } else {
                                // Use cached rates if available
                                tracker
                                    .last_rates
                                    .get(&ifindex)
                                    .copied()
                                    .unwrap_or((0.0, 0.0))
                            }
                        } else {
                            // First measurement, no rates yet
                            (0.0, 0.0)
                        };

                        let stats = NetworkBandwidthStats {
                            rx_bytes: nlink_stats.rx_bytes,
                            tx_bytes: nlink_stats.tx_bytes,
                            rx_packets: nlink_stats.rx_packets,
                            tx_packets: nlink_stats.tx_packets,
                            rx_errors: nlink_stats.rx_errors,
                            tx_errors: nlink_stats.tx_errors,
                            rx_dropped: nlink_stats.rx_dropped,
                            tx_dropped: nlink_stats.tx_dropped,
                            timestamp,
                            rx_bytes_per_sec: rx_bps,
                            tx_bytes_per_sec: tx_bps,
                        };

                        BandwidthUpdate {
                            namespace: tracked_interface.namespace.clone(),
                            interface: tracked_interface.name.clone(),
                            stats,
                            backend_name: self.backend_name.clone(),
                        }
                    })
                })
                .collect()
        };

        // Now send all updates (self is no longer borrowed by tracker)
        for update in updates {
            debug!(
                "Sending bandwidth update for {}/{}: RX rate {:.2} B/s, TX rate {:.2} B/s",
                update.namespace,
                update.interface,
                update.stats.rx_bytes_per_sec,
                update.stats.tx_bytes_per_sec
            );
            self.send_bandwidth_update(update).await?;
        }

        Ok(())
    }

    /// Get netlink statistics for a namespace.
    async fn get_netlink_stats(
        &mut self,
        namespace: &str,
    ) -> Result<(StatsSnapshot, HashMap<u32, NlinkLinkStats>)> {
        // Resolve the netns path for this namespace. `None` = default netns.
        let path = if Self::is_container_namespace(namespace) {
            // Container path comes from the shared cache and changes on restart.
            Some(
                self.get_container_namespace_path(namespace)
                    .await
                    .ok_or_else(|| {
                        anyhow::anyhow!("Container namespace {} has no path in cache", namespace)
                    })?,
            )
        } else if namespace == "default" {
            None
        } else {
            Some(PathBuf::from(format!("/var/run/netns/{}", namespace)))
        };

        let links = self.fetch_links(namespace, path).await?;

        // Create stats snapshot from links
        let snapshot = StatsSnapshot::from_links(&links);

        // Build a map by interface index using LinkMessage convenience methods.
        let interface_stats: HashMap<u32, NlinkLinkStats> = links
            .iter()
            .map(|link| (link.ifindex(), NlinkLinkStats::from_link_message(link)))
            .collect();

        Ok((snapshot, interface_stats))
    }

    /// Fetch the link list for a namespace, reusing a cached netns-bound
    /// connection when possible.
    ///
    /// A cached connection is reused only when its bound `path` still matches
    /// (so a restarted container with a new `/proc/<pid>/ns/net` path rebuilds).
    /// On any fetch error the cached connection is dropped so the next poll
    /// rebuilds it — this self-heals a deleted/recreated namespace or a dead
    /// socket within one cycle.
    async fn fetch_links(
        &mut self,
        namespace: &str,
        path: Option<PathBuf>,
    ) -> Result<Vec<LinkMessage>> {
        // (Re)build the connection if missing or bound to a stale path.
        let needs_new = self
            .namespace_connections
            .get(namespace)
            .is_none_or(|cached| cached.path != path);

        if needs_new {
            let conn = match &path {
                None => Connection::<Route>::new()
                    .map_err(|e| anyhow::Error::new(e).context("Failed to create connection"))?,
                Some(p) => Connection::<Route>::new_in_namespace_path(p).map_err(|e| {
                    anyhow::Error::new(e).context(format!(
                        "Failed to create connection for namespace {}",
                        namespace
                    ))
                })?,
            };
            self.namespace_connections.insert(
                namespace.to_string(),
                CachedConnection {
                    conn,
                    path: path.clone(),
                },
            );
        }

        // Scope the immutable borrow to the call so we can invalidate on error.
        let result = {
            let cached = self
                .namespace_connections
                .get(namespace)
                .expect("just inserted");
            cached.conn.get_links().await
        };

        result.map_err(|e| {
            // Drop the stale/dead connection; the next poll rebuilds it.
            self.namespace_connections.remove(namespace);
            anyhow::Error::new(e).context(format!("Failed to get links in namespace {}", namespace))
        })
    }

    /// Sends a bandwidth update message via Zenoh to the frontend.
    #[instrument(skip(self, update), fields(backend_name = %self.backend_name, namespace = %update.namespace, interface = %update.interface))]
    async fn send_bandwidth_update(&mut self, update: BandwidthUpdate) -> Result<()> {
        let payload = serde_json::to_string(&update).map_err(TcguiError::SerializationError)?;

        // Create publisher key for this specific interface
        let publisher_key = format!("{}/{}", update.namespace, update.interface);

        // Get or create publisher for this interface
        if !self.bandwidth_publishers.contains_key(&publisher_key) {
            let bandwidth_topic = tc::key(
                &self.local_origin,
                &tc::Subject::bandwidth(&update.namespace, &update.interface),
            );
            debug!(
                "Creating bandwidth publisher for {}: {}",
                publisher_key,
                bandwidth_topic.as_str()
            );

            let publisher = self
                .session
                .declare_publisher(zenoh::key_expr::OwnedKeyExpr::from(bandwidth_topic))
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

            debug!(
                "Sent bandwidth update for {}: RX rate {:.2} B/s, TX rate {:.2} B/s",
                publisher_key, update.stats.rx_bytes_per_sec, update.stats.tx_bytes_per_sec
            );
        }

        Ok(())
    }

    /// Parses `/proc/net/dev` file contents into bandwidth statistics (test helper).
    /// Kept for backward compatibility with existing tests.
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
                is_oper_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
                addresses: Vec::new(),
                qdisc_kind: None,
                link_speed_mbps: None,
            },
        );

        interfaces.insert(
            2,
            NetworkInterface {
                name: "eth0".to_string(),
                index: 2,
                namespace: "default".to_string(),
                is_up: true,
                is_oper_up: true,
                has_tc_qdisc: false,
                interface_type: InterfaceType::Physical,
                addresses: Vec::new(),
                qdisc_kind: None,
                link_speed_mbps: None,
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
