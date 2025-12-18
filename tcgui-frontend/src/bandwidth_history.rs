//! Bandwidth history storage for time-series visualization.
//!
//! This module provides data structures for storing and managing historical
//! bandwidth samples, enabling chart visualization of network throughput over time.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Single data point in the bandwidth history.
#[derive(Debug, Clone, Copy)]
pub struct BandwidthSample {
    /// When this sample was recorded
    pub timestamp: Instant,
    /// Receive rate in bytes per second
    pub rx_bytes_per_sec: f64,
    /// Transmit rate in bytes per second
    pub tx_bytes_per_sec: f64,
}

/// Time-series data for one interface.
///
/// Stores bandwidth samples in a ring buffer, automatically pruning
/// samples older than the configured maximum duration.
#[derive(Debug, Clone)]
pub struct BandwidthHistory {
    samples: VecDeque<BandwidthSample>,
    max_duration: Duration,
    max_samples: usize,
}

impl BandwidthHistory {
    /// Create a new bandwidth history with the specified maximum duration.
    ///
    /// At 1 sample/second:
    /// - 1 minute = 60 samples
    /// - 5 minutes = 300 samples
    /// - 1 hour = 3600 samples
    pub fn new(max_duration: Duration) -> Self {
        // Assume ~1 sample per second, add some buffer
        let max_samples = (max_duration.as_secs() as usize).saturating_add(10);
        Self {
            samples: VecDeque::with_capacity(max_samples.min(512)),
            max_duration,
            max_samples,
        }
    }

    /// Add a new bandwidth sample.
    pub fn push(&mut self, rx_bytes_per_sec: f64, tx_bytes_per_sec: f64) {
        let now = Instant::now();

        self.samples.push_back(BandwidthSample {
            timestamp: now,
            rx_bytes_per_sec,
            tx_bytes_per_sec,
        });

        // Remove old samples beyond max duration
        let cutoff = now - self.max_duration;
        while let Some(front) = self.samples.front() {
            if front.timestamp < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // Enforce max samples limit as safety measure
        while self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }
    }

    /// Get all stored samples.
    pub fn samples(&self) -> &VecDeque<BandwidthSample> {
        &self.samples
    }

    /// Get the number of stored samples.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if there are no samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Get samples within a specific time window.
    pub fn samples_in_window(&self, window: Duration) -> impl Iterator<Item = &BandwidthSample> {
        let cutoff = Instant::now() - window;
        self.samples.iter().filter(move |s| s.timestamp >= cutoff)
    }

    /// Calculate peak RX and TX values within a time window.
    pub fn peak_in_window(&self, window: Duration) -> (f64, f64) {
        self.samples_in_window(window)
            .fold((0.0, 0.0), |(max_rx, max_tx), s| {
                (
                    max_rx.max(s.rx_bytes_per_sec),
                    max_tx.max(s.tx_bytes_per_sec),
                )
            })
    }

    /// Calculate average RX and TX values within a time window.
    pub fn average_in_window(&self, window: Duration) -> (f64, f64) {
        let samples: Vec<_> = self.samples_in_window(window).collect();
        if samples.is_empty() {
            return (0.0, 0.0);
        }
        let count = samples.len() as f64;
        let (sum_rx, sum_tx) = samples.iter().fold((0.0, 0.0), |(rx, tx), s| {
            (rx + s.rx_bytes_per_sec, tx + s.tx_bytes_per_sec)
        });
        (sum_rx / count, sum_tx / count)
    }

    /// Get the timestamp of the most recent sample, if any.
    pub fn last_update(&self) -> Option<Instant> {
        self.samples.back().map(|s| s.timestamp)
    }
}

/// Key for identifying an interface across backends and namespaces.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceKey {
    pub backend: String,
    pub namespace: String,
    pub interface: String,
}

impl InterfaceKey {
    pub fn new(backend: impl Into<String>, namespace: impl Into<String>, interface: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            namespace: namespace.into(),
            interface: interface.into(),
        }
    }
}

/// Manages bandwidth history for all interfaces.
///
/// Provides a centralized store for bandwidth time-series data,
/// with automatic cleanup of stale entries.
#[derive(Debug)]
pub struct BandwidthHistoryManager {
    histories: HashMap<InterfaceKey, BandwidthHistory>,
    default_duration: Duration,
}

impl Default for BandwidthHistoryManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(300)) // 5 minutes default
    }
}

impl BandwidthHistoryManager {
    /// Create a new history manager with the specified default duration.
    pub fn new(default_duration: Duration) -> Self {
        Self {
            histories: HashMap::new(),
            default_duration,
        }
    }

    /// Record a bandwidth sample for an interface.
    pub fn record(
        &mut self,
        backend: &str,
        namespace: &str,
        interface: &str,
        rx_bytes_per_sec: f64,
        tx_bytes_per_sec: f64,
    ) {
        let key = InterfaceKey::new(backend, namespace, interface);
        self.histories
            .entry(key)
            .or_insert_with(|| BandwidthHistory::new(self.default_duration))
            .push(rx_bytes_per_sec, tx_bytes_per_sec);
    }

    /// Get the bandwidth history for an interface.
    pub fn get(&self, backend: &str, namespace: &str, interface: &str) -> Option<&BandwidthHistory> {
        let key = InterfaceKey::new(backend, namespace, interface);
        self.histories.get(&key)
    }

