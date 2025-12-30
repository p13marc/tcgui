//! Modular interface components for TC GUI.
//!
//! This module provides a decomposed, maintainable architecture for network
//! interface traffic control management. The original 1,162-line monolithic
//! interface has been broken down into focused, testable components.
//!
//! # Architecture
//!
//! - **Base Interface** (`base.rs`): Core logic and component coordination
//! - **State Management** (`state.rs`): Centralized state using Sprint 1 types
//! - **Messages** (`messages.rs`): Modular message hierarchy
//! - **Controls**: Feature-specific UI components (removed as unused)
//! - **Display** (`display/`): Bandwidth and status display components
//! - **Presets** (`preset/`): Preset management functionality
//!
//! # Usage
//!
//! ```rust,ignore
//! use tcgui_frontend::interface::TcInterface;
//!
//! let mut interface = TcInterface::new("eth0");
//! let view = interface.view();
//! ```

pub mod base;
pub mod messages;
pub mod state;

// Feature-specific control modules (removed as unused)

// Display components
pub mod display;

// Preset management
pub mod preset;

// Re-export the main interface component for backward compatibility
pub use base::TcInterface;

// Re-export commonly used types
// Note: Individual component message types and state are currently unused externally
// They remain available for future development but are not exported to reduce clutter
