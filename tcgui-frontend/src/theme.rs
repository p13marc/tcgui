//! Theme system for TC GUI.
//!
//! Provides centralized color management with light and dark mode support.
//! All UI components should use colors from the theme rather than hardcoded values.

use iced::widget::scrollable::{self, AutoScroll, Rail, Scroller, Status};
use iced::{Background, Border, Color, Shadow};

/// Theme mode selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

/// Complete theme definition with all colors used in the application.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Current theme mode
    pub mode: ThemeMode,
    /// Color palette
    pub colors: ThemeColors,
}

/// All colors used throughout the application.
#[derive(Debug, Clone)]
pub struct ThemeColors {
    // Base colors
    /// Main background color
    pub background: Color,
    /// Surface color for cards and panels
    pub surface: Color,
    /// Surface color on hover
    pub surface_hover: Color,

    // Text colors
    /// Primary text color
    pub text_primary: Color,
    /// Secondary/dimmed text color
    pub text_secondary: Color,
    /// Muted/disabled text color
    pub text_muted: Color,

    // Semantic colors
    /// Success state (e.g., interface up)
    pub success: Color,
    /// Warning state (e.g., TC active)
    pub warning: Color,
    /// Error state
    pub error: Color,
    /// Informational state
    pub info: Color,

    // Bandwidth indicators
    /// Download/receive color
    pub rx_color: Color,
    /// Upload/transmit color
    pub tx_color: Color,

    // Interface states
    /// Interface up indicator
    pub interface_up: Color,
    /// Interface down indicator
    pub interface_down: Color,
    /// TC rules active indicator
    pub tc_active: Color,
    /// TC rules inactive indicator
    pub tc_inactive: Color,

    // Borders and dividers
    /// Border color for cards and inputs
    pub border: Color,
    /// Divider line color
    pub divider: Color,

