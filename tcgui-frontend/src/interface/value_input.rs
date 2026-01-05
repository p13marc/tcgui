//! TC parameter input controls with research-based presets.
//!
//! This module provides input controls optimized for each TC parameter type:
//! - **Chips + Dropdown**: For percentages and timing with common presets
//! - **Chips only**: For small discrete values (gap, correlation)
//! - **NumberInput only**: For rate limit (wide range, needs precision)
//!
//! Preset values are based on real-world network conditions research.

use iced::widget::{Column, Row, button, container, pick_list, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};
use iced_aw::NumberInput;

use crate::messages::TcInterfaceMessage;
use crate::theme::Theme;
use crate::view::{scaled, scaled_spacing};

// ============================================================================
// Research-based preset values
// ============================================================================

/// Loss percentage presets (based on network quality thresholds)
/// - 0.5%: Good 4G/WiFi
/// - 1%: Acceptable threshold (causes 70% throughput drop!)
/// - 5%: Poor connection
const LOSS_CHIP_PRESETS: &[(&str, f32)] = &[("0.5", 0.5), ("1", 1.0), ("5", 5.0)];
const LOSS_DROPDOWN_OPTIONS: &[(&str, f32)] = &[
    ("0.1%", 0.1),
    ("0.5%", 0.5),
    ("1%", 1.0),
    ("2%", 2.0),
    ("5%", 5.0),
    ("10%", 10.0),
    ("20%", 20.0),
];

/// Delay presets in ms (based on network types)
/// - 20ms: Good broadband/5G
/// - 100ms: 4G LTE
/// - 500ms: Satellite (LEO)
const DELAY_CHIP_PRESETS: &[(&str, f32)] = &[("20", 20.0), ("100", 100.0), ("500", 500.0)];
const DELAY_DROPDOWN_OPTIONS: &[(&str, f32)] = &[
    ("5ms", 5.0),
    ("20ms", 20.0),
    ("50ms", 50.0),
    ("100ms", 100.0),
    ("200ms", 200.0),
    ("500ms", 500.0),
    ("1000ms", 1000.0),
    ("2000ms", 2000.0),
];

/// Jitter presets in ms (based on VoIP/video quality thresholds)
/// - 10ms: Good quality
/// - 30ms: VoIP acceptable limit
/// - 100ms: Poor/mobile
const JITTER_CHIP_PRESETS: &[(&str, f32)] = &[("10", 10.0), ("30", 30.0), ("100", 100.0)];
const JITTER_DROPDOWN_OPTIONS: &[(&str, f32)] = &[
    ("5ms", 5.0),
    ("10ms", 10.0),
    ("20ms", 20.0),
    ("30ms", 30.0),
    ("50ms", 50.0),
    ("100ms", 100.0),
    ("200ms", 200.0),
];

/// Correlation (burst) presets
/// - 0%: Fully random
/// - 25%: Slight bursting (typical recommendation)
/// - 50%: Moderate bursting
const CORRELATION_CHIP_PRESETS: &[(&str, f32)] = &[("0", 0.0), ("25", 25.0), ("50", 50.0)];

/// Duplicate/Corrupt percentage presets
/// - 0.1%: Realistic
/// - 1%: Testing
/// - 5%: Stress test
const SMALL_PERCENT_CHIP_PRESETS: &[(&str, f32)] = &[("0.1", 0.1), ("1", 1.0), ("5", 5.0)];
const SMALL_PERCENT_DROPDOWN_OPTIONS: &[(&str, f32)] = &[
    ("0.1%", 0.1),
    ("0.5%", 0.5),
    ("1%", 1.0),
    ("2%", 2.0),
    ("5%", 5.0),
    ("10%", 10.0),
];

/// Reorder percentage presets
/// - 5%: Slight reordering
/// - 10%: Moderate
/// - 25%: Heavy
const REORDER_CHIP_PRESETS: &[(&str, f32)] = &[("5", 5.0), ("10", 10.0), ("25", 25.0)];
const REORDER_DROPDOWN_OPTIONS: &[(&str, f32)] =
    &[("5%", 5.0), ("10%", 10.0), ("25%", 25.0), ("50%", 50.0)];

/// Gap presets (packets between reordered packets)
const GAP_CHIP_PRESETS: &[(&str, u32)] = &[("1", 1), ("3", 3), ("5", 5)];

// ============================================================================
// Theme color extraction (avoids lifetime issues with closures)
// ============================================================================

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

// ============================================================================
// Chip button styling
// ============================================================================

