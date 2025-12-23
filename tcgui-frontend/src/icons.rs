//! SVG icon system for the TC GUI frontend.
//!
//! This module provides embedded SVG icons to replace Unicode emojis,
//! ensuring consistent rendering across all Linux systems regardless
//! of installed fonts.

use iced::widget::svg::{Handle, Svg};
use iced::{Color, Length};

/// All available icons in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // Status indicators
    /// Green circle - ready state
    CircleCheck,
    /// Lightning bolt - applying/in progress
    Zap,
    /// Checkmark in circle - success
    CheckCircle,
    /// X in circle - error/failed
    XCircle,
    /// Rotating arrows - changing/reconnecting
    RefreshCw,
    /// Triangle with exclamation - warning
    AlertTriangle,

    // Navigation/UI
    /// Globe - interfaces tab
    Globe,
    /// Bar chart - scenarios tab
    BarChart3,
    /// Magnifying glass - zoom
    Search,
    /// Open eye - visibility on
    Eye,
    /// Eye with slash - visibility off
    EyeOff,
    /// Crescent moon - light mode toggle
    Moon,
    /// Sun - dark mode toggle
    Sun,

    // Container/Namespace types
    /// House - default namespace
    Home,
    /// Folder - traditional namespace
    Folder,
    /// Container box - Docker
    Container,
    /// Box/package - generic container
    Box,

    // Playback controls
    /// Play triangle
    Play,
    /// Pause bars
    Pause,
    /// Stop square
    Square,
    /// Checkmark
    Check,
    /// X mark
    X,
    /// Empty circle - pending
    Circle,
    /// Circular arrows - loop mode
    Repeat,

    // Data/Activity
    /// Line chart ascending - Rx rate
    TrendingUp,
    /// Arrow up - Tx rate
    ArrowUp,
    /// Activity pulse - active
    Activity,
    /// Hourglass/loader
    Loader,

    // Labels/Sections
    /// Antenna - no interfaces
    Radio,
    /// Clipboard - scenario list
    Clipboard,
    /// Monitor - backend header
    Monitor,
    /// Gamepad - active executions
    Gamepad2,
    /// Target - interface selection
    Target,
    /// Chain link - connected
    Link,
    /// Tag/label
    Tag,
    /// Arrow right - sequential flow
    ArrowRight,
}

impl Icon {
    /// Returns the raw SVG bytes for this icon.
    fn bytes(self) -> &'static [u8] {
        match self {
            // Status indicators
            Icon::CircleCheck => include_bytes!("../icons/circle-check.svg"),
            Icon::Zap => include_bytes!("../icons/zap.svg"),
            Icon::CheckCircle => include_bytes!("../icons/check-circle.svg"),
            Icon::XCircle => include_bytes!("../icons/x-circle.svg"),
            Icon::RefreshCw => include_bytes!("../icons/refresh-cw.svg"),
            Icon::AlertTriangle => include_bytes!("../icons/alert-triangle.svg"),

            // Navigation/UI
            Icon::Globe => include_bytes!("../icons/globe.svg"),
            Icon::BarChart3 => include_bytes!("../icons/bar-chart-3.svg"),
            Icon::Search => include_bytes!("../icons/search.svg"),
            Icon::Eye => include_bytes!("../icons/eye.svg"),
            Icon::EyeOff => include_bytes!("../icons/eye-off.svg"),
            Icon::Moon => include_bytes!("../icons/moon.svg"),
            Icon::Sun => include_bytes!("../icons/sun.svg"),

            // Container/Namespace types
            Icon::Home => include_bytes!("../icons/home.svg"),
            Icon::Folder => include_bytes!("../icons/folder.svg"),
            Icon::Container => include_bytes!("../icons/container.svg"),
            Icon::Box => include_bytes!("../icons/box.svg"),

            // Playback controls
            Icon::Play => include_bytes!("../icons/play.svg"),
            Icon::Pause => include_bytes!("../icons/pause.svg"),
            Icon::Square => include_bytes!("../icons/square.svg"),
            Icon::Check => include_bytes!("../icons/check.svg"),
            Icon::X => include_bytes!("../icons/x.svg"),
            Icon::Circle => include_bytes!("../icons/circle.svg"),
            Icon::Repeat => include_bytes!("../icons/repeat.svg"),

            // Data/Activity
            Icon::TrendingUp => include_bytes!("../icons/trending-up.svg"),
            Icon::ArrowUp => include_bytes!("../icons/arrow-up.svg"),
            Icon::Activity => include_bytes!("../icons/activity.svg"),
            Icon::Loader => include_bytes!("../icons/loader.svg"),

            // Labels/Sections
            Icon::Radio => include_bytes!("../icons/radio.svg"),
            Icon::Clipboard => include_bytes!("../icons/clipboard.svg"),
            Icon::Monitor => include_bytes!("../icons/monitor.svg"),
            Icon::Gamepad2 => include_bytes!("../icons/gamepad-2.svg"),
            Icon::Target => include_bytes!("../icons/target.svg"),
            Icon::Link => include_bytes!("../icons/link.svg"),
            Icon::Tag => include_bytes!("../icons/tag.svg"),
            Icon::ArrowRight => include_bytes!("../icons/arrow-right.svg"),
        }
    }

    /// Creates an SVG widget with the default size (16x16).
    pub fn svg(self) -> Svg<'static> {
        Svg::new(Handle::from_memory(self.bytes()))
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
    }

    /// Creates an SVG widget with a custom size.
    pub fn svg_sized(self, size: f32) -> Svg<'static> {
        Svg::new(Handle::from_memory(self.bytes()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
    }

    /// Creates an SVG widget with a specific color.
    ///
    /// Note: This requires the SVG to use `currentColor` for stroke/fill.
    pub fn svg_colored(self, color: Color) -> Svg<'static> {
        use iced::widget::svg;
        self.svg()
            .style(move |_theme, _status| svg::Style { color: Some(color) })
    }

    /// Creates an SVG widget with a specific size and color.
    pub fn svg_sized_colored(self, size: f32, color: Color) -> Svg<'static> {
        use iced::widget::svg;
        self.svg_sized(size)
            .style(move |_theme, _status| svg::Style { color: Some(color) })
    }
}
