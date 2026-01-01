//! Base interface module with core logic and component coordination.
//!
//! This module provides the main TcInterface component that coordinates all
//! feature-specific components while maintaining the same external API as
//! the original monolithic interface.

use iced::widget::{checkbox, column, container, row, slider, text, tooltip};
use iced::{Background, Color, Element, Task};
use iced_anim::Animation;
use std::time::Duration;
use tcgui_shared::NetworkBandwidthStats;
use tcgui_shared::presets::PresetList;

use super::state::InterfaceState;
use crate::bandwidth_chart::bandwidth_chart_view;
use crate::bandwidth_history::BandwidthHistory;
use crate::icons::Icon;
use crate::messages::TcInterfaceMessage;
use crate::theme::Theme;
use crate::view::{scaled, scaled_spacing};
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
                tracing::debug!("LossChanged slider moved to: {}", v);
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
                tracing::debug!("CorrelationChanged slider moved to: {}", v);
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
                tracing::debug!("DelayChanged slider moved to: {}", v);
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
                tracing::debug!("DuplicatePercentageChanged slider moved to: {}", v);
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
                tracing::debug!("DuplicateCorrelationChanged slider moved to: {}", v);
                self.state.features.duplicate.config.correlation = v;
                if self.state.features.duplicate.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            // Reorder parameter messages
            TcInterfaceMessage::ReorderPercentageChanged(v) => {
                tracing::debug!("ReorderPercentageChanged slider moved to: {}", v);
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
                tracing::debug!("ReorderCorrelationChanged slider moved to: {}", v);
                self.state.features.reorder.config.correlation = v;
                if self.state.features.reorder.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            TcInterfaceMessage::ReorderGapChanged(v) => {
                tracing::debug!("ReorderGapChanged slider moved to: {}", v);
                self.state.features.reorder.config.gap = v;
                if self.state.features.reorder.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            // Corrupt parameter messages
            TcInterfaceMessage::CorruptPercentageChanged(v) => {
                tracing::debug!("CorruptPercentageChanged slider moved to: {}", v);
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
                tracing::debug!("CorruptCorrelationChanged slider moved to: {}", v);
                self.state.features.corrupt.config.correlation = v;
                if self.state.features.corrupt.enabled {
                    self.state.applying = true;
                }
                Task::none()
            }
            // Rate limit parameter messages
            TcInterfaceMessage::RateLimitChanged(v) => {
                tracing::debug!("RateLimitChanged slider moved to: {}", v);
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
            // Preset messages
            TcInterfaceMessage::PresetSelected(preset) => {
                tracing::debug!("Preset selected: {:?}", preset);
                let preset_name = preset.name.clone();
                if self.preset_manager.apply_preset(&preset, &mut self.state) {
                    self.state
                        .add_status_message(format!("Applying preset: {}", preset_name), false);
                }
                Task::none()
            }
            TcInterfaceMessage::TogglePresetDropdown => {
                self.preset_manager.toggle_dropdown();
                Task::none()
            }
            TcInterfaceMessage::ClearAllFeatures => {
                tracing::debug!("Clearing all features");
                self.preset_manager.clear_all_features(&mut self.state);
                self.state
                    .add_status_message("Clearing all TC features".to_string(), false);
                Task::none()
            }
            TcInterfaceMessage::ToggleChart => {
                self.state.chart_expanded = !self.state.chart_expanded;
                Task::none()
            }
            TcInterfaceMessage::AnimateTcIntensity(event) => {
                self.state.tc_active_intensity.update(event);
                Task::none()
            }
        }
    }

    /// Render the complete interface view
    pub fn view<'a>(
        &'a self,
        preset_list: &'a PresetList,
        theme: &'a Theme,
        zoom: f32,
        bandwidth_history: Option<&'a BandwidthHistory>,
    ) -> Element<'a, TcInterfaceMessage> {
        let main_row = self.render_main_row(preset_list, theme, zoom);
        let expandable_rows = self.render_expandable_features(theme, zoom);

        // Build content column with optional chart
        let content = if self.state.chart_expanded {
            let chart_height = scaled(80, zoom);
            let dark_mode = theme.is_dark();
            let chart_element = bandwidth_chart_view(bandwidth_history, chart_height, dark_mode);

            column![main_row, expandable_rows, chart_element].spacing(scaled_spacing(4, zoom))
        } else {
            column![main_row, expandable_rows].spacing(scaled_spacing(4, zoom))
        };

        // Get current animated TC intensity (0.0 = inactive, 1.0 = active)
        let intensity = *self.state.tc_active_intensity.value();

        // Interpolate background color based on intensity and theme
        let tc_active = theme.colors.tc_active;
        let bg_color = Color::from_rgba(tc_active.r, tc_active.g, tc_active.b, 0.1 * intensity);

        // Wrap content in animated container
        let styled_container =
            container(content)
                .padding(scaled_spacing(8, zoom))
                .style(move |_| iced::widget::container::Style {
                    background: Some(Background::Color(bg_color)),
                    border: iced::Border {
                        radius: 8.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..Default::default()
                });

        // Wrap in Animation widget to drive the animation
        Animation::new(&self.state.tc_active_intensity, styled_container)
            .on_update(TcInterfaceMessage::AnimateTcIntensity)
            .into()
    }

    /// Render the main interface row with core controls
    fn render_main_row<'a>(
        &'a self,
        preset_list: &'a PresetList,
        theme: &'a Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        use iced::Length;
        use iced::widget::container;

        // Colors from theme
        let text_primary = theme.colors.text_primary;

        // Interface name and icon
        let interface_icon = if self.state.is_up() {
            if self.state.has_tc_qdisc() {
                Icon::Wrench
            } else {
                Icon::Radio
            }
        } else {
            Icon::Circle
        };

        let interface_name = row![
            interface_icon.svg_sized_colored(scaled(14, zoom), text_primary),
            text(format!(" {}", self.state.name))
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(text_primary),
                })
        ]
        .align_y(iced::Alignment::Center);

        // Core checkboxes - use row with styled text for theme support
        let interface_checkbox = row![
            checkbox(self.state.interface_enabled).on_toggle(TcInterfaceMessage::InterfaceToggled),
            text("ON")
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(text_primary)
                })
        ]
        .spacing(scaled_spacing(2, zoom));

        // Preset selector
        let preset_selector =
            self.preset_manager
                .view(preset_list, &self.state.current_preset_id, theme, zoom);

        // Feature toggles (compact checkboxes)
        let feature_toggles = self.render_feature_toggles(theme, zoom);

        // Bandwidth display
        let bandwidth_display = self.render_bandwidth_display(theme, zoom);

        // TC stats display (drops/packets when TC is active)
        let tc_stats_display = self.render_tc_stats_display(theme, zoom);

        // Status display
        let status_display = self.render_status_display(theme, zoom);

        // Use fixed-width containers for table-like alignment
        // Preset selector expands when open to show all options
        let preset_width = if self.preset_manager.is_expanded() {
            Length::Shrink // Let it expand to fit all buttons
        } else {
            Length::Fixed(130.0 * zoom)
        };

        row![
            container(interface_name)
                .width(Length::Fixed(120.0 * zoom))
                .align_y(iced::alignment::Vertical::Center),
            container(interface_checkbox)
                .width(Length::Fixed(50.0 * zoom))
                .align_y(iced::alignment::Vertical::Center),
            container(preset_selector)
                .width(preset_width)
                .align_y(iced::alignment::Vertical::Center),
            container(feature_toggles)
                .width(Length::Fixed(280.0 * zoom))
                .align_y(iced::alignment::Vertical::Center),
            container(bandwidth_display)
                .width(Length::Fixed(140.0 * zoom))
                .align_y(iced::alignment::Vertical::Center),
            container(tc_stats_display)
                .width(Length::Fixed(120.0 * zoom))
                .align_y(iced::alignment::Vertical::Center),
            container(status_display)
                .width(Length::Fill)
                .align_y(iced::alignment::Vertical::Center),
        ]
        .spacing(scaled_spacing(4, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render feature toggle checkboxes
    fn render_feature_toggles<'a>(
        &'a self,
        theme: &'a Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            // Loss: randomly drop packets
            tooltip(
                row![
                    checkbox(self.state.features.loss.enabled)
                        .on_toggle(TcInterfaceMessage::LossToggled),
                    text("LSS")
                        .size(scaled(12, zoom))
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        })
                ]
                .spacing(scaled_spacing(2, zoom)),
                text("Packet Loss: randomly drop packets at a specified rate"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            // Delay: add latency to packets
            tooltip(
                row![
                    checkbox(self.state.features.delay.enabled)
                        .on_toggle(TcInterfaceMessage::DelayToggled),
                    text("DLY")
                        .size(scaled(12, zoom))
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        })
                ]
                .spacing(scaled_spacing(2, zoom)),
                text("Delay: add latency with optional jitter"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            // Duplicate: send duplicate packets
            tooltip(
                row![
                    checkbox(self.state.features.duplicate.enabled)
                        .on_toggle(|_| TcInterfaceMessage::DuplicateToggled(())),
                    text("DUP")
                        .size(scaled(12, zoom))
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        })
                ]
                .spacing(scaled_spacing(2, zoom)),
                text("Duplicate: send duplicate copies of packets"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            // Reorder: change packet order
            tooltip(
                row![
                    checkbox(self.state.features.reorder.enabled)
                        .on_toggle(|_| TcInterfaceMessage::ReorderToggled(())),
                    text("RO")
                        .size(scaled(12, zoom))
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        })
                ]
                .spacing(scaled_spacing(2, zoom)),
                text("Reorder: change the order of packets"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            // Corrupt: introduce bit errors
            tooltip(
                row![
                    checkbox(self.state.features.corrupt.enabled)
                        .on_toggle(|_| TcInterfaceMessage::CorruptToggled(())),
                    text("CR")
                        .size(scaled(12, zoom))
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        })
                ]
                .spacing(scaled_spacing(2, zoom)),
                text("Corrupt: introduce random bit errors in packets"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            // Rate Limit: cap bandwidth
            tooltip(
                row![
                    checkbox(self.state.features.rate_limit.enabled)
                        .on_toggle(|_| TcInterfaceMessage::RateLimitToggled(())),
                    text("RL")
                        .size(scaled(12, zoom))
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        })
                ]
                .spacing(scaled_spacing(2, zoom)),
                text("Rate Limit: cap maximum bandwidth"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(4, zoom))
        .into()
    }

    /// Render bandwidth display with chart toggle
    fn render_bandwidth_display<'a>(
        &'a self,
        theme: &'a Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        use iced::widget::button;

        let bandwidth = self.bandwidth_display.view(theme, zoom);

        // Chart toggle button placed before bandwidth to avoid shifting
        let chart_icon = if self.state.chart_expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let icon_color = theme.colors.text_primary;
        let chart_button = button(chart_icon.svg_sized_colored(scaled(10, zoom), icon_color))
            .on_press(TcInterfaceMessage::ToggleChart)
            .padding(scaled_spacing(2, zoom));

        row![chart_button, bandwidth]
            .spacing(scaled_spacing(4, zoom))
            .align_y(iced::Alignment::Center)
            .into()
    }

    /// Format bytes per second with appropriate units
    fn format_bps(bps: u32) -> String {
        if bps >= 1_000_000_000 {
            format!("{:.1}G", bps as f64 / 1_000_000_000.0)
        } else if bps >= 1_000_000 {
            format!("{:.1}M", bps as f64 / 1_000_000.0)
        } else if bps >= 1_000 {
            format!("{:.0}K", bps as f64 / 1_000.0)
        } else if bps > 0 {
            format!("{}B", bps)
        } else {
            "0".to_string()
        }
    }

    /// Render TC qdisc statistics (drops/throughput) when TC is active
    fn render_tc_stats_display<'a>(
        &'a self,
        theme: &'a Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        // Only show stats when TC is configured
        if !self.state.has_tc_qdisc() {
            return row![].into();
        }

        let text_muted = theme.colors.text_muted;
        let error_color = theme.colors.error;
        let info_color = theme.colors.info;

        // Get queue stats (drops) and rate estimator (throughput)
        if let Some(queue_stats) = &self.state.tc_stats_queue {
            let drops = queue_stats.drops;

            // Show drops in error color if > 0, otherwise muted
            let drops_color = if drops > 0 { error_color } else { text_muted };

            // Format with compact display
            let drops_text = if drops >= 1_000_000 {
                format!("{}M", drops / 1_000_000)
            } else if drops >= 1_000 {
                format!("{}K", drops / 1_000)
            } else {
                format!("{}", drops)
            };

            // Get rate estimator if available (kernel-computed throughput)
            let rate_text = if let Some(rate_est) = &self.state.tc_stats_rate_est {
                Self::format_bps(rate_est.bps)
            } else {
                "--".to_string()
            };

            row![
                Icon::XCircle.svg_sized_colored(scaled(10, zoom), drops_color),
                text(drops_text)
                    .size(scaled(10, zoom))
                    .style(move |_| text::Style {
                        color: Some(drops_color)
                    }),
                text(" ").size(scaled(10, zoom)),
                Icon::Zap.svg_sized_colored(scaled(10, zoom), info_color),
                text(rate_text)
                    .size(scaled(10, zoom))
                    .style(move |_| text::Style {
                        color: Some(info_color)
                    }),
            ]
            .spacing(scaled_spacing(2, zoom))
            .align_y(iced::Alignment::Center)
            .into()
        } else {
            // TC active but no stats yet - show waiting indicator
            row![
                text("--")
                    .size(scaled(10, zoom))
                    .style(move |_| text::Style {
                        color: Some(text_muted)
                    }),
            ]
            .into()
        }
    }

    /// Render status indicator
    fn render_status_display<'a>(
        &'a self,
        theme: &'a Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        self.status_display.view(theme, zoom)
    }

    /// Render expandable feature rows with parameter sliders
    fn render_expandable_features<'a>(
        &'a self,
        theme: &'a Theme,
        zoom: f32,
    ) -> Element<'a, TcInterfaceMessage> {
        let mut feature_rows = Vec::new();

        // Loss feature controls
        if self.state.features.loss.enabled {
            let loss_controls = self.render_loss_controls(theme, zoom);
            feature_rows.push(loss_controls);
        }

        // Delay feature controls
        if self.state.features.delay.enabled {
            let delay_controls = self.render_delay_controls(theme, zoom);
            feature_rows.push(delay_controls);
        }

        // Duplicate feature controls
        if self.state.features.duplicate.enabled {
            let duplicate_controls = self.render_duplicate_controls(theme, zoom);
            feature_rows.push(duplicate_controls);
        }

        // Reorder feature controls
        if self.state.features.reorder.enabled {
            let reorder_controls = self.render_reorder_controls(theme, zoom);
            feature_rows.push(reorder_controls);
        }

        // Corrupt feature controls
        if self.state.features.corrupt.enabled {
            let corrupt_controls = self.render_corrupt_controls(theme, zoom);
            feature_rows.push(corrupt_controls);
        }

        // Rate limit feature controls
        if self.state.features.rate_limit.enabled {
            let rate_limit_controls = self.render_rate_limit_controls(theme, zoom);
            feature_rows.push(rate_limit_controls);
        }

        column(feature_rows)
            .spacing(scaled_spacing(8, zoom))
            .padding(scaled_spacing(4, zoom))
            .into()
    }

    /// Render loss feature controls with sliders
    fn render_loss_controls(&self, theme: &Theme, zoom: f32) -> Element<'_, TcInterfaceMessage> {
        let loss_config = &self.state.features.loss.config;
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            tooltip(
                row![
                    text("Loss:")
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        loss_config.percentage,
                        TcInterfaceMessage::LossChanged
                    )
                    .width(120.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", loss_config.percentage))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Percentage of packets to drop randomly"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Corr:")
                        .size(scaled(12, zoom))
                        .width(40.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        loss_config.correlation,
                        TcInterfaceMessage::CorrelationChanged
                    )
                    .width(100.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", loss_config.correlation))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("How much loss depends on previous packet (burst loss)"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render duplicate feature controls with sliders
    fn render_duplicate_controls(
        &self,
        theme: &Theme,
        zoom: f32,
    ) -> Element<'_, TcInterfaceMessage> {
        let duplicate_config = &self.state.features.duplicate.config;
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            tooltip(
                row![
                    text("Duplicate:")
                        .size(scaled(12, zoom))
                        .width(80.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        duplicate_config.percentage,
                        TcInterfaceMessage::DuplicatePercentageChanged
                    )
                    .width(120.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", duplicate_config.percentage))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Percentage of packets to duplicate"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Corr:")
                        .size(scaled(12, zoom))
                        .width(40.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        duplicate_config.correlation,
                        TcInterfaceMessage::DuplicateCorrelationChanged
                    )
                    .width(100.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", duplicate_config.correlation))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("How much duplication depends on previous packet"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render reorder feature controls with sliders
    fn render_reorder_controls(&self, theme: &Theme, zoom: f32) -> Element<'_, TcInterfaceMessage> {
        let reorder_config = &self.state.features.reorder.config;
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            tooltip(
                row![
                    text("Reorder:")
                        .size(scaled(12, zoom))
                        .width(60.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        reorder_config.percentage,
                        TcInterfaceMessage::ReorderPercentageChanged
                    )
                    .width(100.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", reorder_config.percentage))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Percentage of packets to reorder"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Gap:")
                        .size(scaled(12, zoom))
                        .width(35.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(1.0..=10.0, reorder_config.gap as f32, |v| {
                        TcInterfaceMessage::ReorderGapChanged(v as u32)
                    })
                    .width(80.0 * zoom)
                    .step(1.0),
                    text(format!("{}", reorder_config.gap))
                        .size(scaled(12, zoom))
                        .width(35.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Number of packets to delay before sending"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Corr:")
                        .size(scaled(12, zoom))
                        .width(40.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        reorder_config.correlation,
                        TcInterfaceMessage::ReorderCorrelationChanged
                    )
                    .width(80.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", reorder_config.correlation))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("How much reordering depends on previous packet"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render corrupt feature controls with sliders
    fn render_corrupt_controls(&self, theme: &Theme, zoom: f32) -> Element<'_, TcInterfaceMessage> {
        let corrupt_config = &self.state.features.corrupt.config;
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            tooltip(
                row![
                    text("Corrupt:")
                        .size(scaled(12, zoom))
                        .width(70.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        corrupt_config.percentage,
                        TcInterfaceMessage::CorruptPercentageChanged
                    )
                    .width(120.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", corrupt_config.percentage))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Percentage of packets with random bit errors"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Corr:")
                        .size(scaled(12, zoom))
                        .width(40.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        corrupt_config.correlation,
                        TcInterfaceMessage::CorruptCorrelationChanged
                    )
                    .width(100.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", corrupt_config.correlation))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("How much corruption depends on previous packet"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render rate limit feature controls with sliders
    fn render_rate_limit_controls(
        &self,
        theme: &Theme,
        zoom: f32,
    ) -> Element<'_, TcInterfaceMessage> {
        let rate_config = &self.state.features.rate_limit.config;
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            tooltip(
                row![
                    text("Rate Limit:")
                        .size(scaled(12, zoom))
                        .width(80.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(1.0..=1000000.0, rate_config.rate_kbps as f32, |v| {
                        TcInterfaceMessage::RateLimitChanged(v as u32)
                    })
                    .width(150.0 * zoom)
                    .step(1.0),
                    text(format!("{} kbps", rate_config.rate_kbps))
                        .size(scaled(12, zoom))
                        .width(80.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Maximum bandwidth in kilobits per second"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Render delay feature controls with sliders
    fn render_delay_controls(&self, theme: &Theme, zoom: f32) -> Element<'_, TcInterfaceMessage> {
        let delay_config = &self.state.features.delay.config;
        let text_color = theme.colors.text_primary;
        let tooltip_delay = Duration::from_millis(500);
        let tooltip_style = theme.tooltip_style();

        row![
            tooltip(
                row![
                    text("Delay:")
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=5000.0,
                        delay_config.base_ms,
                        TcInterfaceMessage::DelayChanged
                    )
                    .width(100.0 * zoom)
                    .step(1.0),
                    text(format!("{:.0}ms", delay_config.base_ms))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Base latency added to each packet in milliseconds"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Jitter:")
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=1000.0,
                        delay_config.jitter_ms,
                        TcInterfaceMessage::DelayJitterChanged
                    )
                    .width(100.0 * zoom)
                    .step(1.0),
                    text(format!("{:.0}ms", delay_config.jitter_ms))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("Random variation added to delay (delay +/- jitter)"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
            tooltip(
                row![
                    text("Corr:")
                        .size(scaled(12, zoom))
                        .width(40.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                    slider(
                        0.0..=100.0,
                        delay_config.correlation,
                        TcInterfaceMessage::DelayCorrelationChanged
                    )
                    .width(80.0 * zoom)
                    .step(0.1),
                    text(format!("{:.1}%", delay_config.correlation))
                        .size(scaled(12, zoom))
                        .width(50.0 * zoom)
                        .style(move |_| text::Style {
                            color: Some(text_color)
                        }),
                ]
                .spacing(scaled_spacing(4, zoom))
                .align_y(iced::Alignment::Center),
                text("How much delay depends on previous packet"),
                tooltip::Position::Top
            )
            .delay(tooltip_delay)
            .style(move |_| tooltip_style),
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center)
        .into()
    }

    // Public API methods to maintain compatibility

    /// Update bandwidth statistics
    pub fn update_bandwidth_stats(&mut self, stats: NetworkBandwidthStats) {
        self.state.update_bandwidth_stats(stats.clone());
        self.bandwidth_display.update_stats(stats);
    }

    /// Update TC qdisc statistics
    pub fn update_tc_statistics(
        &mut self,
        stats_basic: Option<tcgui_shared::TcStatsBasic>,
        stats_queue: Option<tcgui_shared::TcStatsQueue>,
        stats_rate_est: Option<tcgui_shared::TcStatsRateEst>,
    ) {
        self.state
            .update_tc_statistics(stats_basic, stats_queue, stats_rate_est);
    }

    // Removed unused methods that are not called:
    // - update_interface_state: Can use update_from_backend if needed
    // - sync_status_display: Internal method, status is managed by components
    // - add_status_message: Not used in current implementation
    // - mark_config_applied: Not used in current implementation

    /// Get interface name (for tests, use name() for general use)
    #[cfg(test)]
    pub fn interface_name(&self) -> &str {
        &self.state.name
    }

    /// Check if interface is up
    pub fn is_up(&self) -> bool {
        self.state.is_up()
    }

    /// Check if TC qdisc is configured
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

    /// Check if bandwidth chart is expanded (compatibility method)
    pub fn chart_expanded(&self) -> bool {
        self.state.chart_expanded
    }

    /// Get interface name
    pub fn name(&self) -> &str {
        &self.state.name
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
