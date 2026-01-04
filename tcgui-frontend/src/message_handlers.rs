//! Message handlers for TC GUI frontend.
//!
//! This module contains handlers for different types of messages
//! that the application receives, separating the message handling
//! logic from the main application update loop.

use crate::backend_manager::BackendManager;
use crate::interface::TcInterface;
use crate::messages::{TcGuiMessage, TcInterfaceMessage};
use crate::query_manager::QueryManager;
use crate::scenario_manager::ScenarioManager;
use crate::ui_state::UiStateManager;
use iced::Task;
use tcgui_shared::{TcConfigUpdate, TcStatisticsUpdate};
use tracing::{info, warn};

/// Handles bandwidth update messages.
pub fn handle_bandwidth_update(
    backend_manager: &mut BackendManager,
    bandwidth_update: tcgui_shared::BandwidthUpdate,
) -> Task<TcGuiMessage> {
    let backend_name = &bandwidth_update.backend_name;

    if let Some(backend_group) = backend_manager.backends_mut().get_mut(backend_name) {
        if let Some(namespace_group) = backend_group
            .namespaces
            .get_mut(&bandwidth_update.namespace)
        {
            if let Some(tc_interface) = namespace_group
                .tc_interfaces
                .get_mut(&bandwidth_update.interface)
            {
                tracing::trace!(
                    "Updating bandwidth stats for interface '{}' in namespace '{}' of backend '{}'",
                    bandwidth_update.interface,
                    bandwidth_update.namespace,
                    bandwidth_update.backend_name
                );
                tc_interface.update_bandwidth_stats(bandwidth_update.stats.clone());
            } else {
                let available_interfaces: Vec<String> =
                    namespace_group.tc_interfaces.keys().cloned().collect();
                tracing::warn!(
                    "Could not find interface '{}' in namespace '{}' of backend '{}'. Available interfaces in namespace: {:?}",
                    bandwidth_update.interface,
                    bandwidth_update.namespace,
                    bandwidth_update.backend_name,
                    available_interfaces
                );
            }
        } else {
            let available_namespaces: Vec<String> =
                backend_group.namespaces.keys().cloned().collect();
            tracing::warn!(
                "Could not find namespace '{}' in backend '{}' for interface '{}'. Available namespaces: {:?}",
                bandwidth_update.namespace,
                bandwidth_update.backend_name,
                bandwidth_update.interface,
                available_namespaces
            );
        }
    } else {
        let available_backends: Vec<String> = backend_manager.backends().keys().cloned().collect();
        tracing::warn!(
            "Could not find backend '{}' for interface '{}/{}'. Available backends: {:?}",
            bandwidth_update.backend_name,
            bandwidth_update.namespace,
            bandwidth_update.interface,
            available_backends
        );
    }

    Task::none()
}

