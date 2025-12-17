//! Message types for modular interface components.
//!
//! This module defines message types for future component-based architecture.
//! Currently these are unused as the interface uses TcInterfaceMessage directly,
//! but they provide a foundation for future modular development.
//!
//! Note: This entire module is currently unused (dead_code allowed at module level).

#![allow(dead_code)]

use iced::Task;
use tcgui_shared::{presets::NetworkPreset, NetworkBandwidthStats};

/// Main interface message type that routes to specific components
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum InterfaceMessage {
    // Core interface control
    InterfaceToggled(bool),
    QdiscToggled(bool),

    // Configuration application
    ApplyConfiguration,
    RemoveConfiguration,
}

/// Messages for packet loss control component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum LossControlMessage {
    PercentageChanged(f32),
    CorrelationChanged(f32),
    Toggled(bool),
}

/// Messages for network delay control component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum DelayControlMessage {
    BaseDelayChanged(f32),
    JitterChanged(f32),
    CorrelationChanged(f32),
    Toggled(bool),
}

/// Messages for packet duplication control component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum DuplicateControlMessage {
    PercentageChanged(f32),
    CorrelationChanged(f32),
    Toggled(bool),
}

/// Messages for packet reordering control component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum ReorderControlMessage {
    PercentageChanged(f32),
    CorrelationChanged(f32),
    GapChanged(u32),
    Toggled(bool),
}

/// Messages for packet corruption control component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum CorruptControlMessage {
    PercentageChanged(f32),
    CorrelationChanged(f32),
    Toggled(bool),
}

/// Messages for rate limiting control component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum RateLimitControlMessage {
    RateChanged(u32),
    Toggled(bool),
}

/// Messages for display components (bandwidth, status)
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum DisplayMessage {
    BandwidthUpdate(NetworkBandwidthStats),
    StatusMessage(String),
    ClearStatus,
}

/// Messages for preset management component
/// Currently unused - prepared for future modular architecture
#[derive(Debug, Clone)]
pub enum PresetMessage {
    PresetSelected(NetworkPreset),
    ApplyPreset,
    ToggleVisibility,
}

/// Conversion trait for bridging old and new message systems
/// Currently unused - prepared for future modular architecture
pub trait MessageConverter<T> {
    fn convert_to_legacy(self) -> T;
    fn convert_from_legacy(legacy: T) -> Self;
}

/// Helper type for component updates
/// Currently unused - prepared for future modular architecture
pub type ComponentTask<T> = Task<T>;
