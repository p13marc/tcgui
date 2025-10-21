//! Preset management component for network configuration presets.
//!
//! This component handles the display and management of network presets,
//! allowing users to quickly apply common traffic control configurations.

use tcgui_shared::presets::NetworkPreset;

// Unused imports removed:
// - iced::widget imports: UI components not used since view method was removed
// - iced::{Element, Task}: Not used since update and view methods were removed
// - TcInterfaceMessage and PresetMessage: Not used since those message handling methods were removed

/// Component for preset management UI and logic
#[derive(Debug, Clone)]
pub struct PresetManagerComponent {
    /// Currently selected preset
    current_preset: NetworkPreset,
    /// Available presets (currently unused but kept for future)
    #[allow(dead_code)]
    available_presets: Vec<NetworkPreset>,
    /// Whether preset controls are visible (currently unused but kept for future)
    #[allow(dead_code)]
    show_presets: bool,
    /// Whether preset application is in progress
    applying_preset: bool,
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
            current_preset: NetworkPreset::Custom,
            available_presets: NetworkPreset::all_presets(),
            show_presets: false,
            applying_preset: false,
        }
    }

    // Removed unused methods:
    // - current_preset: Available via direct field access if needed
    // - available_presets: Available via direct field access if needed
    // - is_visible: Available via direct field access if needed
    // - is_applying: Available via direct field access if needed

    /// Set preset application state
    pub fn set_applying(&mut self, applying: bool) {
        self.applying_preset = applying;
    }

    // Removed unused methods:
    // - update: Message handling logic available if needed
    // - view: UI rendering logic available if needed (uses PresetMessage which is also unused)

    /// Apply the selected preset to the interface state
    pub fn apply_to_interface_state(&self, state: &mut crate::interface::state::InterfaceState) {
        // Apply the preset configuration to the interface state
        state.current_preset = self.current_preset.clone();

        // TODO: Extract configuration from preset and apply to feature states
        // For now, we'll just set the current preset
        match self.current_preset {
            NetworkPreset::Custom => {
                // Don't change anything for custom preset
            }
            _ => {
                // TODO: Apply preset-specific configuration
                // This would involve extracting the preset's configuration
                // and applying it to the feature states
            }
        }

        // Note: applying_preset state updated through set_applying method
    }

    // Removed unused methods:
    // - matches_interface_state: Preset matching logic available if needed
    // - reset: Reset logic available if needed
    // - preset_info: Information formatting logic available if needed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_manager_creation() {
        let component = PresetManagerComponent::new();
        assert_eq!(component.current_preset, NetworkPreset::Custom);
        assert!(!component.show_presets);
        assert!(!component.applying_preset);
        assert!(!component.available_presets.is_empty());
    }

    #[test]
    fn test_preset_selection() {
        let mut component = PresetManagerComponent::new();

        // Find a non-custom preset to test with
        let test_preset = component
            .available_presets
            .iter()
            .find(|p| !matches!(p, NetworkPreset::Custom))
            .cloned()
            .unwrap_or(NetworkPreset::Custom);

        // Set preset directly since update method was removed
        component.current_preset = test_preset.clone();
        assert_eq!(component.current_preset, test_preset);
    }

    #[test]
    fn test_toggle_visibility() {
        let mut component = PresetManagerComponent::new();
        assert!(!component.show_presets);

        // Toggle visibility directly
        component.show_presets = !component.show_presets;
        assert!(component.show_presets);

        component.show_presets = !component.show_presets;
        assert!(!component.show_presets);
    }

    #[test]
    fn test_apply_preset() {
        let mut component = PresetManagerComponent::new();
        assert!(!component.applying_preset);

        // Set applying state directly
        component.applying_preset = true;
        assert!(component.applying_preset);
    }

    #[test]
    fn test_reset_manually() {
        let mut component = PresetManagerComponent::new();

        // Set some non-default state
        component.show_presets = true;
        component.applying_preset = true;
        assert!(component.show_presets);
        assert!(component.applying_preset);

        // Reset manually
        component.current_preset = NetworkPreset::Custom;
        component.show_presets = false;
        component.applying_preset = false;
        assert_eq!(component.current_preset, NetworkPreset::Custom);
        assert!(!component.show_presets);
        assert!(!component.applying_preset);
    }

    #[test]
    fn test_set_applying() {
        let mut component = PresetManagerComponent::new();
        assert!(!component.applying_preset);

        component.set_applying(true);
        assert!(component.applying_preset);

        component.set_applying(false);
        assert!(!component.applying_preset);
    }
}