/// Apply complete TC configuration to interface component
fn apply_tc_configuration_to_interface(
    tc_interface: &mut TcInterface,
    tc_config_update: &TcConfigUpdate,
) {
    // Apply TC configuration parameters directly (no master TC checkbox anymore)

    // If there's a configuration, apply the specific parameters with values
    if let Some(config) = &tc_config_update.configuration {
        info!(
            "Applying TC configuration parameters to {}/{}: loss={}%, delay={:?}ms, duplicate={:?}%, reorder={:?}%, corrupt={:?}%, rate={:?}kbps",
            tc_config_update.namespace,
            tc_config_update.interface,
            config.loss,
            config.delay_ms,
            config.duplicate_percent,
            config.reorder_percent,
            config.corrupt_percent,
            config.rate_limit_kbps
        );

        // Apply loss configuration - update checkbox based on actual value
        if config.loss > 0.0 {
            let _ = tc_interface.update(TcInterfaceMessage::LossToggled(true));
            let _ = tc_interface.update(TcInterfaceMessage::LossChanged(config.loss));

            if let Some(correlation) = config.correlation
                && correlation > 0.0
            {
                let _ = tc_interface.update(TcInterfaceMessage::CorrelationChanged(correlation));
            }
        } else {
            // If loss is 0.0, uncheck the Loss checkbox
            let _ = tc_interface.update(TcInterfaceMessage::LossToggled(false));
        }

        // Apply delay configuration - update checkbox based on actual value
        if let Some(delay_ms) = config.delay_ms {
            if delay_ms > 0.0 {
                let _ = tc_interface.update(TcInterfaceMessage::DelayToggled(true));
                let _ = tc_interface.update(TcInterfaceMessage::DelayChanged(delay_ms));

                if let Some(jitter) = config.delay_jitter_ms
                    && jitter > 0.0
                {
                    let _ = tc_interface.update(TcInterfaceMessage::DelayJitterChanged(jitter));
                }

                if let Some(delay_corr) = config.delay_correlation
                    && delay_corr > 0.0
                {
                    let _ = tc_interface
                        .update(TcInterfaceMessage::DelayCorrelationChanged(delay_corr));
                }
            } else {
                // If delay is 0.0, uncheck the Delay checkbox
                let _ = tc_interface.update(TcInterfaceMessage::DelayToggled(false));
            }
        } else {
            // If delay is None, uncheck the Delay checkbox
            let _ = tc_interface.update(TcInterfaceMessage::DelayToggled(false));
        }

        // Apply duplicate configuration - set parameters directly (auto-enable logic will handle checkbox)
        if let Some(duplicate_percent) = config.duplicate_percent {
            if duplicate_percent > 0.0 {
                // Set duplicate parameter - this will auto-enable the checkbox
                let _ = tc_interface.update(TcInterfaceMessage::DuplicatePercentageChanged(
                    duplicate_percent,
                ));

                if let Some(dup_corr) = config.duplicate_correlation
                    && dup_corr > 0.0
                {
                    let _ = tc_interface
                        .update(TcInterfaceMessage::DuplicateCorrelationChanged(dup_corr));
                }
            } else {
                // If duplicate is 0.0, disable the Duplicate checkbox if enabled
                if tc_interface.duplicate_enabled() {
                    let _ = tc_interface.update(TcInterfaceMessage::DuplicateToggled(()));
                }
            }
        } else {
            // If duplicate is None, disable the Duplicate checkbox if enabled
            if tc_interface.duplicate_enabled() {
                let _ = tc_interface.update(TcInterfaceMessage::DuplicateToggled(()));
            }
        }

        // Apply reorder configuration - set parameters directly (auto-enable logic will handle checkbox)
        if let Some(reorder_percent) = config.reorder_percent {
            if reorder_percent > 0.0 {
                // Set reorder parameter - this will auto-enable the checkbox
                let _ = tc_interface.update(TcInterfaceMessage::ReorderPercentageChanged(
                    reorder_percent,
                ));

                if let Some(reorder_corr) = config.reorder_correlation
                    && reorder_corr > 0.0
                {
                    let _ = tc_interface
                        .update(TcInterfaceMessage::ReorderCorrelationChanged(reorder_corr));
                }

                if let Some(gap) = config.reorder_gap
                    && gap > 0
                {
                    let _ = tc_interface.update(TcInterfaceMessage::ReorderGapChanged(gap));
                }
            } else {
                // If reorder is 0.0, disable the Reorder checkbox
                if tc_interface.reorder_enabled() {
                    let _ = tc_interface.update(TcInterfaceMessage::ReorderToggled(()));
                }
            }
        } else {
            // If reorder is None, disable the Reorder checkbox
            if tc_interface.reorder_enabled() {
                let _ = tc_interface.update(TcInterfaceMessage::ReorderToggled(()));
            }
        }

        // Apply corrupt configuration - set parameters directly (auto-enable logic will handle checkbox)
        if let Some(corrupt_percent) = config.corrupt_percent {
            if corrupt_percent > 0.0 {
                // Set corrupt parameter - this will auto-enable the checkbox
                let _ = tc_interface.update(TcInterfaceMessage::CorruptPercentageChanged(
                    corrupt_percent,
                ));

                if let Some(corrupt_corr) = config.corrupt_correlation
                    && corrupt_corr > 0.0
                {
                    let _ = tc_interface
                        .update(TcInterfaceMessage::CorruptCorrelationChanged(corrupt_corr));
                }
            } else {
                // If corrupt is 0.0, disable the Corrupt checkbox
                if tc_interface.corrupt_enabled() {
                    let _ = tc_interface.update(TcInterfaceMessage::CorruptToggled(()));
                }
            }
        } else {
            // If corrupt is None, disable the Corrupt checkbox
            if tc_interface.corrupt_enabled() {
                let _ = tc_interface.update(TcInterfaceMessage::CorruptToggled(()));
            }
        }

        // Apply rate limit configuration - set parameters directly (auto-enable logic will handle checkbox)
        if let Some(rate_kbps) = config.rate_limit_kbps {
            if rate_kbps > 0 {
                // Set rate limit parameter - this will auto-enable the checkbox
                let _ = tc_interface.update(TcInterfaceMessage::RateLimitChanged(rate_kbps));
            } else {
                // If rate is 0, disable the Rate Limit checkbox
                if tc_interface.rate_limit_enabled() {
                    let _ = tc_interface.update(TcInterfaceMessage::RateLimitToggled(()));
                }
            }
        } else {
            // If rate is None, disable the Rate Limit checkbox
            if tc_interface.rate_limit_enabled() {
                let _ = tc_interface.update(TcInterfaceMessage::RateLimitToggled(()));
            }
        }
    } else if !tc_config_update.has_tc {
        // Only disable features if we're certain there's no TC configuration at all
        // This handles the case where the interface truly has no TC configured
        let _ = tc_interface.update(TcInterfaceMessage::LossToggled(false));
        let _ = tc_interface.update(TcInterfaceMessage::DelayToggled(false));

        // Disable unit-type toggles if they are currently enabled
        if tc_interface.duplicate_enabled() {
            let _ = tc_interface.update(TcInterfaceMessage::DuplicateToggled(()));
        }
        if tc_interface.reorder_enabled() {
            let _ = tc_interface.update(TcInterfaceMessage::ReorderToggled(()));
        }
        if tc_interface.corrupt_enabled() {
            let _ = tc_interface.update(TcInterfaceMessage::CorruptToggled(()));
        }
        if tc_interface.rate_limit_enabled() {
            let _ = tc_interface.update(TcInterfaceMessage::RateLimitToggled(()));
        }
    }
    // If has_tc is true but configuration is None, we don't override any checkboxes
    // This allows the user to interact with the interface without being overridden
}

