//! Base interface module with core logic and component coordination.
//!
//! This module provides the main TcInterface component that coordinates all
//! feature-specific components while maintaining the same external API as
//! the original monolithic interface.

use iced::widget::{checkbox, column, row, slider, text};
use iced::{Color, Element, Task};
use tcgui_shared::NetworkBandwidthStats;

use super::state::InterfaceState;
use crate::messages::TcInterfaceMessage;
// Component message imports removed - using TcInterfaceMessage directly
use super::display::{BandwidthDisplayComponent, StatusDisplayComponent};
use super::preset::PresetManagerComponent;

/// Main network interface component for traffic control management.
///
/// This is the refactored version of the original TcInterface that coordinates
/// multiple feature-specific components while maintaining the same external API.
#[derive(Clone)]
pub struct TcInterface {
    /// Centralized state management
    state: InterfaceState,
    /// Bandwidth display component
    bandwidth_display: BandwidthDisplayComponent,
    /// Status display component
    status_display: StatusDisplayComponent,
    /// Preset management component
    preset_manager: PresetManagerComponent,
}

impl TcInterface {
    /// Creates a new TC interface component with default settings.
    ///
    /// # Arguments
    ///
    /// * `iface` - Interface name (will be converted to String)
    ///
    /// # Returns
    ///
    /// A new `TcInterface` with default configuration and disabled features.
    pub fn new(iface: impl ToString) -> Self {
        Self {
            state: InterfaceState::new(iface.to_string()),
            bandwidth_display: BandwidthDisplayComponent::new(),
            status_display: StatusDisplayComponent::new(),
            preset_manager: PresetManagerComponent::new(),
        }
    }

