//! Preset management component for network configuration presets.
//!
//! This component handles the display and management of network presets,
//! allowing users to quickly apply common traffic control configurations.
//! Presets are loaded from JSON5 files by the backend.

use iced::widget::{button, row, text};
use iced::Element;
use tcgui_shared::presets::{CustomPreset, PresetList};

use crate::theme::Theme;

use crate::icons::Icon;
use crate::interface::state::InterfaceState;
use crate::messages::TcInterfaceMessage;
use crate::view::{scaled, scaled_spacing};

/// Component for preset management UI and logic
#[derive(Debug, Clone)]
pub struct PresetManagerComponent {
    /// Whether preset dropdown is visible
    pub show_presets: bool,
}

impl PresetManagerComponent {
    /// Check if the preset selector is expanded
    pub fn is_expanded(&self) -> bool {
        self.show_presets
    }
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
            show_presets: false,
        }
    }

    /// Toggle the preset dropdown visibility
    pub fn toggle_dropdown(&mut self) {
        self.show_presets = !self.show_presets;
    }

    /// Apply a preset configuration to the interface state
    ///
    /// Returns true if settings were changed
    pub fn apply_preset(&mut self, preset: &CustomPreset, state: &mut InterfaceState) -> bool {
        state.current_preset_id = Some(preset.id.clone());
        self.show_presets = false; // Close dropdown after selection

        let config = &preset.config;

        // Apply loss settings
        if config.loss.enabled && config.loss.percentage > 0.0 {
            state.features.loss.enable();
            state.features.loss.config.percentage = config.loss.percentage;
            state.features.loss.config.correlation = config.loss.correlation;
        } else {
            state.features.loss.disable();
        }

        // Apply delay settings
        if config.delay.enabled {
            state.features.delay.enable();
            state.features.delay.config.base_ms = config.delay.base_ms;
            state.features.delay.config.jitter_ms = config.delay.jitter_ms;
            state.features.delay.config.correlation = config.delay.correlation;
        } else {
            state.features.delay.disable();
        }

        // Apply duplicate settings
        if config.duplicate.enabled {
            state.features.duplicate.enable();
            state.features.duplicate.config.percentage = config.duplicate.percentage;
            state.features.duplicate.config.correlation = config.duplicate.correlation;
        } else {
            state.features.duplicate.disable();
        }

        // Apply reorder settings
        if config.reorder.enabled {
            state.features.reorder.enable();
            state.features.reorder.config.percentage = config.reorder.percentage;
            state.features.reorder.config.correlation = config.reorder.correlation;
            state.features.reorder.config.gap = config.reorder.gap;
        } else {
            state.features.reorder.disable();
        }

        // Apply corrupt settings
        if config.corrupt.enabled {
            state.features.corrupt.enable();
            state.features.corrupt.config.percentage = config.corrupt.percentage;
            state.features.corrupt.config.correlation = config.corrupt.correlation;
        } else {
            state.features.corrupt.disable();
        }

        // Apply rate limit settings
        if config.rate_limit.enabled {
            state.features.rate_limit.enable();
            state.features.rate_limit.config.rate_kbps = config.rate_limit.rate_kbps;
        } else {
            state.features.rate_limit.disable();
        }

        // Mark as applying to trigger backend update
        state.applying = true;
        true
    }

    /// Clear all features (disable all TC settings)
    pub fn clear_all_features(&mut self, state: &mut InterfaceState) {
        state.current_preset_id = None;
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

    /// Get short display name for a preset (first word or truncated)
    fn short_name(preset: &CustomPreset) -> String {
        // Use first word of name, or truncate to 6 chars
        preset
            .name
            .split_whitespace()
            .next()
            .map(|s| if s.len() > 6 { &s[..6] } else { s })
            .unwrap_or(&preset.id)
            .to_string()
    }

    /// Render the preset selector UI
    ///
    /// Takes a reference to the preset list and current preset ID.
    /// Uses a horizontal scrollable row when expanded to avoid vertical layout shifts.
    pub fn view<'a>(
        &self,
        preset_list: &'a PresetList,
        current_preset_id: &Option<String>,
        theme: &Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        let current_label = current_preset_id
            .as_ref()
            .and_then(|id| preset_list.find_by_id(id))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Custom".to_string());

        if self.show_presets {
            // When expanded, show a horizontal row of preset buttons plus Clear
            let mut buttons: Vec<Element<'a, _>> = preset_list
                .all()
                .iter()
                .map(|preset| {
                    let is_selected = current_preset_id
                        .as_ref()
                        .map(|id| id == &preset.id)
                        .unwrap_or(false);
                    let label = Self::short_name(preset);

                    button(text(label).size(scaled(10, zoom)))
                        .padding([2.0 * zoom, 4.0 * zoom])
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
                button(text("Clear").size(scaled(10, zoom)))
                    .padding([2.0 * zoom, 4.0 * zoom])
                    .style(button::danger)
                    .on_press(TcInterfaceMessage::ClearAllFeatures)
                    .into(),
            );

            row(buttons).spacing(scaled_spacing(2, zoom)).into()
        } else {
            // When collapsed, show a button with current preset name
            let icon_color = theme.colors.text_primary;
            button(
                row![
                    text(current_label).size(scaled(11, zoom)),
                    Icon::ChevronDown.svg_sized_colored(scaled(9, zoom), icon_color),
                ]
                .align_y(iced::Alignment::Center),
            )
            .padding([2.0 * zoom, 6.0 * zoom])
            .on_press(TcInterfaceMessage::TogglePresetDropdown)
            .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::TcNetemConfig;

    fn create_test_preset(id: &str, name: &str, loss_pct: f32) -> CustomPreset {
        let mut config = TcNetemConfig::default();
        if loss_pct > 0.0 {
            config.loss.enabled = true;
            config.loss.percentage = loss_pct;
        }
        CustomPreset {
            id: id.to_string(),
            name: name.to_string(),
            description: format!("Test preset {}", name),
            config,
        }
    }

    #[test]
    fn test_preset_manager_creation() {
        let component = PresetManagerComponent::new();
        assert!(!component.show_presets);
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
    fn test_apply_preset() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        let preset = create_test_preset("test-preset", "Test Preset", 5.0);
        let changed = component.apply_preset(&preset, &mut state);

        assert!(changed);
        assert!(state.applying);
        assert_eq!(state.current_preset_id, Some("test-preset".to_string()));
        assert!(state.features.loss.enabled);
        assert_eq!(state.features.loss.config.percentage, 5.0);
    }

    #[test]
    fn test_apply_preset_closes_dropdown() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        component.show_presets = true;
        let preset = create_test_preset("test", "Test", 1.0);
        component.apply_preset(&preset, &mut state);

        assert!(!component.show_presets);
    }

    #[test]
    fn test_clear_all_features() {
        let mut component = PresetManagerComponent::new();
        let mut state = InterfaceState::new("eth0");

        // Enable some features first
        state.features.loss.enable();
        state.features.loss.config.percentage = 10.0;
        state.features.delay.enable();
        state.current_preset_id = Some("some-preset".to_string());

        component.clear_all_features(&mut state);

        assert!(!state.features.loss.enabled);
        assert!(!state.features.delay.enabled);
        assert!(state.current_preset_id.is_none());
        assert!(state.applying);
    }

    #[test]
    fn test_short_name() {
        let preset1 = create_test_preset("sat", "Satellite Link", 1.0);
        assert_eq!(PresetManagerComponent::short_name(&preset1), "Satell");

        let preset2 = create_test_preset("wan", "WAN", 0.5);
        assert_eq!(PresetManagerComponent::short_name(&preset2), "WAN");

        let preset3 = create_test_preset("poor-wifi", "Poor WiFi Connection", 8.0);
        assert_eq!(PresetManagerComponent::short_name(&preset3), "Poor");
    }
}