/// Handles TC configuration update messages from backend.
pub fn handle_tc_config_update(
    backend_manager: &mut BackendManager,
    tc_config_update: TcConfigUpdate,
) -> Task<TcGuiMessage> {
    let backend_name = &tc_config_update.backend_name;

    info!(
        "Received TC config update for interface '{}' in namespace '{}' of backend '{}': has_tc={}",
        tc_config_update.interface,
        tc_config_update.namespace,
        tc_config_update.backend_name,
        tc_config_update.has_tc
    );

    if let Some(backend_group) = backend_manager.backends_mut().get_mut(backend_name) {
        if let Some(namespace_group) = backend_group
            .namespaces
            .get_mut(&tc_config_update.namespace)
        {
            if let Some(tc_interface) = namespace_group
                .tc_interfaces
                .get_mut(&tc_config_update.interface)
            {
                // Find the corresponding network interface from the namespace data
                let network_interface = namespace_group
                    .namespace
                    .interfaces
                    .iter()
                    .find(|iface| iface.name == tc_config_update.interface)
                    .cloned(); // Clone to avoid borrowing issues

                if let Some(network_interface) = network_interface {
                    // Update the TC interface component based on the received configuration
                    tc_interface.update_from_backend(&network_interface);
                }

                // Apply the complete TC configuration from the backend
                apply_tc_configuration_to_interface(tc_interface, &tc_config_update);
            } else {
                warn!(
                    "Could not find TC interface '{}' in namespace '{}' of backend '{}' to update TC config",
                    tc_config_update.interface,
                    tc_config_update.namespace,
                    tc_config_update.backend_name
                );
            }
        } else {
            warn!(
                "Could not find namespace '{}' in backend '{}' for TC config update on interface '{}'",
                tc_config_update.namespace,
                tc_config_update.backend_name,
                tc_config_update.interface
            );
        }
    } else {
        warn!(
            "Could not find backend '{}' for TC config update on interface '{}/{}'",
            tc_config_update.backend_name, tc_config_update.namespace, tc_config_update.interface
        );
    }

    Task::none()
}