    // Button colors
    /// Primary button background
    pub button_primary: Color,
    /// Primary button text
    pub button_primary_text: Color,
    /// Secondary button background
    pub button_secondary: Color,
    /// Secondary button text
    pub button_secondary_text: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    /// Create a light theme.
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            colors: ThemeColors {
                // Base colors
                background: Color::from_rgb(0.95, 0.95, 0.95),
                surface: Color::WHITE,
                surface_hover: Color::from_rgb(0.98, 0.98, 0.98),

                // Text colors
                text_primary: Color::from_rgb(0.1, 0.1, 0.1),
                text_secondary: Color::from_rgb(0.4, 0.4, 0.4),
                text_muted: Color::from_rgb(0.6, 0.6, 0.6),

                // Semantic colors
                success: Color::from_rgb(0.2, 0.7, 0.3),
                warning: Color::from_rgb(0.9, 0.6, 0.1),
                error: Color::from_rgb(0.9, 0.3, 0.3),
                info: Color::from_rgb(0.2, 0.5, 0.8),

                // Bandwidth indicators
                rx_color: Color::from_rgb(0.0, 0.6, 0.9),
                tx_color: Color::from_rgb(0.9, 0.5, 0.0),

                // Interface states
                interface_up: Color::from_rgb(0.2, 0.7, 0.3),
                interface_down: Color::from_rgb(0.6, 0.6, 0.6),
                tc_active: Color::from_rgb(0.9, 0.6, 0.1),
                tc_inactive: Color::from_rgb(0.8, 0.8, 0.8),

                // Borders and dividers
                border: Color::from_rgb(0.85, 0.85, 0.85),
                divider: Color::from_rgb(0.9, 0.9, 0.9),

                // Button colors
                button_primary: Color::from_rgb(0.2, 0.5, 0.8),
                button_primary_text: Color::WHITE,
                button_secondary: Color::from_rgb(0.9, 0.9, 0.9),
                button_secondary_text: Color::from_rgb(0.2, 0.2, 0.2),
            },
        }
    }

    /// Create a dark theme.
    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            colors: ThemeColors {
                // Base colors
                background: Color::from_rgb(0.1, 0.1, 0.12),
                surface: Color::from_rgb(0.15, 0.15, 0.18),
                surface_hover: Color::from_rgb(0.2, 0.2, 0.24),

                // Text colors
                text_primary: Color::from_rgb(0.93, 0.93, 0.93),
                text_secondary: Color::from_rgb(0.7, 0.7, 0.7),
                text_muted: Color::from_rgb(0.5, 0.5, 0.5),

                // Semantic colors
                success: Color::from_rgb(0.3, 0.8, 0.4),
                warning: Color::from_rgb(1.0, 0.7, 0.2),
                error: Color::from_rgb(1.0, 0.4, 0.4),
                info: Color::from_rgb(0.4, 0.7, 1.0),

                // Bandwidth indicators
                rx_color: Color::from_rgb(0.3, 0.8, 1.0),
                tx_color: Color::from_rgb(1.0, 0.6, 0.2),

                // Interface states
                interface_up: Color::from_rgb(0.3, 0.8, 0.4),
                interface_down: Color::from_rgb(0.5, 0.5, 0.5),
                tc_active: Color::from_rgb(1.0, 0.7, 0.2),
                tc_inactive: Color::from_rgb(0.3, 0.3, 0.3),

                // Borders and dividers
                border: Color::from_rgb(0.3, 0.3, 0.35),
                divider: Color::from_rgb(0.25, 0.25, 0.3),

                // Button colors
                button_primary: Color::from_rgb(0.3, 0.6, 0.9),
                button_primary_text: Color::WHITE,
                button_secondary: Color::from_rgb(0.25, 0.25, 0.3),
                button_secondary_text: Color::from_rgb(0.9, 0.9, 0.9),
            },
        }
    }

    /// Toggle between light and dark mode.
    pub fn toggle(&self) -> Self {
        match self.mode {
            ThemeMode::Light => Self::dark(),
            ThemeMode::Dark => Self::light(),
        }
    }

    /// Check if dark mode is active.
    pub fn is_dark(&self) -> bool {
        self.mode == ThemeMode::Dark
    }

    /// Create a tooltip style for this theme.
    ///
    /// Tooltips have a solid background with good contrast for readability
    /// in both light and dark modes, with a subtle border and shadow.
    pub fn tooltip_style(&self) -> iced::widget::container::Style {
        let (bg_color, text_color, border_color) = if self.is_dark() {
            (
                Color::from_rgb(0.2, 0.2, 0.24),   // Dark surface
                Color::from_rgb(0.93, 0.93, 0.93), // Light text
                Color::from_rgb(0.35, 0.35, 0.4),  // Subtle border
            )
        } else {
            (
                Color::from_rgb(0.15, 0.15, 0.18), // Dark background for contrast
                Color::from_rgb(0.95, 0.95, 0.95), // Light text
                Color::from_rgb(0.1, 0.1, 0.12),   // Dark border
            )
        };

        iced::widget::container::Style {
            background: Some(Background::Color(bg_color)),
            text_color: Some(text_color),
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: border_color,
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 4.0,
            },
            snap: false,
        }
    }

    /// Create a smart scrollbar style function for this theme.
    ///
    /// Smart scrollbars are semi-transparent when idle and become fully visible
    /// when hovered or dragged, providing a cleaner UI while maintaining usability.
    pub fn smart_scrollbar_style(&self) -> impl Fn(&iced::Theme, Status) -> scrollable::Style {
        let is_dark = self.is_dark();

        move |_theme: &iced::Theme, status: Status| {
            // Base colors depend on theme mode
            let (rail_bg, scroller_idle, scroller_hover, scroller_drag) = if is_dark {
                (
                    Color::from_rgba(1.0, 1.0, 1.0, 0.05), // Very subtle rail
                    Color::from_rgba(1.0, 1.0, 1.0, 0.15), // Semi-transparent idle
                    Color::from_rgba(1.0, 1.0, 1.0, 0.35), // More visible on hover
                    Color::from_rgba(1.0, 1.0, 1.0, 0.5),  // Fully visible when dragging
                )
            } else {
                (
                    Color::from_rgba(0.0, 0.0, 0.0, 0.05), // Very subtle rail
                    Color::from_rgba(0.0, 0.0, 0.0, 0.15), // Semi-transparent idle
                    Color::from_rgba(0.0, 0.0, 0.0, 0.35), // More visible on hover
                    Color::from_rgba(0.0, 0.0, 0.0, 0.5),  // Fully visible when dragging
                )
            };

            // Determine scroller color based on status
            let scroller_color = match status {
                Status::Active { .. } => scroller_idle,
                Status::Hovered { .. } => scroller_hover,
                Status::Dragged { .. } => scroller_drag,
            };

            let rail = Rail {
                background: Some(Background::Color(rail_bg)),
                border: Border::default(),
                scroller: Scroller {
                    background: Background::Color(scroller_color),
                    border: Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                },
            };

            scrollable::Style {
                container: iced::widget::container::Style::default(),
                vertical_rail: rail,
                horizontal_rail: rail,
                gap: None,
                auto_scroll: AutoScroll {
                    background: Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.7)),
                    border: Border::default(),
                    shadow: Shadow::default(),
                    icon: Color::WHITE,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_light() {
        let theme = Theme::default();
        assert_eq!(theme.mode, ThemeMode::Light);
    }

    #[test]
    fn test_toggle_light_to_dark() {
        let light = Theme::light();
        let dark = light.toggle();
        assert_eq!(dark.mode, ThemeMode::Dark);
    }

    #[test]
    fn test_toggle_dark_to_light() {
        let dark = Theme::dark();
        let light = dark.toggle();
        assert_eq!(light.mode, ThemeMode::Light);
    }

    #[test]
    fn test_is_dark() {
        assert!(!Theme::light().is_dark());
        assert!(Theme::dark().is_dark());
    }
}
