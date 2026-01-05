//! Dual-control widget combining slider and NumberInput for numeric values.
//!
//! This module implements the synchronized dual-control pattern recommended
//! by UX best practices, allowing users to either:
//! - Drag a slider for quick visual adjustments
//! - Type an exact value in a NumberInput for precision
//!
//! Both controls use the same message type, so they stay synchronized
//! automatically through the Iced update cycle.

use iced::Element;
use iced::widget::{row, slider, text};
use iced_aw::NumberInput;
use std::ops::RangeInclusive;

use crate::messages::TcInterfaceMessage;
use crate::view::{scaled, scaled_spacing};

/// Configuration for a slider-input control
pub struct SliderInputConfig {
    /// Label text (e.g., "Loss:", "Delay:")
    pub label: &'static str,
    /// Label width in base pixels (will be scaled by zoom)
    pub label_width: f32,
    /// Slider width in base pixels
    pub slider_width: f32,
    /// NumberInput width in base pixels
    pub input_width: f32,
    /// Unit suffix (e.g., "%", "ms", "kbps")
    pub unit: &'static str,
}

impl SliderInputConfig {
    /// Create config for percentage values (0-100%)
    pub fn percentage(label: &'static str) -> Self {
        Self {
            label,
            label_width: 50.0,
            slider_width: 120.0,
            input_width: 55.0,
            unit: "%",
        }
    }

    /// Create config for correlation values
    pub fn correlation() -> Self {
        Self {
            label: "Corr:",
            label_width: 40.0,
            slider_width: 100.0,
            input_width: 55.0,
            unit: "%",
        }
    }

    /// Create config for millisecond values
    pub fn milliseconds(label: &'static str) -> Self {
        Self {
            label,
            label_width: 50.0,
            slider_width: 100.0,
            input_width: 55.0,
            unit: "ms",
        }
    }

    /// Create config for rate limit (kbps)
    pub fn rate_limit() -> Self {
        Self {
            label: "Rate:",
            label_width: 50.0,
            slider_width: 150.0,
            input_width: 80.0,
            unit: "kbps",
        }
    }

    /// Create config for gap values (small integers)
    pub fn gap() -> Self {
        Self {
            label: "Gap:",
            label_width: 35.0,
            slider_width: 80.0,
            input_width: 40.0,
            unit: "",
        }
    }

    /// Builder method to set custom label width
    pub fn with_label_width(mut self, width: f32) -> Self {
        self.label_width = width;
        self
    }

    /// Builder method to set custom slider width
    pub fn with_slider_width(mut self, width: f32) -> Self {
        self.slider_width = width;
        self
    }
}

/// Render a dual-control slider+NumberInput widget for f32 values
///
/// # Arguments
/// * `config` - Widget configuration (label, widths, unit)
/// * `value` - Current numeric value
/// * `range` - Valid range for the slider and input
/// * `step` - Step size for increments
/// * `on_change` - Message constructor for value changes (used by both slider and NumberInput)
/// * `text_color` - Color for text elements
/// * `zoom` - Current zoom level
pub fn slider_input_f32<'a>(
    config: &SliderInputConfig,
    value: f32,
    range: RangeInclusive<f32>,
    step: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    text_color: iced::Color,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let label_width = config.label_width * zoom;
    let slider_width = config.slider_width * zoom;
    let input_width = config.input_width * zoom;
    let unit_width = if config.unit.is_empty() {
        0.0
    } else {
        (config.unit.len() as f32 * 8.0 + 4.0) * zoom
    };

    let on_change_clone = on_change.clone();

    let number_input: Element<'a, TcInterfaceMessage> =
        NumberInput::new(&value, range.clone(), on_change_clone)
            .step(step)
            .width(input_width)
            .into();

    let mut content = row![
        text(config.label)
            .size(scaled(12, zoom))
            .width(label_width)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        slider(range, value, on_change)
            .width(slider_width)
            .step(step),
        number_input,
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(iced::Alignment::Center);

    if !config.unit.is_empty() {
        content = content.push(
            text(config.unit)
                .size(scaled(12, zoom))
                .width(unit_width)
                .style(move |_| iced::widget::text::Style {
                    color: Some(text_color),
                }),
        );
    }

    content.into()
}

/// Render a dual-control slider+NumberInput widget for u32 values
///
/// Similar to `slider_input_f32` but for unsigned integer values.
pub fn slider_input_u32<'a>(
    config: &SliderInputConfig,
    value: u32,
    range: RangeInclusive<u32>,
    step: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    text_color: iced::Color,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let label_width = config.label_width * zoom;
    let slider_width = config.slider_width * zoom;
    let input_width = config.input_width * zoom;
    let unit_width = if config.unit.is_empty() {
        0.0
    } else {
        (config.unit.len() as f32 * 8.0 + 4.0) * zoom
    };

    let on_change_clone = on_change.clone();

    // Convert u32 range to f32 for slider
    let slider_range = (*range.start() as f32)..=(*range.end() as f32);

    let number_input: Element<'a, TcInterfaceMessage> =
        NumberInput::new(&value, range, on_change_clone)
            .step(step)
            .width(input_width)
            .into();

    let mut content = row![
        text(config.label)
            .size(scaled(12, zoom))
            .width(label_width)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        slider(slider_range, value as f32, move |v| on_change(v as u32))
            .width(slider_width)
            .step(step as f32),
        number_input,
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(iced::Alignment::Center);

    if !config.unit.is_empty() {
        content = content.push(
            text(config.unit)
                .size(scaled(12, zoom))
                .width(unit_width)
                .style(move |_| iced::widget::text::Style {
                    color: Some(text_color),
                }),
        );
    }

    content.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentage_config() {
        let config = SliderInputConfig::percentage("Loss:");
        assert_eq!(config.label, "Loss:");
        assert_eq!(config.unit, "%");
    }

    #[test]
    fn test_config_builder() {
        let config = SliderInputConfig::percentage("Loss:")
            .with_label_width(80.0)
            .with_slider_width(200.0);
        assert_eq!(config.label_width, 80.0);
        assert_eq!(config.slider_width, 200.0);
    }
}