/// Handles TC statistics update messages from backend.
pub fn handle_tc_statistics_update(
    backend_manager: &mut BackendManager,
    tc_stats_update: TcStatisticsUpdate,
) -> Task<TcGuiMessage> {
    let backend_name = &tc_stats_update.backend_name;

    if let Some(backend_group) = backend_manager.backends_mut().get_mut(backend_name)
        && let Some(namespace_group) = backend_group.namespaces.get_mut(&tc_stats_update.namespace)
        && let Some(tc_interface) = namespace_group
            .tc_interfaces
            .get_mut(&tc_stats_update.interface)
    {
        // Update the TC interface with the received statistics
        tc_interface.update_tc_statistics(
            tc_stats_update.stats_basic,
            tc_stats_update.stats_queue,
            tc_stats_update.stats_rate_est,
        );
    }

    Task::none()
}

/// Handles TC interface messages (user interactions with interface components).
pub fn handle_tc_interface_message(
    backend_manager: &mut BackendManager,
    backend_name: String,
    namespace: String,
    interface_name: String,
    tc_message: TcInterfaceMessage,
) -> Task<TcGuiMessage> {
    // Use the provided backend and namespace to route the message directly
    if let Some(backend_group) = backend_manager.backends_mut().get_mut(&backend_name)
        && let Some(namespace_group) = backend_group.namespaces.get_mut(&namespace)
        && let Some(tc_interface) = namespace_group.tc_interfaces.get_mut(&interface_name)
    {
        let task = tc_interface.update(tc_message.clone());

        // Handle messages that need to be sent to backend
        let backend_task = match tc_message {
            TcInterfaceMessage::LossChanged(_)
            | TcInterfaceMessage::CorrelationChanged(_)
            | TcInterfaceMessage::DelayChanged(_)
            | TcInterfaceMessage::DelayJitterChanged(_)
            | TcInterfaceMessage::DelayCorrelationChanged(_)
            | TcInterfaceMessage::DuplicatePercentageChanged(_)
            | TcInterfaceMessage::DuplicateCorrelationChanged(_)
            | TcInterfaceMessage::ReorderPercentageChanged(_)
            | TcInterfaceMessage::ReorderCorrelationChanged(_)
            | TcInterfaceMessage::ReorderGapChanged(_)
            | TcInterfaceMessage::CorruptPercentageChanged(_)
            | TcInterfaceMessage::CorruptCorrelationChanged(_)
            | TcInterfaceMessage::RateLimitChanged(_) => {
                // Auto-apply TC changes immediately
                let correlation_value = if tc_interface.correlation_value() > 0.0 {
                    Some(tc_interface.correlation_value())
                } else {
                    None
                };

                // For parameter changes, respect feature checkbox enabled states
                let loss_value = if tc_interface.loss_enabled() && tc_interface.loss() > 0.0 {
                    tc_interface.loss()
                } else {
                    0.0
                };

                let delay_ms_value = if tc_interface.delay_enabled() {
                    if tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        Some(10.0) // Default 10ms delay when Delay checkbox enabled but slider not moved
                    }
                } else {
                    None
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: loss_value,
                    correlation: correlation_value,
                    delay_ms: delay_ms_value,
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Handle LossToggled separately to properly handle enabling/disabling Loss feature
            TcInterfaceMessage::LossToggled(enabled) => {
                // When Loss checkbox is toggled, send appropriate loss value based on enabled state
                let loss_value = if enabled {
                    // When enabling Loss checkbox, use current value or meaningful default
                    if tc_interface.loss() > 0.0 {
                        tc_interface.loss()
                    } else {
                        1.0 // Default 1% loss when Loss feature is enabled but slider not moved
                    }
                } else {
                    // When disabling Loss checkbox, send 0.0% to remove loss
                    0.0
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: loss_value,
                    correlation: None,
                    delay_ms: if tc_interface.delay_enabled() && tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        None
                    },
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Handle DelayToggled separately to properly handle enabling/disabling Delay feature
            TcInterfaceMessage::DelayToggled(enabled) => {
                // When Delay checkbox is toggled, send appropriate delay value based on enabled state
                let delay_ms_value = if enabled {
                    // When enabling Delay checkbox, use current value or meaningful default
                    if tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        Some(10.0) // Default 10ms delay when Delay feature is enabled but slider not moved
                    }
                } else {
                    // When disabling Delay checkbox, send None to remove delay
                    None
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: if tc_interface.loss_enabled() && tc_interface.loss() > 0.0 {
                        tc_interface.loss()
                    } else {
                        0.0
                    },
                    correlation: if tc_interface.correlation_value() > 0.0 {
                        Some(tc_interface.correlation_value())
                    } else {
                        None
                    },
                    delay_ms: delay_ms_value,
                    delay_jitter_ms: if enabled && tc_interface.delay_jitter_ms() > 0.0 {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if enabled && tc_interface.delay_correlation() > 0.0 {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Handle DuplicateToggled separately to properly handle enabling/disabling Duplicate feature
            TcInterfaceMessage::DuplicateToggled(_) => {
                // When Duplicate checkbox is toggled, send appropriate duplicate value based on enabled state
                let duplicate_percent_value = if tc_interface.duplicate_enabled() {
                    // When enabling Duplicate checkbox, use current value or meaningful default
                    if tc_interface.duplicate_percentage() > 0.0 {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        Some(1.0) // Default 1% duplicate when Duplicate feature is enabled but slider not moved
                    }
                } else {
                    // When disabling Duplicate checkbox, send None to remove duplicate
                    None
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: if tc_interface.loss_enabled() && tc_interface.loss() > 0.0 {
                        tc_interface.loss()
                    } else {
                        0.0
                    },
                    correlation: if tc_interface.correlation_value() > 0.0 {
                        Some(tc_interface.correlation_value())
                    } else {
                        None
                    },
                    delay_ms: if tc_interface.delay_enabled() && tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        None
                    },
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: duplicate_percent_value,
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Handle ReorderToggled separately to properly handle enabling/disabling Reorder feature
            TcInterfaceMessage::ReorderToggled(_) => {
                // When Reorder checkbox is toggled, send appropriate reorder value based on enabled state
                let reorder_percent_value = if tc_interface.reorder_enabled() {
                    // When enabling Reorder checkbox, use current value or meaningful default
                    if tc_interface.reorder_percentage() > 0.0 {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        Some(1.0) // Default 1% reorder when Reorder feature is enabled but slider not moved
                    }
                } else {
                    // When disabling Reorder checkbox, send None to remove reorder
                    None
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: if tc_interface.loss_enabled() && tc_interface.loss() > 0.0 {
                        tc_interface.loss()
                    } else {
                        0.0
                    },
                    correlation: if tc_interface.correlation_value() > 0.0 {
                        Some(tc_interface.correlation_value())
                    } else {
                        None
                    },
                    delay_ms: if tc_interface.delay_enabled() && tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        None
                    },
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: reorder_percent_value,
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && reorder_percent_value.is_some()
                        && tc_interface.reorder_gap() > 0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Handle CorruptToggled separately to properly handle enabling/disabling Corrupt feature
            TcInterfaceMessage::CorruptToggled(_) => {
                // When Corrupt checkbox is toggled, send appropriate corrupt value based on enabled state
                let corrupt_percent_value = if tc_interface.corrupt_enabled() {
                    // When enabling Corrupt checkbox, use current value or meaningful default
                    if tc_interface.corrupt_percentage() > 0.0 {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        Some(1.0) // Default 1% corrupt when Corrupt feature is enabled but slider not moved
                    }
                } else {
                    // When disabling Corrupt checkbox, send None to remove corrupt
                    None
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: if tc_interface.loss_enabled() && tc_interface.loss() > 0.0 {
                        tc_interface.loss()
                    } else {
                        0.0
                    },
                    correlation: if tc_interface.correlation_value() > 0.0 {
                        Some(tc_interface.correlation_value())
                    } else {
                        None
                    },
                    delay_ms: if tc_interface.delay_enabled() && tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        None
                    },
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: corrupt_percent_value,
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Handle RateLimitToggled separately to properly handle enabling/disabling Rate Limit feature
            TcInterfaceMessage::RateLimitToggled(_) => {
                // When Rate Limit checkbox is toggled, send appropriate rate limit value based on enabled state
                let rate_limit_value = if tc_interface.rate_limit_enabled() {
                    // When enabling Rate Limit checkbox, use current value or meaningful default
                    if tc_interface.rate_limit_kbps() > 0 {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        Some(1000) // Default 1000 kbps rate limit when Rate Limit feature is enabled but slider not moved
                    }
                } else {
                    // When disabling Rate Limit checkbox, send None to remove rate limit
                    None
                };

                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: if tc_interface.loss_enabled() && tc_interface.loss() > 0.0 {
                        tc_interface.loss()
                    } else {
                        0.0
                    },
                    correlation: if tc_interface.correlation_value() > 0.0 {
                        Some(tc_interface.correlation_value())
                    } else {
                        None
                    },
                    delay_ms: if tc_interface.delay_enabled() && tc_interface.delay_ms() > 0.0 {
                        Some(tc_interface.delay_ms())
                    } else {
                        None
                    },
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: rate_limit_value,
                })
            }
            TcInterfaceMessage::InterfaceToggled(enabled) => {
                // Send interface up/down command to backend
                if enabled {
                    Task::done(TcGuiMessage::EnableInterface {
                        backend_name: backend_name.clone(),
                        namespace: namespace.clone(),
                        interface: interface_name.clone(),
                    })
                } else {
                    Task::done(TcGuiMessage::DisableInterface {
                        backend_name: backend_name.clone(),
                        namespace: namespace.clone(),
                        interface: interface_name.clone(),
                    })
                }
            }
            // Preset messages - apply all TC settings from preset
            TcInterfaceMessage::PresetSelected(_) => {
                // Preset was applied in TcInterface::update(), now send to backend
                Task::done(TcGuiMessage::ApplyTc {
                    backend_name: backend_name.clone(),
                    namespace: namespace.clone(),
                    interface: interface_name.clone(),
                    loss: if tc_interface.loss_enabled() {
                        tc_interface.loss()
                    } else {
                        0.0
                    },
                    correlation: if tc_interface.loss_enabled()
                        && tc_interface.correlation_value() > 0.0
                    {
                        Some(tc_interface.correlation_value())
                    } else {
                        None
                    },
                    delay_ms: if tc_interface.delay_enabled() {
                        Some(tc_interface.delay_ms())
                    } else {
                        None
                    },
                    delay_jitter_ms: if tc_interface.delay_enabled()
                        && tc_interface.delay_jitter_ms() > 0.0
                    {
                        Some(tc_interface.delay_jitter_ms())
                    } else {
                        None
                    },
                    delay_correlation: if tc_interface.delay_enabled()
                        && tc_interface.delay_correlation() > 0.0
                    {
                        Some(tc_interface.delay_correlation())
                    } else {
                        None
                    },
                    duplicate_percent: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_percentage() > 0.0
                    {
                        Some(tc_interface.duplicate_percentage())
                    } else {
                        None
                    },
                    duplicate_correlation: if tc_interface.duplicate_enabled()
                        && tc_interface.duplicate_correlation() > 0.0
                    {
                        Some(tc_interface.duplicate_correlation())
                    } else {
                        None
                    },
                    reorder_percent: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_percentage())
                    } else {
                        None
                    },
                    reorder_correlation: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_correlation() > 0.0
                    {
                        Some(tc_interface.reorder_correlation())
                    } else {
                        None
                    },
                    reorder_gap: if tc_interface.reorder_enabled()
                        && tc_interface.reorder_percentage() > 0.0
                    {
                        Some(tc_interface.reorder_gap())
                    } else {
                        None
                    },
                    corrupt_percent: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_percentage() > 0.0
                    {
                        Some(tc_interface.corrupt_percentage())
                    } else {
                        None
                    },
                    corrupt_correlation: if tc_interface.corrupt_enabled()
                        && tc_interface.corrupt_correlation() > 0.0
                    {
                        Some(tc_interface.corrupt_correlation())
                    } else {
                        None
                    },
                    rate_limit_kbps: if tc_interface.rate_limit_enabled()
                        && tc_interface.rate_limit_kbps() > 0
                    {
                        Some(tc_interface.rate_limit_kbps())
                    } else {
                        None
                    },
                })
            }
            // Toggle preset dropdown is UI-only, no backend action needed
            TcInterfaceMessage::TogglePresetDropdown => Task::none(),
            // Clear all features - remove the TC qdisc entirely
            TcInterfaceMessage::ClearAllFeatures => Task::done(TcGuiMessage::RemoveTc {
                backend_name: backend_name.clone(),
                namespace: namespace.clone(),
                interface: interface_name.clone(),
            }),
            // Toggle chart visibility is UI-only, no backend action needed
            TcInterfaceMessage::ToggleChart => Task::none(),
            // Diagnostics messages - StartDiagnostics triggers backend query
            TcInterfaceMessage::StartDiagnostics => Task::done(TcGuiMessage::RunDiagnostics {
                backend_name: backend_name.clone(),
                namespace: namespace.clone(),
                interface: interface_name.clone(),
            }),
            // DiagnosticsComplete and DismissDiagnostics are UI-only state updates
            TcInterfaceMessage::DiagnosticsComplete(_) => Task::none(),
            TcInterfaceMessage::DismissDiagnostics => Task::none(),
        };

        let backend_copy = backend_name.clone();
        let ns_copy = namespace.clone();
        let iface_name_copy = interface_name.clone();
        let interface_task = task.map(move |msg| {
            TcGuiMessage::TcInterfaceMessage(
                backend_copy.clone(),
                ns_copy.clone(),
                iface_name_copy.clone(),
                msg,
            )
        });
        return Task::batch([interface_task, backend_task]);
    }
    Task::none()
}

/// Handles backend connection status changes.
pub fn handle_backend_connection_status(
    backend_manager: &mut BackendManager,
    _query_manager: &mut QueryManager,
    backend_name: String,
    connected: bool,
) -> Task<TcGuiMessage> {
    info!(
        "Backend '{}' connection status: {}",
        backend_name,
        if connected {
            "connected"
        } else {
            "disconnected"
        }
    );

    if let Some(backend_group) = backend_manager.backends_mut().get_mut(&backend_name) {
        backend_group.is_connected = connected;

        if connected {
            info!(
                "Backend '{}' connected - channels should be available from Zenoh manager",
                backend_name
            );
        } else {
            info!(
                "Backend '{}' disconnected - channels will be preserved for reconnection",
                backend_name
            );
            // Don't clear channels - they will be refreshed when Zenoh reconnects
        }
    }

    Task::none()
}

/// Handles namespace visibility toggles.
pub fn handle_toggle_namespace_visibility(
    ui_state: &mut UiStateManager,
    backend_name: String,
    namespace_name: String,
) -> Task<TcGuiMessage> {
    ui_state.toggle_namespace_visibility(&backend_name, &namespace_name);
    info!(
        "Toggled namespace visibility for '{}/{}' - now {}",
        backend_name,
        namespace_name,
        if ui_state.is_namespace_hidden(&backend_name, &namespace_name) {
            "hidden"
        } else {
            "visible"
        }
    );
    Task::none()
}

/// Handles showing all hidden namespaces.
pub fn handle_show_all_namespaces(ui_state: &mut UiStateManager) -> Task<TcGuiMessage> {
    ui_state.show_all_namespaces();
    info!("Showing all namespaces");
    Task::none()
}

/// Handles resetting all UI state.
pub fn handle_reset_ui_state(ui_state: &mut UiStateManager) -> Task<TcGuiMessage> {
    ui_state.reset_all();
    info!("Reset all UI visibility state");
    Task::none()
}

/// Handles showing all hidden backends.
pub fn handle_show_all_backends(ui_state: &mut UiStateManager) -> Task<TcGuiMessage> {
    ui_state.show_all_backends();
    info!("Showing all backends");
    Task::none()
}

/// Handles TC operations (apply/remove).
#[allow(clippy::too_many_arguments)] // Legacy handler maintained for backward compatibility
pub fn handle_apply_tc(
    query_manager: &QueryManager,
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
) -> Task<TcGuiMessage> {
    if let Err(e) = query_manager.apply_tc(
        backend_name.clone(),
        namespace,
        interface,
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
    ) {
        tracing::error!("Failed to apply TC: {}", e);
    }
    Task::none()
}

/// Handles TC removal operations (clears netem qdisc).
pub fn handle_remove_tc(
    query_manager: &QueryManager,
    backend_name: String,
    namespace: String,
    interface: String,
) -> Task<TcGuiMessage> {
    if let Err(e) = query_manager.remove_tc(backend_name.clone(), namespace, interface) {
        tracing::error!("Failed to remove TC: {}", e);
    }
    Task::none()
}

/// Handles interface enable operations.
pub fn handle_enable_interface(
    query_manager: &QueryManager,
    backend_name: String,
    namespace: String,
    interface: String,
) -> Task<TcGuiMessage> {
    if let Err(e) = query_manager.enable_interface(backend_name.clone(), namespace, interface) {
        tracing::error!("Failed to enable interface: {}", e);
    }
    Task::none()
}

/// Handles interface disable operations.
pub fn handle_disable_interface(
    query_manager: &QueryManager,
    backend_name: String,
    namespace: String,
    interface: String,
) -> Task<TcGuiMessage> {
    if let Err(e) = query_manager.disable_interface(backend_name.clone(), namespace, interface) {
        tracing::error!("Failed to disable interface: {}", e);
    }
    Task::none()
}

/// Handles backend cleanup operations.
pub fn handle_cleanup_stale_backends(
    backend_manager: &mut BackendManager,
    bandwidth_history: &mut crate::bandwidth_history::BandwidthHistoryManager,
    _query_manager: &mut QueryManager,
    ui_state: &mut UiStateManager,
    scenario_manager: &mut ScenarioManager,
) -> Task<TcGuiMessage> {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut backends_to_remove = Vec::new();

    for (backend_name, backend_group) in backend_manager.backends() {
        if let Some(disconnected_at) = backend_group.disconnected_at {
            let disconnected_duration = current_time.saturating_sub(disconnected_at);
            if disconnected_duration >= 10 {
                info!(
                    "Backend '{}' has been disconnected for {} seconds, removing from list",
                    backend_name, disconnected_duration
                );
                backends_to_remove.push(backend_name.clone());
            }
        }
    }

    // Remove stale backends
    for backend_name in &backends_to_remove {
        if let Some(backend_group) = backend_manager.backends_mut().remove(backend_name) {
            info!(
                "Removed stale backend '{}' with {} namespaces and {} total interfaces",
                backend_name,
                backend_group.namespaces.len(),
                backend_group
                    .namespaces
                    .values()
                    .map(|ns| ns.tc_interfaces.len())
                    .sum::<usize>()
            );

            // Temporarily toggle backend visibility before cleanup (demo usage of unused method)
            ui_state.toggle_backend_visibility(backend_name);
            // Clean up UI state for this backend
            ui_state.cleanup_backend_state(backend_name);
            // Clean up scenario state for this backend
            scenario_manager.cleanup_backend_state(backend_name);
            // Clean up bandwidth history for this backend
            bandwidth_history.remove_backend(backend_name);
        }
    }

    // Remove stale backends but preserve query channels
    // Channels will be automatically refreshed when Zenoh reconnects
    if !backends_to_remove.is_empty() {
        backend_manager.cleanup_stale_backends();

        // Log scenario manager stats after cleanup
        let stats = scenario_manager.get_stats();
        info!(
            "Removed {} stale backends - preserving query channels for future connections. {}",
            backends_to_remove.len(),
            stats
        );
    }

    Task::none()
}

/// Handles running diagnostics for an interface.
pub fn handle_run_diagnostics(
    query_manager: &QueryManager,
    backend_manager: &mut BackendManager,
    backend_name: String,
    namespace: String,
    interface: String,
) -> Task<TcGuiMessage> {
    info!(
        "Running diagnostics for {}/{}/{}",
        backend_name, namespace, interface
    );

    // Mark diagnostics as running in the interface state
    if let Some(backend_group) = backend_manager.backends_mut().get_mut(&backend_name)
        && let Some(namespace_group) = backend_group.namespaces.get_mut(&namespace)
        && let Some(tc_interface) = namespace_group.tc_interfaces.get_mut(&interface)
    {
        let _ = tc_interface.update(TcInterfaceMessage::StartDiagnostics);
    }

    // Send diagnostics query to backend
    if let Err(e) =
        query_manager.run_diagnostics(backend_name.clone(), namespace.clone(), interface.clone())
    {
        warn!("Failed to run diagnostics: {}", e);
    }

    Task::none()
}

/// Handles diagnostics result from backend.
pub fn handle_diagnostics_result(
    backend_manager: &mut BackendManager,
    backend_name: String,
    namespace: String,
    interface: String,
    response: tcgui_shared::DiagnosticsResponse,
) -> Task<TcGuiMessage> {
    info!(
        "Received diagnostics result for {}/{}/{}: {}",
        backend_name, namespace, interface, response.message
    );

    // Update the interface with diagnostics result
    if let Some(backend_group) = backend_manager.backends_mut().get_mut(&backend_name)
        && let Some(namespace_group) = backend_group.namespaces.get_mut(&namespace)
        && let Some(tc_interface) = namespace_group.tc_interfaces.get_mut(&interface)
    {
        let _ = tc_interface.update(TcInterfaceMessage::DiagnosticsComplete(response));
    }

    Task::none()
}
