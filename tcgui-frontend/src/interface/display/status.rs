//! Status display component for operation and error indicators.
//!
//! This component shows the current status of interface operations with
//! visual indicators for success, pending operations, and errors.

use iced::widget::text;
use iced::{Color, Element};

use crate::messages::TcInterfaceMessage;

/// Status indicator types
/// Note: Some variants currently unused but kept for future extensibility
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum StatusType {
    /// Ready/idle state
    Ready,
    /// Operation in progress
    Applying,
    /// Operation completed successfully
    Success,
    /// Error occurred
    Error,
    /// Interface state change in progress
    InterfaceChanging,
}

/// Component for status indicator display
#[derive(Debug, Clone)]
pub struct StatusDisplayComponent {
    /// Current status type
    status: StatusType,
    /// Optional status message (currently unused but kept for future)
    #[allow(dead_code)]
    message: Option<String>,
    /// Whether any operation is in progress (currently unused but kept for future)
    #[allow(dead_code)]
    applying: bool,
    /// Whether interface state change is in progress (currently unused but kept for future)
    #[allow(dead_code)]
    applying_interface_state: bool,
}

impl Default for StatusDisplayComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusDisplayComponent {
    /// Create a new status display component
    pub fn new() -> Self {
        Self {
            status: StatusType::Ready,
            message: None,
            applying: false,
            applying_interface_state: false,
        }
    }

    // Removed unused methods (kept for tests where needed):
    // - update_status: Complex state management logic available if needed
    // - set_error: Error handling logic available if needed
    // - set_success: Success handling logic available if needed
    // - clear_status: Status clearing logic available if needed
    // - message: Message access available via field if needed

    /// Get current status type (only used in tests)
    #[cfg(test)]
    pub fn status(&self) -> &StatusType {
        &self.status
    }

    /// Get the appropriate icon for the current status
    fn status_icon(&self) -> &'static str {
        match self.status {
            StatusType::Ready => "🟢",
            StatusType::Applying => "⚡",
            StatusType::Success => "✅",
            StatusType::Error => "❌",
            StatusType::InterfaceChanging => "🔄",
        }
    }

    /// Get the appropriate color for the current status
    fn status_color(&self) -> Color {
        match self.status {
            StatusType::Ready => Color::from_rgb(0.5, 0.5, 0.5), // Gray
            StatusType::Applying => Color::from_rgb(1.0, 0.6, 0.0), // Orange
            StatusType::Success => Color::from_rgb(0.0, 0.8, 0.3), // Green
            StatusType::Error => Color::from_rgb(0.9, 0.2, 0.2), // Red
            StatusType::InterfaceChanging => Color::from_rgb(0.0, 0.6, 0.9), // Blue
        }
    }

    /// Render the status display
    pub fn view(&self) -> Element<'_, TcInterfaceMessage> {
        let color = self.status_color();
        text(self.status_icon())
            .size(13)
            .style(move |_| text::Style { color: Some(color) })
            .into()
    }

    // Removed unused methods:
    // - tooltip_text: Tooltip logic available if needed
    // - is_busy: Status checking logic available if needed
    // - has_error: Error checking logic available if needed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_display_creation() {
        let component = StatusDisplayComponent::new();
        assert_eq!(component.status(), &StatusType::Ready);
        assert!(component.message.is_none());
        assert!(!component.applying);
        assert!(!component.applying_interface_state);
    }

    #[test]
    fn test_status_update_manually() {
        let mut component = StatusDisplayComponent::new();

        // Test manual status setting
        component.status = StatusType::Applying;
        assert_eq!(component.status(), &StatusType::Applying);

        component.status = StatusType::Error;
        assert_eq!(component.status(), &StatusType::Error);

        component.status = StatusType::Success;
        assert_eq!(component.status(), &StatusType::Success);
    }

    #[test]
    fn test_status_icons() {
        let mut component = StatusDisplayComponent::new();

        assert_eq!(component.status_icon(), "🟢"); // Ready

        component.status = StatusType::Applying;
        assert_eq!(component.status_icon(), "⚡"); // Applying

        component.status = StatusType::Success;
        assert_eq!(component.status_icon(), "✅"); // Success

        component.status = StatusType::Error;
        assert_eq!(component.status_icon(), "❌"); // Error

        component.status = StatusType::InterfaceChanging;
        assert_eq!(component.status_icon(), "🔄"); // Interface changing
    }
}