fn chip_style(
    is_selected: bool,
    colors: ChipColors,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme: &iced::Theme, status: button::Status| {
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

// ============================================================================
// Helper: Check if value matches a preset
// ============================================================================

fn matches_f32(value: f32, presets: &[(&str, f32)]) -> Option<usize> {
    presets.iter().position(|(_, v)| (*v - value).abs() < 0.01)
}

fn matches_u32(value: u32, presets: &[(&str, u32)]) -> Option<usize> {
    presets.iter().position(|(_, v)| *v == value)
}

// ============================================================================
// Chip row builders
// ============================================================================

fn chips_f32<'a>(
    presets: &[(&'static str, f32)],
    current_value: f32,
    on_select: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    colors: ChipColors,
    zoom: f32,
) -> Row<'a, TcInterfaceMessage> {
    let selected_idx = matches_f32(current_value, presets);

    let chips: Vec<Element<'a, TcInterfaceMessage>> = presets
        .iter()
        .enumerate()
        .map(|(idx, (label, value))| {
            let is_selected = selected_idx == Some(idx);
            let v = *value;
            let on_select = on_select.clone();

            button(
                text(*label)
                    .size(scaled(10, zoom))
                    .align_x(iced::alignment::Horizontal::Center),
            )
            .padding([scaled_spacing(1, zoom), scaled_spacing(4, zoom)])
            .style(chip_style(is_selected, colors))
            .on_press(on_select(v))
            .into()
        })
        .collect();

    Row::with_children(chips)
        .spacing(scaled_spacing(2, zoom))
        .align_y(Alignment::Center)
}

fn chips_u32<'a>(
    presets: &[(&'static str, u32)],
    current_value: u32,
    on_select: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    colors: ChipColors,
    zoom: f32,
) -> Row<'a, TcInterfaceMessage> {
    let selected_idx = matches_u32(current_value, presets);

    let chips: Vec<Element<'a, TcInterfaceMessage>> = presets
        .iter()
        .enumerate()
        .map(|(idx, (label, value))| {
            let is_selected = selected_idx == Some(idx);
            let v = *value;
            let on_select = on_select.clone();

            button(
                text(*label)
                    .size(scaled(10, zoom))
                    .align_x(iced::alignment::Horizontal::Center),
            )
            .padding([scaled_spacing(1, zoom), scaled_spacing(4, zoom)])
            .style(chip_style(is_selected, colors))
            .on_press(on_select(v))
            .into()
        })
        .collect();

    Row::with_children(chips)
        .spacing(scaled_spacing(2, zoom))
        .align_y(Alignment::Center)
}

// ============================================================================
// Dropdown wrapper for f32 values
// ============================================================================

/// A selectable dropdown option for f32 values
#[derive(Debug, Clone, PartialEq)]
pub struct DropdownOption {
    pub label: String,
    pub value: f32,
}

impl std::fmt::Display for DropdownOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

fn dropdown_f32<'a>(
    options: &[(&str, f32)],
    current_value: f32,
    on_select: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let dropdown_options: Vec<DropdownOption> = options
        .iter()
        .map(|(label, value)| DropdownOption {
            label: label.to_string(),
            value: *value,
        })
        .collect();

    let selected = dropdown_options
        .iter()
        .find(|o| (o.value - current_value).abs() < 0.01)
        .cloned();

    pick_list(dropdown_options, selected, move |opt: DropdownOption| {
        on_select(opt.value)
    })
    .text_size(scaled(10, zoom))
    .padding(scaled_spacing(2, zoom))
    .width(Length::Shrink)
    .into()
}

// ============================================================================
// PUBLIC API: Input components for each parameter type
// ============================================================================

/// Loss percentage: chips [0.5] [1] [5] + dropdown + custom input
pub fn loss_input<'a>(
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Loss:")
            .size(scaled(10, zoom))
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(LOSS_CHIP_PRESETS, value, on_change.clone(), colors, zoom),
        dropdown_f32(LOSS_DROPDOWN_OPTIONS, value, on_change.clone(), zoom),
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

/// Delay in ms: chips [20] [100] [500] + dropdown + custom input
pub fn delay_input<'a>(
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Delay:")
            .size(scaled(10, zoom))
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(DELAY_CHIP_PRESETS, value, on_change.clone(), colors, zoom),
        dropdown_f32(DELAY_DROPDOWN_OPTIONS, value, on_change.clone(), zoom),
        NumberInput::new(&value, 0.0..=10000.0, on_change)
            .step(1.0)
            .width(50.0 * zoom),
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

/// Jitter in ms: chips [10] [30] [100] + dropdown + custom input
pub fn jitter_input<'a>(
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Jitter:")
            .size(scaled(10, zoom))
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(JITTER_CHIP_PRESETS, value, on_change.clone(), colors, zoom),
        dropdown_f32(JITTER_DROPDOWN_OPTIONS, value, on_change.clone(), zoom),
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

/// Correlation (burst): chips only [0] [25] [50] - small fixed set
pub fn correlation_input<'a>(
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
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(
            CORRELATION_CHIP_PRESETS,
            value,
            on_change.clone(),
            colors,
            zoom
        ),
        NumberInput::new(&value, 0.0..=100.0, on_change)
            .step(1.0)
            .width(40.0 * zoom),
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

/// Duplicate percentage: chips [0.1] [1] [5] + dropdown + custom
pub fn duplicate_input<'a>(
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Dup:")
            .size(scaled(10, zoom))
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(
            SMALL_PERCENT_CHIP_PRESETS,
            value,
            on_change.clone(),
            colors,
            zoom
        ),
        dropdown_f32(
            SMALL_PERCENT_DROPDOWN_OPTIONS,
            value,
            on_change.clone(),
            zoom
        ),
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

/// Corrupt percentage: chips [0.1] [1] [5] + dropdown + custom
pub fn corrupt_input<'a>(
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Corrupt:")
            .size(scaled(10, zoom))
            .width(50.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(
            SMALL_PERCENT_CHIP_PRESETS,
            value,
            on_change.clone(),
            colors,
            zoom
        ),
        dropdown_f32(
            SMALL_PERCENT_DROPDOWN_OPTIONS,
            value,
            on_change.clone(),
            zoom
        ),
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

/// Reorder percentage: chips [5] [10] [25] + dropdown + custom
pub fn reorder_input<'a>(
    value: f32,
    on_change: impl Fn(f32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Reorder:")
            .size(scaled(10, zoom))
            .width(50.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_f32(REORDER_CHIP_PRESETS, value, on_change.clone(), colors, zoom),
        dropdown_f32(REORDER_DROPDOWN_OPTIONS, value, on_change.clone(), zoom),
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

/// Gap (packets): chips only [1] [3] [5] - small discrete set
pub fn gap_input<'a>(
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;
    let colors = ChipColors::from_theme(theme);

    row![
        text("Gap:")
            .size(scaled(10, zoom))
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        chips_u32(GAP_CHIP_PRESETS, value, on_change.clone(), colors, zoom),
        NumberInput::new(&value, 1..=10, on_change)
            .step(1)
            .width(35.0 * zoom),
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

/// Rate limit in kbps: NumberInput only (wide range, needs precision)
pub fn rate_input<'a>(
    value: u32,
    on_change: impl Fn(u32) -> TcInterfaceMessage + Clone + 'static,
    theme: &Theme,
    zoom: f32,
) -> Element<'a, TcInterfaceMessage> {
    let text_color = theme.colors.text_secondary;

    row![
        text("Rate:")
            .size(scaled(10, zoom))
            .width(40.0 * zoom)
            .style(move |_| iced::widget::text::Style {
                color: Some(text_color)
            }),
        NumberInput::new(&value, 1..=1_000_000, on_change)
            .step(1)
            .width(80.0 * zoom),
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

// ============================================================================
// Feature Card - styled container for grouping feature controls
// ============================================================================

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
        .size(scaled(11, zoom))
        .style(move |_| iced::widget::text::Style {
            color: Some(text_color),
        });

    container(iced::widget::column![header, content].spacing(scaled_spacing(2, zoom)))
        .padding(scaled_spacing(4, zoom))
        .width(Length::Shrink)
        .style(card_style(card_colors))
        .into()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_f32() {
        assert_eq!(matches_f32(0.5, LOSS_CHIP_PRESETS), Some(0));
        assert_eq!(matches_f32(1.0, LOSS_CHIP_PRESETS), Some(1));
        assert_eq!(matches_f32(5.0, LOSS_CHIP_PRESETS), Some(2));
        assert_eq!(matches_f32(3.7, LOSS_CHIP_PRESETS), None);
    }

    #[test]
    fn test_matches_u32() {
        assert_eq!(matches_u32(1, GAP_CHIP_PRESETS), Some(0));
        assert_eq!(matches_u32(3, GAP_CHIP_PRESETS), Some(1));
        assert_eq!(matches_u32(2, GAP_CHIP_PRESETS), None);
    }

    #[test]
    fn test_dropdown_option_display() {
        let opt = DropdownOption {
            label: "5%".to_string(),
            value: 5.0,
        };
        assert_eq!(format!("{}", opt), "5%");
    }
}
