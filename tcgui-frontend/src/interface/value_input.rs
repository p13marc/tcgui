//! Compact value input controls for TC parameters.
//!
//! This module provides a preset-based input pattern following UX best practices:
//! - **Preset chips**: Quick selection of common values
//! - **NumberInput**: For precise custom values
//! - **Feature cards**: Grouped controls in styled containers

use iced::widget::{Column, Row, button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use iced_aw::NumberInput;

use crate::messages::TcInterfaceMessage;
use crate::theme::Theme;
use crate::view::{scaled, scaled_spacing};

/// Colors extracted from theme for styling (avoids lifetime issues with closures)
#[derive(Clone, Copy)]
struct ChipColors {
    bg_selected: iced::Color,
    bg_normal: iced::Color,
    bg_hover: iced::Color,
    text_selected: iced::Color,
    text_normal: iced::Color,
    border_color: iced::Color,
}

impl ChipColors {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            bg_selected: theme.colors.button_primary,
            bg_normal: theme.colors.surface,
            bg_hover: theme.colors.surface_hover,
            text_selected: theme.colors.button_primary_text,
            text_normal: theme.colors.text_primary,
            border_color: theme.colors.border,
        }
    }
}

/// A preset value with display label
#[derive(Clone)]
pub struct Preset<T: Clone> {
    pub label: &'static str,
    pub value: T,
}

impl<T: Clone> Preset<T> {
    pub const fn new(label: &'static str, value: T) -> Self {
        Self { label, value }
    }
}

// Compact presets - fewer options, include unit in label for clarity
const PERCENT_PRESETS: &[Preset<f32>] = &[
    Preset::new("1%", 1.0),
    Preset::new("5%", 5.0),
    Preset::new("10%", 10.0),
];

const DELAY_PRESETS: &[Preset<f32>] = &[
    Preset::new("50", 50.0),
    Preset::new("100", 100.0),
    Preset::new("200", 200.0),
];

const JITTER_PRESETS: &[Preset<f32>] = &[
    Preset::new("10", 10.0),
    Preset::new("25", 25.0),
    Preset::new("50", 50.0),
];

const RATE_PRESETS: &[Preset<u32>] = &[
    Preset::new("1M", 1000),
    Preset::new("10M", 10000),
    Preset::new("100M", 100000),
];

/// Check if a value matches any preset
fn matches_preset_f32(value: f32, presets: &[Preset<f32>]) -> Option<usize> {
    presets.iter().position(|p| (p.value - value).abs() < 0.01)
}

fn matches_preset_u32(value: u32, presets: &[Preset<u32>]) -> Option<usize> {
    presets.iter().position(|p| p.value == value)
}

/// Style for preset chip buttons
fn chip_style(
    is_selected: bool,
    colors: ChipColors,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_iced_theme: &iced::Theme, status: button::Status| {
        let (background, text_color) = if is_selected {
            (colors.bg_selected, colors.text_selected)
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => {
                    (colors.bg_hover, colors.text_normal)
                }
                _ => (colors.bg_normal, colors.text_normal),
            }
        };

        button::Style {
            background: Some(Background::Color(background)),
            text_color,
            border: Border {
                radius: 3.0.into(),
                width: 1.0,
                color: if is_selected {
                    colors.bg_selected
                } else {
                    colors.border_color
                },
            },
            ..Default::default()
        }
    }
}

/// Render preset chips for f32 values
fn preset_chips_f32<'a>(
    presets: &[Preset<f32>],
    current_value: f32,
    on_select: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    colors: ChipColors,
    zoom: f32,
) -> Row<'a, TcInterfaceMessage> {
    let selected_idx = matches_preset_f32(current_value, presets);

    let chips: Vec<Element<'a, TcInterfaceMessage>> = presets
        .iter()
        .enumerate()
        .map(|(idx, preset)| {
            let is_selected = selected_idx == Some(idx);
            let value = preset.value;
            let on_select = on_select.clone();

            button(
                text(preset.label)
                    .size(scaled(10, zoom))
                    .align_x(iced::alignment::Horizontal::Center),
            )
            .padding([scaled_spacing(1, zoom), scaled_spacing(4, zoom)])
            .style(chip_style(is_selected, colors))
            .on_press(on_select(value))
            .into()
        })
        .collect();

    Row::with_children(chips)
        .spacing(scaled_spacing(2, zoom))
        .align_y(Alignment::Center)
}

/// Render preset chips for u32 values
fn preset_chips_u32<'a>(
    presets: &[Preset<u32>],
    current_value: u32,
    on_select: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    colors: ChipColors,
    zoom: f32,
) -> Row<'a, TcInterfaceMessage> {
    let selected_idx = matches_preset_u32(current_value, presets);

    let chips: Vec<Element<'a, TcInterfaceMessage>> = presets
        .iter()
        .enumerate()
        .map(|(idx, preset)| {
            let is_selected = selected_idx == Some(idx);
            let value = preset.value;
            let on_select = on_select.clone();

            button(
                text(preset.label)
                    .size(scaled(10, zoom))
                    .align_x(iced::alignment::Horizontal::Center),
            )
            .padding([scaled_spacing(1, zoom), scaled_spacing(4, zoom)])
            .style(chip_style(is_selected, colors))
            .on_press(on_select(value))
            .into()
        })
        .collect();

    Row::with_children(chips)
        .spacing(scaled_spacing(2, zoom))
        .align_y(Alignment::Center)
}

// ============================================================================
// Compact inline controls: [Label] [presets...] [input][unit]
// ============================================================================

