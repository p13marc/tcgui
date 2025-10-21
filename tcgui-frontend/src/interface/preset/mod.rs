//! Preset management components for network configuration presets.
//!
//! This module provides components for managing and applying network
//! traffic control presets, allowing users to quickly configure
//! common network conditions.

pub mod manager;

// Re-export components for easier access
pub use manager::PresetManagerComponent;
