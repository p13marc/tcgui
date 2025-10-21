//! Query channel management for TC GUI frontend.
//!
//! This module handles TC and interface control query channels,
//! providing a centralized way to send queries to backends.

use crate::messages::{InterfaceControlQueryMessage, TcQueryMessage};
use tcgui_shared::{InterfaceControlOperation, InterfaceControlRequest, TcOperation, TcRequest};
use tokio::sync::mpsc;
use tracing::{error, info};

/// Manager for query channels and operations.
pub struct QueryManager {
    /// Channel for sending TC queries to specific backends
    tc_query_sender: Option<mpsc::UnboundedSender<TcQueryMessage>>,
    /// Channel for sending interface control queries to specific backends
    interface_query_sender: Option<mpsc::UnboundedSender<InterfaceControlQueryMessage>>,
}

impl QueryManager {
    /// Creates a new query manager.
    pub fn new() -> Self {
        Self {
            tc_query_sender: None,
            interface_query_sender: None,
        }
    }

    /// Sets up the TC query channel.
    pub fn setup_tc_query_channel(&mut self, sender: mpsc::UnboundedSender<TcQueryMessage>) {
        info!("Setting up TC query channel for multi-backend communication");
        self.tc_query_sender = Some(sender);
    }

    /// Sets up the interface query channel.
    pub fn setup_interface_query_channel(
        &mut self,
        sender: mpsc::UnboundedSender<InterfaceControlQueryMessage>,
    ) {
        info!("Setting up interface query channel for multi-backend communication");
        self.interface_query_sender = Some(sender);
    }

    /// Sends an apply TC query to a backend.
    #[allow(clippy::too_many_arguments)] // Legacy method maintained for backward compatibility
    pub fn apply_tc(
        &self,
        backend_name: String,
        namespace: String,
        interface: String,
        loss: f32,
        correlation: Option<f32>,
        delay_ms: Option<f32>,
        delay_jitter_ms: Option<f32>,
        delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        reorder_correlation: Option<f32>,
        reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> Result<(), String> {
        if let Some(sender) = &self.tc_query_sender {
            let request = TcRequest {
                namespace: namespace.clone(),
                interface: interface.clone(),
                operation: TcOperation::Apply {
                    loss,
                    correlation,
                    delay_ms,
                    delay_jitter_ms,
                    delay_correlation,
                    duplicate_percent,
                    duplicate_correlation,
                    reorder_percent,
                    reorder_correlation,
                    reorder_gap,
                    corrupt_percent,
                    corrupt_correlation,
                    rate_limit_kbps,
                },
            };
            let tc_query_message = TcQueryMessage {
                backend_name: backend_name.clone(),
                request,
                response_sender: None, // No response handling needed for fire-and-forget
            };

            if let Err(e) = sender.send(tc_query_message) {
                let error_msg = format!(
                    "Failed to send TC apply query to backend '{}': {}",
                    backend_name, e
                );
                error!("{}", error_msg);
                return Err(error_msg);
            }

            info!(
                "Sent TC apply query to backend '{}' for {}/{}",
                backend_name, namespace, interface
            );
            Ok(())
        } else {
            let error_msg = "TC query sender not available".to_string();
            error!("{}", error_msg);
            Err(error_msg)
        }
    }

    /// Sends an enable interface query to a backend.
    pub fn enable_interface(
        &self,
        backend_name: String,
        namespace: String,
        interface: String,
    ) -> Result<(), String> {
        if let Some(sender) = &self.interface_query_sender {
            let request = InterfaceControlRequest {
                namespace: namespace.clone(),
                interface: interface.clone(),
                operation: InterfaceControlOperation::Enable,
            };
            let query_message = InterfaceControlQueryMessage {
                backend_name: backend_name.clone(),
                request,
                response_sender: None, // No response handling needed for fire-and-forget
            };

            if let Err(e) = sender.send(query_message) {
                let error_msg = format!(
                    "Failed to send interface enable query to backend '{}': {}",
                    backend_name, e
                );
                error!("{}", error_msg);
                return Err(error_msg);
            }

            info!(
                "Sent interface enable query to backend '{}' for {}/{}",
                backend_name, namespace, interface
            );
            Ok(())
        } else {
            let error_msg = "Interface query sender not available".to_string();
            error!("{}", error_msg);
            Err(error_msg)
        }
    }

    /// Sends a disable interface query to a backend.
    pub fn disable_interface(
        &self,
        backend_name: String,
        namespace: String,
        interface: String,
    ) -> Result<(), String> {
        if let Some(sender) = &self.interface_query_sender {
            let request = InterfaceControlRequest {
                namespace: namespace.clone(),
                interface: interface.clone(),
                operation: InterfaceControlOperation::Disable,
            };
            let query_message = InterfaceControlQueryMessage {
                backend_name: backend_name.clone(),
                request,
                response_sender: None, // No response handling needed for fire-and-forget
            };

            if let Err(e) = sender.send(query_message) {
                let error_msg = format!(
                    "Failed to send interface disable query to backend '{}': {}",
                    backend_name, e
                );
                error!("{}", error_msg);
                return Err(error_msg);
            }

            info!(
                "Sent interface disable query to backend '{}' for {}/{}",
                backend_name, namespace, interface
            );
            Ok(())
        } else {
            let error_msg = "Interface query sender not available".to_string();
            error!("{}", error_msg);
            Err(error_msg)
        }
    }

    /// Checks if TC query channel is available.
    pub fn has_tc_query_channel(&self) -> bool {
        self.tc_query_sender.is_some()
    }

    /// Checks if interface query channel is available.
    pub fn has_interface_query_channel(&self) -> bool {
        self.interface_query_sender.is_some()
    }

    /// Checks if both channels are available.
    pub fn has_both_channels(&self) -> bool {
        self.has_tc_query_channel() && self.has_interface_query_channel()
    }
}

impl Default for QueryManager {
    fn default() -> Self {
        Self::new()
    }
}