    /// Remove history for a specific interface.
    pub fn remove(&mut self, backend: &str, namespace: &str, interface: &str) {
        let key = InterfaceKey::new(backend, namespace, interface);
        self.histories.remove(&key);
    }

    /// Remove all history for a specific backend.
    pub fn remove_backend(&mut self, backend: &str) {
        self.histories.retain(|key, _| key.backend != backend);
    }

    /// Clean up stale entries that haven't been updated recently.
    pub fn cleanup_stale(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.histories.retain(|_, history| {
            history
                .last_update()
                .is_some_and(|last| now.duration_since(last) < max_age)
        });
    }

    /// Get the number of tracked interfaces.
    pub fn interface_count(&self) -> usize {
        self.histories.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_bandwidth_history_new() {
        let history = BandwidthHistory::new(Duration::from_secs(60));
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_bandwidth_history_push() {
        let mut history = BandwidthHistory::new(Duration::from_secs(60));

        history.push(1000.0, 500.0);
        assert_eq!(history.len(), 1);

        history.push(2000.0, 1000.0);
        assert_eq!(history.len(), 2);

        let samples: Vec<_> = history.samples().iter().collect();
        assert_eq!(samples[0].rx_bytes_per_sec, 1000.0);
        assert_eq!(samples[1].tx_bytes_per_sec, 1000.0);
    }

    #[test]
    fn test_bandwidth_history_peak() {
        let mut history = BandwidthHistory::new(Duration::from_secs(60));

        history.push(1000.0, 500.0);
        history.push(3000.0, 1500.0);
        history.push(2000.0, 1000.0);

        let (peak_rx, peak_tx) = history.peak_in_window(Duration::from_secs(60));
        assert_eq!(peak_rx, 3000.0);
        assert_eq!(peak_tx, 1500.0);
    }

    #[test]
    fn test_bandwidth_history_average() {
        let mut history = BandwidthHistory::new(Duration::from_secs(60));

        history.push(1000.0, 600.0);
        history.push(2000.0, 900.0);
        history.push(3000.0, 1500.0);

        let (avg_rx, avg_tx) = history.average_in_window(Duration::from_secs(60));
        assert_eq!(avg_rx, 2000.0);
        assert_eq!(avg_tx, 1000.0);
    }

    #[test]
    fn test_bandwidth_history_empty_average() {
        let history = BandwidthHistory::new(Duration::from_secs(60));
        let (avg_rx, avg_tx) = history.average_in_window(Duration::from_secs(60));
        assert_eq!(avg_rx, 0.0);
        assert_eq!(avg_tx, 0.0);
    }

    #[test]
    fn test_bandwidth_history_manager_record() {
        let mut manager = BandwidthHistoryManager::new(Duration::from_secs(60));

        manager.record("backend1", "ns1", "eth0", 1000.0, 500.0);
        manager.record("backend1", "ns1", "eth0", 2000.0, 1000.0);
        manager.record("backend1", "ns2", "eth1", 3000.0, 1500.0);

        assert_eq!(manager.interface_count(), 2);

        let eth0_history = manager.get("backend1", "ns1", "eth0").unwrap();
        assert_eq!(eth0_history.len(), 2);

        let eth1_history = manager.get("backend1", "ns2", "eth1").unwrap();
        assert_eq!(eth1_history.len(), 1);
    }

    #[test]
    fn test_bandwidth_history_manager_remove() {
        let mut manager = BandwidthHistoryManager::new(Duration::from_secs(60));

        manager.record("backend1", "ns1", "eth0", 1000.0, 500.0);
        manager.record("backend1", "ns1", "eth1", 2000.0, 1000.0);

        assert_eq!(manager.interface_count(), 2);

        manager.remove("backend1", "ns1", "eth0");
        assert_eq!(manager.interface_count(), 1);
        assert!(manager.get("backend1", "ns1", "eth0").is_none());
        assert!(manager.get("backend1", "ns1", "eth1").is_some());
    }

    #[test]
    fn test_bandwidth_history_manager_remove_backend() {
        let mut manager = BandwidthHistoryManager::new(Duration::from_secs(60));

        manager.record("backend1", "ns1", "eth0", 1000.0, 500.0);
        manager.record("backend1", "ns2", "eth1", 2000.0, 1000.0);
        manager.record("backend2", "ns1", "eth0", 3000.0, 1500.0);

        assert_eq!(manager.interface_count(), 3);

        manager.remove_backend("backend1");
        assert_eq!(manager.interface_count(), 1);
        assert!(manager.get("backend2", "ns1", "eth0").is_some());
    }

    #[test]
    fn test_interface_key_equality() {
        let key1 = InterfaceKey::new("b1", "ns1", "eth0");
        let key2 = InterfaceKey::new("b1", "ns1", "eth0");
        let key3 = InterfaceKey::new("b1", "ns1", "eth1");

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_bandwidth_history_pruning() {
        // Use a very short duration for testing
        let mut history = BandwidthHistory::new(Duration::from_millis(50));

        history.push(1000.0, 500.0);
        assert_eq!(history.len(), 1);

        // Wait for the sample to expire
        sleep(Duration::from_millis(60));

        // Push a new sample, which should trigger pruning
        history.push(2000.0, 1000.0);

        // Old sample should be pruned
        assert_eq!(history.len(), 1);
        assert_eq!(history.samples().back().unwrap().rx_bytes_per_sec, 2000.0);
    }
}
