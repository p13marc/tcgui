//! Display components for bandwidth monitoring and status indicators.
//!
//! This module contains components responsible for displaying real-time
//! information about network interfaces, including bandwidth statistics
//! and operational status.

pub mod bandwidth;
pub mod status;

// Re-export components for easier access
pub use bandwidth::BandwidthDisplayComponent;
pub use status::StatusDisplayComponent;