/// Compact percentage input (loss, duplicate, corrupt, correlation)
/// Format: "Label: [1%] [5%] [10%] [___]%"
pub fn compact_percent<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_primary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_f32(PERCENT_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 0.0..=100.0, on_change)
            .step(0.1)
            .width(50.0 * zoom),
        text("%")
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Compact delay input (ms)
/// Format: "Label: [50] [100] [200] [___] ms"
pub fn compact_delay<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_primary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_f32(DELAY_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 0.0..=10000.0, on_change)
            .step(1.0)
            .width(60.0 * zoom),
        text("ms")
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Compact jitter input (ms)
pub fn compact_jitter<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_primary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_f32(JITTER_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 0.0..=5000.0, on_change)
            .step(1.0)
            .width(50.0 * zoom),
        text("ms")
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Compact rate input (kbps)
pub fn compact_rate<'a>(
    label: &'static str,
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_primary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_u32(RATE_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 1..=1_000_000, on_change)
            .step(1)
            .width(70.0 * zoom),
        text("kbps")
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Compact gap input (no presets, just number)
pub fn compact_gap<'a>(
    label: &'static str,
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_primary;

    row![
        text(label)
            .size(scaled(11, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        NumberInput::new(&value, 1..=10, on_change)
            .step(1)
            .width(40.0 * zoom),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

// ============================================================================
// Legacy API for backwards compatibility (delegates to compact versions)
// ============================================================================

pub fn percentage_input<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    compact_percent(label, value, on_change, theme, zoom)
}

pub fn correlation_input<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    compact_percent(label, value, on_change, theme, zoom)
}

pub fn delay_input<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    compact_delay(label, value, on_change, theme, zoom)
}

pub fn jitter_input<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    compact_jitter(label, value, on_change, theme, zoom)
}

pub fn rate_input<'a>(
    label: &'static str,
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    compact_rate(label, value, on_change, theme, zoom)
}

pub fn gap_input<'a>(
    label: &'static str,
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    compact_gap(label, value, on_change, theme, zoom)
}

// ============================================================================
// Feature Card - styled container for grouping feature controls
// ============================================================================

/// Colors extracted from theme for card styling (avoids lifetime issues)
#[derive(Clone, Copy)]
struct CardColors {
    bg: iced::Color,
    border_color: iced::Color,
}

impl CardColors {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            bg: theme.colors.surface,
            border_color: theme.colors.border,
        }
    }
}

/// Style for feature card container
fn card_style(colors: CardColors) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(colors.bg)),
        border: Border {
            radius: 6.0.into(),
            width: 1.0,
            color: colors.border_color,
        },
        shadow: Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.1),
            offset: iced::Vector::new(0.0, 1.0),
            blur_radius: 3.0,
        },
        text_color: None,
        snap: false,
    }
}

/// Create a feature card with title and content
pub fn feature_card<'a>(
    title: &'static str,
    content: Column<'a, TcInterfaceMessage>,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_primary;
    let card_colors = CardColors::from_theme(theme);

    let header = text(title)
        .size(scaled(12, zoom))
        .style(move |_| iced::widget::text::Style {
            color: Some(text_color),
        });

    container(column![header, content].spacing(scaled_spacing(2, zoom)))
        .padding(scaled_spacing(4, zoom))
        .width(Length::Shrink)
        .style(card_style(card_colors))
        .into()
}

/// Create a row of controls for inside a card (no label, just presets + input)
pub fn card_row_percent<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(10, zoom))
            .width(45.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_f32(PERCENT_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 0.0..=100.0, on_change)
            .step(0.1)
            .width(45.0 * zoom),
        text("%")
            .size(scaled(10, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Create a row for delay values inside a card
pub fn card_row_delay<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(10, zoom))
            .width(45.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_f32(DELAY_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 0.0..=10000.0, on_change)
            .step(1.0)
            .width(55.0 * zoom),
        text("ms")
            .size(scaled(10, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Create a row for jitter values inside a card
pub fn card_row_jitter<'a>(
    label: &'static str,
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(10, zoom))
            .width(45.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_f32(JITTER_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 0.0..=5000.0, on_change)
            .step(1.0)
            .width(45.0 * zoom),
        text("ms")
            .size(scaled(10, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Create a row for rate values inside a card
pub fn card_row_rate<'a>(
    label: &'static str,
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text(label)
            .size(scaled(10, zoom))
            .width(45.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        preset_chips_u32(RATE_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 1..=1_000_000, on_change)
            .step(1)
            .width(65.0 * zoom),
        text("kbps")
            .size(scaled(10, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

/// Create a row for gap values inside a card (no presets)
pub fn card_row_gap<'a>(
    label: &'static str,
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;

    row![
        text(label)
            .size(scaled(10, zoom))
            .width(45.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        NumberInput::new(&value, 1..=10, on_change)
            .step(1)
            .width(40.0 * zoom),
        text("pkts")
            .size(scaled(10, zoom))
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
    ]
    .spacing(scaled_spacing(4, zoom))
    .align_y(Alignment::Center)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_preset_f32() {
        assert_eq!(matches_preset_f32(1.0, PERCENT_PRESETS), Some(0));
        assert_eq!(matches_preset_f32(5.0, PERCENT_PRESETS), Some(1));
        assert_eq!(matches_preset_f32(3.7, PERCENT_PRESETS), None);
    }

    #[test]
    fn test_matches_preset_u32() {
        assert_eq!(matches_preset_u32(1000, RATE_PRESETS), Some(0));
        assert_eq!(matches_preset_u32(999, RATE_PRESETS), None);
    }
}
