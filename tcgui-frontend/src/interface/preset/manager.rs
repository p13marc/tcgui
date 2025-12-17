//! Preset management component for network configuration presets.
//!
//! This component handles the display and management of network presets,
//! allowing users to quickly apply common traffic control configurations.

use iced::widget::{button, row, text};
use iced::Element;
use tcgui_shared::presets::NetworkPreset;

use crate::interface::state::InterfaceState;
use crate::messages::TcInterfaceMessage;

/// Component for preset management UI and logic
#[derive(Debug, Clone)]
pub struct PresetManagerComponent {
    /// Available presets
    available_presets: Vec<NetworkPreset>,
    /// Whether preset dropdown is visible
    pub show_presets: bool,
}

impl Default for PresetManagerComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetManagerComponent {
    /// Create new preset manager component
    pub fn new() -> Self {
        Self {
            available_presets: NetworkPreset::all_presets(),
            show_presets: false,
        }
    }

    /// Toggle the preset dropdown visibility
    pub fn toggle_dropdown(&mut self) {
        self.show_presets = !self.show_presets;
    }

    /// Apply a preset configuration to the interface state
    ///
    /// Returns true if settings were changed (i.e., not Custom preset)
    pub fn apply_preset(&mut self, preset: NetworkPreset, state: &mut InterfaceState) -> bool {
        state.current_preset = preset.clone();
        self.show_presets = false; // Close dropdown after selection

        // Custom preset doesn't change settings
        if matches!(preset, NetworkPreset::Custom) {
            return false;
        }

        let config = preset.get_configuration();

        // Apply loss settings
        if config.loss > 0.0 {
            state.features.loss.enable();
            state.features.loss.config.percentage = config.loss;
            state.features.loss.config.correlation = config.correlation.unwrap_or(0.0);
        } else {
            state.features.loss.disable();
        }

        // Apply delay settings
        if config.delay_enabled {
            state.features.delay.enable();
            state.features.delay.config.base_ms = config.delay_ms.unwrap_or(0.0);
            state.features.delay.config.jitter_ms = config.delay_jitter_ms.unwrap_or(0.0);
            state.features.delay.config.correlation = config.delay_correlation.unwrap_or(0.0);
        } else {
            state.features.delay.disable();
        }

        // Apply duplicate settings
        if config.duplicate_enabled {
            state.features.duplicate.enable();
            state.features.duplicate.config.percentage = config.duplicate_percent.unwrap_or(0.0);
            state.features.duplicate.config.correlation =
                config.duplicate_correlation.unwrap_or(0.0);
        } else {
            state.features.duplicate.disable();
        }

        // Apply reorder settings
        if config.reorder_enabled {
            state.features.reorder.enable();
            state.features.reorder.config.percentage = config.reorder_percent.unwrap_or(0.0);
            state.features.reorder.config.correlation = config.reorder_correlation.unwrap_or(0.0);
            state.features.reorder.config.gap = config.reorder_gap.unwrap_or(5);
        } else {
            state.features.reorder.disable();
        }

        // Apply corrupt settings
        if config.corrupt_enabled {
            state.features.corrupt.enable();
            state.features.corrupt.config.percentage = config.corrupt_percent.unwrap_or(0.0);
            state.features.corrupt.config.correlation = config.corrupt_correlation.unwrap_or(0.0);
        } else {
            state.features.corrupt.disable();
        }

        // Apply rate limit settings
        if config.rate_limit_enabled {
            state.features.rate_limit.enable();
            state.features.rate_limit.config.rate_kbps = config.rate_limit_kbps.unwrap_or(1000);
        } else {
            state.features.rate_limit.disable();
        }

        // Mark as applying to trigger backend update
        state.applying = true;
        true
    }

    /// Clear all features (disable all TC settings)
    pub fn clear_all_features(&mut self, state: &mut InterfaceState) {
        state.current_preset = NetworkPreset::Custom;
        self.show_presets = false;

        // Disable all features
        state.features.loss.disable();
        state.features.delay.disable();
        state.features.duplicate.disable();
        state.features.reorder.disable();
        state.features.corrupt.disable();
        state.features.rate_limit.disable();

        // Mark as applying to trigger backend update
        state.applying = true;
    }

