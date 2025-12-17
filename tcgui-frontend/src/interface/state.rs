//! Centralized state management for interface components.
//!
//! This module leverages the structured configuration types from Sprint 1
//! to provide clean state management across all interface components.

use tcgui_shared::{presets::NetworkPreset, InterfaceFeatureStates, NetworkBandwidthStats};

/// Centralized state for a network interface and all its components
#[derive(Debug, Clone)]
pub struct InterfaceState {
    /// Interface name (e.g., "eth0", "fo")
    pub name: String,

    /// Current interface state (from backend)
    pub is_up: bool,

    /// Whether TC qdisc is currently configured (from backend)
    pub has_tc_qdisc: bool,

    /// User's desired interface enable state
    pub interface_enabled: bool,

    /// Feature states using Sprint 1 structured configuration
    pub features: InterfaceFeatureStates,

    /// Current bandwidth statistics (updated from backend)
    pub bandwidth_stats: Option<NetworkBandwidthStats>,

    /// Status message history (bounded to prevent memory growth)
    pub status_messages: Vec<String>,

    /// Currently selected preset
    pub current_preset: NetworkPreset,

    // Note: show_presets field removed as it was unused
    /// Whether a TC operation is currently in progress
    pub applying: bool,

    /// Whether an interface state change is in progress
    pub applying_interface_state: bool,
}

impl InterfaceState {
    /// Create new interface state with defaults
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            is_up: false,
            has_tc_qdisc: false,
            interface_enabled: true,
            features: InterfaceFeatureStates::new(),
            bandwidth_stats: None,
            status_messages: vec!["Ready.".to_string()],
            current_preset: NetworkPreset::Custom,
            // show_presets field removed
            applying: false,
            applying_interface_state: false,
        }
    }

    // Methods removed as they were unused in current implementation:
    // - get_tc_config: Available via features.to_config() if needed
    // - validate_config: Available via features.to_config().validate() if needed
    // - has_pending_changes: Available via features.has_any_pending_changes() if needed
    // - mark_all_applied: Available via features.mark_all_applied() if needed

    /// Add a status message (bounded history)
    pub fn add_status_message(&mut self, message: String, is_update: bool) {
        if is_update {
            if let Some(last) = self.status_messages.last_mut() {
                // Replace the last message if it's an update
                *last = message;
            } else {
                // No messages yet, just push
                self.status_messages.push(message);
            }
        } else {
            self.status_messages.push(message);
        }

        // Keep only the last 10 messages to prevent memory growth
        if self.status_messages.len() > 10 {
            self.status_messages
                .drain(0..self.status_messages.len() - 10);
        }
    }

    // Methods removed as they were unused in current implementation:
    // - clear_status_messages: Available via status_messages.clear() if needed
    // - latest_status: Available via status_messages.last() if needed

    /// Update bandwidth statistics
    pub fn update_bandwidth_stats(&mut self, stats: NetworkBandwidthStats) {
        self.bandwidth_stats = Some(stats);
    }

    /// Check if the interface is currently up
    pub fn is_up(&self) -> bool {
        self.is_up
    }

    /// Check if TC qdisc is configured
    pub fn has_tc_qdisc(&self) -> bool {
        self.has_tc_qdisc
    }

    /// Set interface up/down state from backend update
    pub fn set_interface_state(&mut self, is_up: bool, has_tc_qdisc: bool) {
        self.is_up = is_up;
        self.has_tc_qdisc = has_tc_qdisc;
    }

    // Methods removed as they were unused in current implementation:
    // - apply_preset: Preset application logic available if needed later
    // - matches_preset: Preset matching logic available if needed later
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::TcValidate; // Needed for validate() method in tests

    #[test]
    fn test_interface_state_creation() {
        let state = InterfaceState::new("eth0");
        assert_eq!(state.name, "eth0");
        assert!(!state.is_up);
        assert!(!state.has_tc_qdisc);
        assert!(state.interface_enabled);
        assert_eq!(state.status_messages.len(), 1);
        assert_eq!(state.status_messages[0], "Ready.");
    }

    #[test]
    fn test_status_message_management() {
        let mut state = InterfaceState::new("eth0");

        // Add multiple messages
        for i in 0..15 {
            state.add_status_message(format!("Message {}", i), false);
        }

        // Should be bounded to 10 messages
        // Started with "Ready.", then added "Message 0" through "Message 14"
        // Total: 16 messages, trimmed to last 10
        assert_eq!(state.status_messages.len(), 10);
        assert_eq!(state.status_messages[0], "Message 5"); // First kept message after trimming
        assert_eq!(state.status_messages[9], "Message 14"); // Last message
    }

    #[test]
    fn test_feature_state_integration() {
        let mut state = InterfaceState::new("eth0");

        // Enable loss feature
        state.features.loss.enable();
        state.features.loss.config.percentage = 10.0;
        state.features.loss.config.correlation = 5.0;

        // Get TC config directly from features
        let config = state.features.to_config();
        assert!(config.loss.enabled);
        assert_eq!(config.loss.percentage, 10.0);
        assert_eq!(config.loss.correlation, 5.0);

        // Validate configuration directly
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pending_changes_tracking() {
        let mut state = InterfaceState::new("eth0");

        // Initially no pending changes
        assert!(!state.features.has_any_pending_changes());

        // Enable a feature - should have pending changes
        state.features.loss.enable();
        assert!(state.features.has_any_pending_changes());

        // Mark as applied - no pending changes
        state.features.mark_all_applied();
        state.applying = false;
        assert!(!state.features.has_any_pending_changes());
        assert!(!state.applying);
    }
}
