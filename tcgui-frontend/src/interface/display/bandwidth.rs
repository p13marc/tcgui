//! Bandwidth display component for real-time network statistics.
//!
//! This component handles the display of network bandwidth statistics with
//! automatic unit formatting and visual indicators.

use iced::widget::{row, text};
use iced::{Color, Element};
use tcgui_shared::NetworkBandwidthStats;

use crate::messages::TcInterfaceMessage;

/// Component for bandwidth statistics display
#[derive(Debug, Clone)]
pub struct BandwidthDisplayComponent {
    /// Current bandwidth statistics (None if no data available)
    stats: Option<NetworkBandwidthStats>,
}

impl Default for BandwidthDisplayComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl BandwidthDisplayComponent {
    /// Create a new bandwidth display component
    pub fn new() -> Self {
        Self { stats: None }
    }

    /// Update bandwidth statistics
    pub fn update_stats(&mut self, stats: NetworkBandwidthStats) {
        self.stats = Some(stats);
    }

    // Removed unused methods:
    // - clear_stats: Available via self.stats = None if needed
    // - stats: Available via direct field access if needed

    /// Format bytes per second with appropriate units
    fn format_rate(bytes_per_sec: f64) -> String {
        if bytes_per_sec >= 1_073_741_824.0 {
            format!("{:.1}G", bytes_per_sec / 1_073_741_824.0)
        } else if bytes_per_sec >= 1_048_576.0 {
            format!("{:.1}M", bytes_per_sec / 1_048_576.0)
        } else if bytes_per_sec >= 1024.0 {
            format!("{:.0}K", bytes_per_sec / 1024.0)
        } else if bytes_per_sec > 0.0 {
            format!("{:.0}B", bytes_per_sec)
        } else {
            "0".to_string()
        }
    }

    /// Render the bandwidth display
    pub fn view(&self) -> Element<'_, TcInterfaceMessage> {
        if let Some(stats) = &self.stats {
            let rx_rate = Self::format_rate(stats.rx_bytes_per_sec);
            let tx_rate = Self::format_rate(stats.tx_bytes_per_sec);

            row![
                text("📈").size(11),
                text(rx_rate).size(11).style(|_| text::Style {
                    color: Some(Color::from_rgb(0.0, 0.6, 0.9))
                }),
                text("📤").size(11),
                text(tx_rate).size(11).style(|_| text::Style {
                    color: Some(Color::from_rgb(0.9, 0.5, 0.0))
                })
            ]
            .spacing(2)
            .into()
        } else {
            let text_secondary = Color::from_rgb(0.4, 0.4, 0.4);
            row![
                text("📊").size(11),
                text("--").size(11).style(move |_| text::Style {
                    color: Some(text_secondary)
                })
            ]
            .spacing(2)
            .into()
        }
    }

    // Removed unused methods:
    // - detailed_stats: Statistics details available via stats field if needed
    // - has_errors: Error checking logic available if needed
    // - error_color: Color logic available if needed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bandwidth_display_creation() {
        let component = BandwidthDisplayComponent::new();
        assert!(component.stats.is_none());
        // Test that component initializes correctly
        assert!(component.stats.is_none());
    }

    #[test]
    fn test_format_rate() {
        assert_eq!(BandwidthDisplayComponent::format_rate(0.0), "0");
        assert_eq!(BandwidthDisplayComponent::format_rate(500.0), "500B");
        assert_eq!(BandwidthDisplayComponent::format_rate(1500.0), "1K");
        assert_eq!(BandwidthDisplayComponent::format_rate(1_500_000.0), "1.4M");
        assert_eq!(
            BandwidthDisplayComponent::format_rate(1_500_000_000.0),
            "1.4G"
        );
    }

    #[test]
    fn test_update_stats() {
        let mut component = BandwidthDisplayComponent::new();
        let stats = NetworkBandwidthStats {
            rx_bytes: 10_000_000,
            rx_packets: 10_000,
            rx_errors: 0,
            rx_dropped: 0,
            tx_bytes: 5_000_000,
            tx_packets: 5_000,
            tx_errors: 1,
            tx_dropped: 0,
            timestamp: 1234567890,
            rx_bytes_per_sec: 1_000_000.0,
            tx_bytes_per_sec: 500_000.0,
        };

        component.update_stats(stats.clone());
        assert!(component.stats.is_some());
        assert_eq!(
            component.stats.as_ref().unwrap().rx_bytes_per_sec,
            1_000_000.0
        );
        // Test that stats were updated correctly
        assert_eq!(component.stats.as_ref().unwrap().tx_errors, 1);
    }

    #[test]
    fn test_update_stats_values() {
        let mut component = BandwidthDisplayComponent::new();
        let stats = NetworkBandwidthStats {
            rx_bytes: 100_000_000,
            rx_packets: 1000,
            rx_errors: 2,
            rx_dropped: 0,
            tx_bytes: 50_000_000,
            tx_packets: 500,
            tx_errors: 1,
            tx_dropped: 0,
            timestamp: 1234567890,
            rx_bytes_per_sec: 1_048_576.0, // 1 MB/s
            tx_bytes_per_sec: 524_288.0,   // 0.5 MB/s
        };

        component.update_stats(stats);
        assert!(component.stats.is_some());
        assert_eq!(
            component.stats.as_ref().unwrap().rx_bytes_per_sec,
            1_048_576.0
        );
        assert_eq!(
            component.stats.as_ref().unwrap().tx_bytes_per_sec,
            524_288.0
        );
        assert_eq!(component.stats.as_ref().unwrap().rx_errors, 2);
    }

    #[test]
    fn test_stats_management() {
        let mut component = BandwidthDisplayComponent::new();
        let stats = NetworkBandwidthStats {
            rx_bytes: 1000,
            rx_packets: 10,
            rx_errors: 0,
            rx_dropped: 0,
            tx_bytes: 500,
            tx_packets: 5,
            tx_errors: 0,
            tx_dropped: 0,
            timestamp: 1234567890,
            rx_bytes_per_sec: 1000.0,
            tx_bytes_per_sec: 500.0,
        };

        component.update_stats(stats);
        assert!(component.stats.is_some());

        // Clear stats manually (since clear_stats method was removed)
        component.stats = None;
        assert!(component.stats.is_none());
    }
}