    /// Render the preset selector UI
    ///
    /// Takes a reference to the current preset from state to ensure display is in sync.
    /// Uses a horizontal scrollable row when expanded to avoid vertical layout shifts.
    pub fn view(&self, current_preset: &NetworkPreset) -> Element<'_, TcInterfaceMessage> {
        let current_label = current_preset.display_name();

        if self.show_presets {
            // When expanded, show a horizontal row of preset buttons plus Clear
            let mut buttons: Vec<Element<'_, _>> = self
                .available_presets
                .iter()
                .map(|preset| {
                    let is_selected = preset == current_preset;
                    let label = preset.short_name();

                    button(text(label).size(10))
                        .padding([2, 4])
                        .style(if is_selected {
                            button::primary
                        } else {
                            button::secondary
                        })
                        .on_press(TcInterfaceMessage::PresetSelected(preset.clone()))
                        .into()
                })
                .collect();

            // Add Clear button at the end
            buttons.push(
                button(text("Clear").size(10))
                    .padding([2, 4])
                    .style(button::danger)
                    .on_press(TcInterfaceMessage::ClearAllFeatures)
                    .into(),
            );

            row(buttons).spacing(2).into()
        } else {
            // When collapsed, show a button with current preset name
            button(row![text(current_label).size(11), text(" ▼").size(9),])
                .padding([2, 6])
                .on_press(TcInterfaceMessage::TogglePresetDropdown)
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_manager_creation() {
        let component = PresetManagerComponent::new();
        assert!(!component.show_presets);
        assert_eq!(component.available_presets.len(), 8);
    }

    #[test]
    fn test_toggle_dropdown() {
        let mut component = PresetManagerComponent::new();
        assert!(!component.show_presets);

        component.toggle_dropdown();
        assert!(component.show_presets);

        component.toggle_dropdown();
        assert!(!component.show_presets);
    }

    #[test]
    fn test_apply_custom_preset_returns_false() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        // Set some values first
        state.features.loss.enable();
        state.features.loss.config.percentage = 5.0;

        // Apply Custom preset - should return false and not change settings
        let changed = component.apply_preset(NetworkPreset::Custom, &mut state);

        assert!(!changed);
        assert!(state.features.loss.enabled);
        assert_eq!(state.features.loss.config.percentage, 5.0);
    }

    #[test]
    fn test_apply_satellite_preset() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        let changed = component.apply_preset(NetworkPreset::SatelliteLink, &mut state);

        assert!(changed);
        assert!(state.applying);

        // Satellite link: 1% loss, 500ms delay, 2 Mbps rate limit
        assert!(state.features.loss.enabled);
        assert_eq!(state.features.loss.config.percentage, 1.0);

        assert!(state.features.delay.enabled);
        assert_eq!(state.features.delay.config.base_ms, 500.0);

        assert!(state.features.rate_limit.enabled);
        assert_eq!(state.features.rate_limit.config.rate_kbps, 2000);

        // Should not enable corrupt
        assert!(!state.features.corrupt.enabled);
    }

    #[test]
    fn test_apply_poor_wifi_preset() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        let changed = component.apply_preset(NetworkPreset::PoorWiFi, &mut state);

        assert!(changed);

        // Poor WiFi: 8% loss, delay, duplicate, reorder, corrupt all enabled
        assert!(state.features.loss.enabled);
        assert_eq!(state.features.loss.config.percentage, 8.0);

        assert!(state.features.delay.enabled);
        assert!(state.features.duplicate.enabled);
        assert!(state.features.reorder.enabled);
        assert!(state.features.corrupt.enabled);

        // Should not enable rate limit
        assert!(!state.features.rate_limit.enabled);
    }

    #[test]
    fn test_apply_preset_closes_dropdown() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        component.show_presets = true;
        component.apply_preset(NetworkPreset::WanLink, &mut state);

        assert!(!component.show_presets);
    }

    #[test]
    fn test_apply_preset_updates_state_preset() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        // Apply a preset
        component.apply_preset(NetworkPreset::SatelliteLink, &mut state);
        assert_eq!(state.current_preset, NetworkPreset::SatelliteLink);

        // Apply another preset
        component.apply_preset(NetworkPreset::PoorWiFi, &mut state);
        assert_eq!(state.current_preset, NetworkPreset::PoorWiFi);
    }
}