    /// Update the interface component with a message.
    ///
    /// This method routes messages to appropriate components and handles
    /// cross-component coordination.
    pub fn update(&mut self, message: TcInterfaceMessage) -> Task<TcInterfaceMessage> {
        // For now, we'll maintain compatibility with the old message system
        // while gradually migrating to the new modular approach
        match message {
            TcInterfaceMessage::LossChanged(v) => {
                println!("DEBUG: LossChanged slider moved to: {}", v);
                self.state.features.loss.config.percentage = v;
                // Auto-apply immediately
                self.state.applying = true;
                self.state.add_status_message(
                    format!(
                        "Updating: dev={} loss={}% corr={}%",
                        self.state.name, v, self.state.features.loss.config.correlation
                    ),
                    true,
                );
                Task::none()
            }
            TcInterfaceMessage::CorrelationChanged(v) => {
                println!("DEBUG: CorrelationChanged slider moved to: {}", v);
                self.state.features.loss.config.correlation = v;
                // Auto-apply immediately
                self.state.applying = true;
                self.state.add_status_message(
                    format!(
                        "Updating: dev={} loss={}% corr={}%",
                        self.state.name, self.state.features.loss.config.percentage, v
                    ),
                    true,
                );
                Task::none()
            }
            TcInterfaceMessage::LossToggled(enabled) => {
                if enabled {
                    self.state.features.loss.enable();
                } else {
                    self.state.features.loss.disable();
                }
                Task::none()
            }
            TcInterfaceMessage::InterfaceToggled(enabled) => {
                self.state.interface_enabled = enabled;
                self.state.applying_interface_state = true;
                self.state.add_status_message(
                    format!(
                        "{} interface: {}",
                        if enabled { "Enabling" } else { "Disabling" },
                        self.state.name
                    ),
                    true,
                );
                Task::none()
            }
            // Delay-related messages
            TcInterfaceMessage::DelayChanged(v) => {
                println!("DEBUG: DelayChanged slider moved to: {}", v);
                self.state.features.delay.config.base_ms = v;
                if self.state.features.delay.enabled {
                    self.state.applying = true;
                    self.state
                        .add_status_message(format!("Updating delay: {}ms", v), true);
                }
                Task::none()
            }
            TcInterfaceMessage::DelayJitterChanged(v) => {
                self.state.features.delay.config.jitter_ms = v;
                if self.state.features.delay.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            TcInterfaceMessage::DelayCorrelationChanged(v) => {
                self.state.features.delay.config.correlation = v;
                if self.state.features.delay.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            TcInterfaceMessage::DelayToggled(enabled) => {
                if enabled {
                    self.state.features.delay.enable();
                } else {
                    self.state.features.delay.disable();
                }
                Task::none()
            }
            // Feature toggle messages (unit type handlers)
            TcInterfaceMessage::DuplicateToggled(_) => {
                // Toggle duplicate feature
                if self.state.features.duplicate.enabled {
                    self.state.features.duplicate.disable();
                } else {
                    self.state.features.duplicate.enable();
                }
                Task::none()
            }
            TcInterfaceMessage::ReorderToggled(_) => {
                // Toggle reorder feature
                if self.state.features.reorder.enabled {
                    self.state.features.reorder.disable();
                } else {
                    self.state.features.reorder.enable();
                }
                Task::none()
            }
            TcInterfaceMessage::CorruptToggled(_) => {
                // Toggle corrupt feature
                if self.state.features.corrupt.enabled {
                    self.state.features.corrupt.disable();
                } else {
                    self.state.features.corrupt.enable();
                }
                Task::none()
            }
            TcInterfaceMessage::RateLimitToggled(_) => {
                // Toggle rate limit feature
                if self.state.features.rate_limit.enabled {
                    self.state.features.rate_limit.disable();
                } else {
                    self.state.features.rate_limit.enable();
                }
                Task::none()
            }
            // Duplicate parameter messages
            TcInterfaceMessage::DuplicatePercentageChanged(v) => {
                println!("DEBUG: DuplicatePercentageChanged slider moved to: {}", v);
                self.state.features.duplicate.config.percentage = v;
                // Auto-enable duplicate checkbox when meaningful value is set from backend
                if v > 0.0 && !self.state.features.duplicate.enabled {
                    self.state.features.duplicate.enable();
                }
                if self.state.features.duplicate.enabled {
                    self.state.applying = true;
                    self.state
                        .add_status_message(format!("Updating duplicate: {}%", v), true);
                }
                Task::none()
            }
            TcInterfaceMessage::DuplicateCorrelationChanged(v) => {
                println!("DEBUG: DuplicateCorrelationChanged slider moved to: {}", v);
                self.state.features.duplicate.config.correlation = v;
                if self.state.features.duplicate.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            // Reorder parameter messages
            TcInterfaceMessage::ReorderPercentageChanged(v) => {
                println!("DEBUG: ReorderPercentageChanged slider moved to: {}", v);
                self.state.features.reorder.config.percentage = v;
                // Auto-enable reorder checkbox when meaningful value is set from backend
                if v > 0.0 && !self.state.features.reorder.enabled {
                    self.state.features.reorder.enable();
                }
                if self.state.features.reorder.enabled {
                    self.state.applying = true;
                    self.state
                        .add_status_message(format!("Updating reorder: {}%", v), true);
                }
                Task::none()
            }
            TcInterfaceMessage::ReorderCorrelationChanged(v) => {
                println!("DEBUG: ReorderCorrelationChanged slider moved to: {}", v);
                self.state.features.reorder.config.correlation = v;
                if self.state.features.reorder.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            TcInterfaceMessage::ReorderGapChanged(v) => {
                println!("DEBUG: ReorderGapChanged slider moved to: {}", v);
                self.state.features.reorder.config.gap = v;
                if self.state.features.reorder.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            // Corrupt parameter messages
            TcInterfaceMessage::CorruptPercentageChanged(v) => {
                println!("DEBUG: CorruptPercentageChanged slider moved to: {}", v);
                self.state.features.corrupt.config.percentage = v;
                // Auto-enable corrupt checkbox when meaningful value is set from backend
                if v > 0.0 && !self.state.features.corrupt.enabled {
                    self.state.features.corrupt.enable();
                }
                if self.state.features.corrupt.enabled {
                    self.state.applying = true;
                    self.state
                        .add_status_message(format!("Updating corrupt: {}%", v), true);
                }
                Task::none()
            }
            TcInterfaceMessage::CorruptCorrelationChanged(v) => {
                println!("DEBUG: CorruptCorrelationChanged slider moved to: {}", v);
                self.state.features.corrupt.config.correlation = v;
                if self.state.features.corrupt.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            // Rate limit parameter messages
            TcInterfaceMessage::RateLimitChanged(v) => {
                println!("DEBUG: RateLimitChanged slider moved to: {}", v);
                self.state.features.rate_limit.config.rate_kbps = v;
                // Auto-enable rate limit checkbox when meaningful value is set from backend
                if v > 0 && !self.state.features.rate_limit.enabled {
                    self.state.features.rate_limit.enable();
                }
                if self.state.features.rate_limit.enabled {
                    self.state.applying = true;
                    self.state
                        .add_status_message(format!("Updating rate limit: {} kbps", v), true);
                }
                Task::none()
            }
            // Preset-related messages
            TcInterfaceMessage::PresetSelected(preset) => {
                self.state.current_preset = preset;
                self.preset_manager.set_applying(false); // Reset applying state
                Task::none()
            }
            TcInterfaceMessage::ApplyPreset => {
                // Apply the selected preset to the interface state
                self.preset_manager
                    .apply_to_interface_state(&mut self.state);
                // Mark as applying
                self.state.applying = true;
                self.state.add_status_message(
                    format!(
                        "Applying preset: {}",
                        self.state.current_preset.display_name()
                    ),
                    true,
                );
                Task::none()
            }
            TcInterfaceMessage::TogglePresets => {
                // Handled internally by preset manager
                Task::none()
            }
        }
    }

    /// Render the complete interface view
    pub fn view(&self) -> Element<'_, TcInterfaceMessage> {
        let main_row = self.render_main_row();
        let expandable_rows = self.render_expandable_features();

        column![main_row, expandable_rows].spacing(4).into()
    }

    /// Render the main interface row with core controls
    fn render_main_row(&self) -> Element<'_, TcInterfaceMessage> {
        // Colors
        let text_primary = Color::from_rgb(0.1, 0.1, 0.1);
        let _text_secondary = Color::from_rgb(0.4, 0.4, 0.4);

        // Interface name and icon
        let interface_icon = if self.state.is_up() {
            if self.state.has_tc_qdisc() {
                "🔧"
            } else {
                "📡"
            }
        } else {
            "⚫"
        };

        let interface_name = text(format!("{} {}", interface_icon, self.state.name))
            .size(14)
            .style(move |_| text::Style {
                color: Some(text_primary),
            });

        // Core checkboxes
        let interface_checkbox = checkbox(self.state.interface_enabled)
            .label("ON")
            .on_toggle(TcInterfaceMessage::InterfaceToggled)
            .text_size(12);

        // Feature toggles (compact checkboxes)
        let feature_toggles = self.render_feature_toggles();

        // Bandwidth display
        let bandwidth_display = self.render_bandwidth_display();

        // Status display
        let status_display = self.render_status_display();

        row![
            interface_name,
            interface_checkbox,
            feature_toggles,
            bandwidth_display,
            status_display
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render feature toggle checkboxes
    fn render_feature_toggles(&self) -> Element<'_, TcInterfaceMessage> {
        row![
            // Loss is always visible as it was in the original main row
            checkbox(self.state.features.loss.enabled)
                .label("LSS")
                .text_size(12)
                .on_toggle(TcInterfaceMessage::LossToggled),
            checkbox(self.state.features.delay.enabled)
                .label("DLY")
                .text_size(12)
                .on_toggle(TcInterfaceMessage::DelayToggled),
            checkbox(self.state.features.duplicate.enabled)
                .label("DUP")
                .text_size(12)
                .on_toggle(|_| TcInterfaceMessage::DuplicateToggled(())),
            checkbox(self.state.features.reorder.enabled)
                .label("RO")
                .text_size(12)
                .on_toggle(|_| TcInterfaceMessage::ReorderToggled(())),
            checkbox(self.state.features.corrupt.enabled)
                .label("CR")
                .text_size(12)
                .on_toggle(|_| TcInterfaceMessage::CorruptToggled(())),
            checkbox(self.state.features.rate_limit.enabled)
                .label("RL")
                .text_size(12)
                .on_toggle(|_| TcInterfaceMessage::RateLimitToggled(())),
        ]
        .spacing(4)
        .into()
    }

    /// Render bandwidth display
    fn render_bandwidth_display(&self) -> Element<'_, TcInterfaceMessage> {
        self.bandwidth_display.view()
    }

    /// Render status indicator
    fn render_status_display(&self) -> Element<'_, TcInterfaceMessage> {
        self.status_display.view()
    }

    /// Render expandable feature rows with parameter sliders
    fn render_expandable_features(&self) -> Element<'_, TcInterfaceMessage> {
        let mut feature_rows = Vec::new();

        // Loss feature controls
        if self.state.features.loss.enabled {
            let loss_controls = self.render_loss_controls();
            feature_rows.push(loss_controls);
        }

        // Delay feature controls
        if self.state.features.delay.enabled {
            let delay_controls = self.render_delay_controls();
            feature_rows.push(delay_controls);
        }

        // Duplicate feature controls
        if self.state.features.duplicate.enabled {
            let duplicate_controls = self.render_duplicate_controls();
            feature_rows.push(duplicate_controls);
        }

        // Reorder feature controls
        if self.state.features.reorder.enabled {
            let reorder_controls = self.render_reorder_controls();
            feature_rows.push(reorder_controls);
        }

        // Corrupt feature controls
        if self.state.features.corrupt.enabled {
            let corrupt_controls = self.render_corrupt_controls();
            feature_rows.push(corrupt_controls);
        }

        // Rate limit feature controls
        if self.state.features.rate_limit.enabled {
            let rate_limit_controls = self.render_rate_limit_controls();
            feature_rows.push(rate_limit_controls);
        }

        column(feature_rows).spacing(8).padding(4).into()
    }

    /// Render loss feature controls with sliders
    fn render_loss_controls(&self) -> Element<'_, TcInterfaceMessage> {
        let loss_config = &self.state.features.loss.config;

        row![
            text("Loss:").size(12).width(50),
            slider(
                0.0..=100.0,
                loss_config.percentage,
                TcInterfaceMessage::LossChanged
            )
            .width(120)
            .step(0.1),
            text(format!("{:.1}%", loss_config.percentage))
                .size(12)
                .width(50),
            text("Corr:").size(12).width(40),
            slider(
                0.0..=100.0,
                loss_config.correlation,
                TcInterfaceMessage::CorrelationChanged
            )
            .width(100)
            .step(0.1),
            text(format!("{:.1}%", loss_config.correlation))
                .size(12)
                .width(50),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render duplicate feature controls with sliders
    fn render_duplicate_controls(&self) -> Element<'_, TcInterfaceMessage> {
        let duplicate_config = &self.state.features.duplicate.config;

        row![
            text("Duplicate:").size(12).width(80),
            slider(
                0.0..=100.0,
                duplicate_config.percentage,
                TcInterfaceMessage::DuplicatePercentageChanged
            )
            .width(120)
            .step(0.1),
            text(format!("{:.1}%", duplicate_config.percentage))
                .size(12)
                .width(50),
            text("Corr:").size(12).width(40),
            slider(
                0.0..=100.0,
                duplicate_config.correlation,
                TcInterfaceMessage::DuplicateCorrelationChanged
            )
            .width(100)
            .step(0.1),
            text(format!("{:.1}%", duplicate_config.correlation))
                .size(12)
                .width(50),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render reorder feature controls with sliders
    fn render_reorder_controls(&self) -> Element<'_, TcInterfaceMessage> {
        let reorder_config = &self.state.features.reorder.config;

        row![
            text("Reorder:").size(12).width(60),
            slider(
                0.0..=100.0,
                reorder_config.percentage,
                TcInterfaceMessage::ReorderPercentageChanged
            )
            .width(100)
            .step(0.1),
            text(format!("{:.1}%", reorder_config.percentage))
                .size(12)
                .width(50),
            text("Gap:").size(12).width(35),
            slider(1.0..=10.0, reorder_config.gap as f32, |v| {
                TcInterfaceMessage::ReorderGapChanged(v as u32)
            })
            .width(80)
            .step(1.0),
            text(format!("{}", reorder_config.gap)).size(12).width(35),
            text("Corr:").size(12).width(40),
            slider(
                0.0..=100.0,
                reorder_config.correlation,
                TcInterfaceMessage::ReorderCorrelationChanged
            )
            .width(80)
            .step(0.1),
            text(format!("{:.1}%", reorder_config.correlation))
                .size(12)
                .width(50),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render corrupt feature controls with sliders
    fn render_corrupt_controls(&self) -> Element<'_, TcInterfaceMessage> {
        let corrupt_config = &self.state.features.corrupt.config;

        row![
            text("Corrupt:").size(12).width(70),
            slider(
                0.0..=100.0,
                corrupt_config.percentage,
                TcInterfaceMessage::CorruptPercentageChanged
            )
            .width(120)
            .step(0.1),
            text(format!("{:.1}%", corrupt_config.percentage))
                .size(12)
                .width(50),
            text("Corr:").size(12).width(40),
            slider(
                0.0..=100.0,
                corrupt_config.correlation,
                TcInterfaceMessage::CorruptCorrelationChanged
            )
            .width(100)
            .step(0.1),
            text(format!("{:.1}%", corrupt_config.correlation))
                .size(12)
                .width(50),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render rate limit feature controls with sliders
    fn render_rate_limit_controls(&self) -> Element<'_, TcInterfaceMessage> {
        let rate_config = &self.state.features.rate_limit.config;

        row![
            text("Rate Limit:").size(12).width(80),
            slider(1.0..=1000000.0, rate_config.rate_kbps as f32, |v| {
                TcInterfaceMessage::RateLimitChanged(v as u32)
            })
            .width(150)
            .step(1.0),
            text(format!("{} kbps", rate_config.rate_kbps))
                .size(12)
                .width(80),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render delay feature controls with sliders
    fn render_delay_controls(&self) -> Element<'_, TcInterfaceMessage> {
        let delay_config = &self.state.features.delay.config;

        row![
            text("Delay:").size(12).width(50),
            slider(
                0.0..=5000.0,
                delay_config.base_ms,
                TcInterfaceMessage::DelayChanged
            )
            .width(100)
            .step(1.0),
            text(format!("{:.0}ms", delay_config.base_ms))
                .size(12)
                .width(50),
            text("Jitter:").size(12).width(50),
            slider(
                0.0..=1000.0,
                delay_config.jitter_ms,
                TcInterfaceMessage::DelayJitterChanged
            )
            .width(100)
            .step(1.0),
            text(format!("{:.0}ms", delay_config.jitter_ms))
                .size(12)
                .width(50),
            text("Corr:").size(12).width(40),
            slider(
                0.0..=100.0,
                delay_config.correlation,
                TcInterfaceMessage::DelayCorrelationChanged
            )
            .width(80)
            .step(0.1),
            text(format!("{:.1}%", delay_config.correlation))
                .size(12)
                .width(50),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    // Public API methods to maintain compatibility

    /// Update bandwidth statistics
    pub fn update_bandwidth_stats(&mut self, stats: NetworkBandwidthStats) {
        self.state.update_bandwidth_stats(stats.clone());
        self.bandwidth_display.update_stats(stats);
    }

    // Removed unused methods that are not called:
    // - update_interface_state: Can use update_from_backend if needed
    // - sync_status_display: Internal method, status is managed by components
    // - add_status_message: Not used in current implementation
    // - mark_config_applied: Not used in current implementation

    /// Get interface name (only used in tests)
    #[cfg(test)]
    pub fn interface_name(&self) -> &str {
        &self.state.name
    }

    /// Check if interface is up (only used in tests)
    #[cfg(test)]
    pub fn is_up(&self) -> bool {
        self.state.is_up()
    }

    /// Check if TC qdisc is configured (only used in tests)
    #[cfg(test)]
    pub fn has_tc_qdisc(&self) -> bool {
        self.state.has_tc_qdisc()
    }

    /// Update from backend interface information (compatibility method)
    pub fn update_from_backend(&mut self, interface: &tcgui_shared::NetworkInterface) {
        self.state
            .set_interface_state(interface.is_up, interface.has_tc_qdisc);
    }

    /// Get bandwidth stats (compatibility method)
    pub fn bandwidth_stats(&self) -> Option<&NetworkBandwidthStats> {
        self.state.bandwidth_stats.as_ref()
    }

    /// Get loss value (compatibility method)
    pub fn loss(&self) -> f32 {
        self.state.features.loss.config.percentage
    }

    /// Get correlation value (compatibility method)
    pub fn correlation_value(&self) -> f32 {
        self.state.features.loss.config.correlation
    }

    /// Check if loss is enabled (compatibility method)
    pub fn loss_enabled(&self) -> bool {
        self.state.features.loss.enabled
    }

    /// Check if delay is enabled (compatibility method)
    pub fn delay_enabled(&self) -> bool {
        self.state.features.delay.enabled
    }

    /// Get delay value in ms (compatibility method)
    pub fn delay_ms(&self) -> f32 {
        self.state.features.delay.config.base_ms
    }

    /// Get delay jitter value in ms (compatibility method)
    pub fn delay_jitter_ms(&self) -> f32 {
        self.state.features.delay.config.jitter_ms
    }

    /// Get delay correlation value (compatibility method)
    pub fn delay_correlation(&self) -> f32 {
        self.state.features.delay.config.correlation
    }

    /// Check if duplicate is enabled (compatibility method)
    pub fn duplicate_enabled(&self) -> bool {
        self.state.features.duplicate.enabled
    }

    /// Get duplicate percentage value (compatibility method)
    pub fn duplicate_percentage(&self) -> f32 {
        self.state.features.duplicate.config.percentage
    }

    /// Get duplicate correlation value (compatibility method)
    pub fn duplicate_correlation(&self) -> f32 {
        self.state.features.duplicate.config.correlation
    }

    /// Check if reorder is enabled (compatibility method)
    pub fn reorder_enabled(&self) -> bool {
        self.state.features.reorder.enabled
    }

    /// Get reorder percentage value (compatibility method)
    pub fn reorder_percentage(&self) -> f32 {
        self.state.features.reorder.config.percentage
    }

    /// Get reorder correlation value (compatibility method)
    pub fn reorder_correlation(&self) -> f32 {
        self.state.features.reorder.config.correlation
    }

    /// Get reorder gap value (compatibility method)
    pub fn reorder_gap(&self) -> u32 {
        self.state.features.reorder.config.gap
    }

    /// Check if corrupt is enabled (compatibility method)
    pub fn corrupt_enabled(&self) -> bool {
        self.state.features.corrupt.enabled
    }

    /// Get corrupt percentage value (compatibility method)
    pub fn corrupt_percentage(&self) -> f32 {
        self.state.features.corrupt.config.percentage
    }

    /// Get corrupt correlation value (compatibility method)
    pub fn corrupt_correlation(&self) -> f32 {
        self.state.features.corrupt.config.correlation
    }

    /// Check if rate limit is enabled (compatibility method)
    pub fn rate_limit_enabled(&self) -> bool {
        self.state.features.rate_limit.enabled
    }

    /// Get rate limit value in kbps (compatibility method)
    pub fn rate_limit_kbps(&self) -> u32 {
        self.state.features.rate_limit.config.rate_kbps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_creation() {
        let interface = TcInterface::new("eth0");
        assert_eq!(interface.interface_name(), "eth0");
        assert!(!interface.is_up());
        assert!(!interface.has_tc_qdisc());
    }

    #[test]
    fn test_loss_parameter_update() {
        let mut interface = TcInterface::new("eth0");
        let _ = interface.update(TcInterfaceMessage::LossChanged(15.0));
        assert_eq!(interface.state.features.loss.config.percentage, 15.0);
    }

    #[test]
    fn test_loss_feature_toggle() {
        let mut interface = TcInterface::new("eth0");

        // Initially disabled with default percentage
        assert!(!interface.state.features.loss.enabled);
        assert_eq!(interface.state.features.loss.config.percentage, 0.0);

        // Toggle on - should enable but keep default percentage
        let _ = interface.update(TcInterfaceMessage::LossToggled(true));
        assert!(interface.state.features.loss.enabled);
        assert_eq!(interface.state.features.loss.config.percentage, 0.0);

        // Toggle off - should disable but keep percentage value
        let _ = interface.update(TcInterfaceMessage::LossToggled(false));
        assert!(!interface.state.features.loss.enabled);
        assert_eq!(interface.state.features.loss.config.percentage, 0.0);
    }
}
