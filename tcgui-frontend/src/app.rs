//! Main application state and message handling for TC GUI frontend.
//!
//! This module contains the refactored application logic following the Elm architecture
//! pattern used by Iced. The functionality has been broken down into smaller, more
//! manageable modules for better maintainability.

use iced::event::{self, Event};
use iced::keyboard::{Event as KeyboardEvent, Key, Modifiers};
use iced::mouse::{Event as MouseEvent, ScrollDelta};
use iced::{Element, Subscription, Task};
use tcgui_shared::ZenohConfig;

use crate::backend_manager::BackendManager;
use crate::message_handlers::*;
use crate::messages::{TcGuiMessage, ZenohEvent};
use crate::query_manager::QueryManager;
use crate::scenario_manager::ScenarioManager;
use crate::ui_state::UiStateManager;
use crate::view::render_main_view;
use crate::zenoh_manager::ZenohManager;

/// Main application state for the TC GUI frontend with modular architecture.
///
/// This struct represents the refactored application state following the Elm architecture.
/// The functionality has been broken down into specialized managers for better
/// maintainability and separation of concerns.
///
/// # Modular Architecture
///
/// * **Backend Management**: Handled by `BackendManager`
/// * **Query Operations**: Managed by `QueryManager`
/// * **UI State**: Controlled by `UiStateManager`
/// * **View Rendering**: Delegated to `view` module
/// * **Message Handling**: Processed by `message_handlers` module
///
/// # Message Flow
///
/// ```text
/// Zenoh Events → TcGui → Specialized Handlers → Managers → UI Updates
///       ↓           ↓            ↓               ↓         ↓
///   Raw Events   Routing    Processing      State     View
/// ```
pub struct TcGui {
    /// Backend management and state
    backend_manager: BackendManager,
    /// Query channel management for TC and interface operations
    query_manager: QueryManager,
    /// Scenario management and operations
    scenario_manager: ScenarioManager,
    /// UI state and visibility management
    ui_state: UiStateManager,
    /// Zenoh session management
    zenoh_manager: ZenohManager,
}

impl TcGui {
    /// Creates a new TcGui application instance with modular architecture.
    pub fn new() -> (Self, Task<TcGuiMessage>) {
        let app = Self {
            backend_manager: BackendManager::new(),
            query_manager: QueryManager::new(),
            scenario_manager: ScenarioManager::new(),
            ui_state: UiStateManager::new(),
            zenoh_manager: ZenohManager::new(ZenohConfig::default()),
        };

        (app, Task::none())
    }

    /// Creates a new TcGui application instance with custom Zenoh configuration.
    pub fn new_with_config(zenoh_config: ZenohConfig) -> (Self, Task<TcGuiMessage>) {
        let app = Self {
            backend_manager: BackendManager::new(),
            query_manager: QueryManager::new(),
            scenario_manager: ScenarioManager::new(),
            ui_state: UiStateManager::new(),
            zenoh_manager: ZenohManager::new(zenoh_config),
        };

        (app, Task::none())
    }

    /// Updates application state in response to messages (Elm architecture update function).
    ///
    /// This simplified update function delegates to specialized message handlers,
    /// making the code much more maintainable and focused.
    pub fn update(&mut self, message: TcGuiMessage) -> Task<TcGuiMessage> {
        match message {
            // Backend-related messages
            TcGuiMessage::InterfaceListUpdate(interface_update) => {
                self.backend_manager
                    .handle_interface_list_update(interface_update);
                Task::none()
            }
            TcGuiMessage::BackendHealthUpdate(health_status) => {
                self.backend_manager
                    .handle_backend_health_update(health_status);
                Task::none()
            }
            TcGuiMessage::BackendLiveliness {
                backend_name,
                alive,
            } => {
                self.backend_manager
                    .handle_backend_liveliness(backend_name.clone(), alive);
                // Auto-refresh scenarios when backend reconnects
                if alive {
                    self.scenario_manager.set_loading(&backend_name, true);
                    if let Err(e) = self.scenario_manager.request_scenarios(&backend_name) {
                        tracing::error!("Failed to auto-refresh scenarios on reconnect: {}", e);
                        self.scenario_manager.set_loading(&backend_name, false);
                    }
                }
                Task::none()
            }
            TcGuiMessage::InterfaceStateEvent(state_event) => {
                self.backend_manager
                    .handle_interface_state_event(state_event);
                Task::none()
            }
            TcGuiMessage::TcConfigUpdate(tc_config_update) => {
                handle_tc_config_update(&mut self.backend_manager, tc_config_update)
            }
            TcGuiMessage::BackendConnectionStatus {
                backend_name,
                connected,
            } => handle_backend_connection_status(
                &mut self.backend_manager,
                &mut self.query_manager,
                backend_name,
                connected,
            ),

            // Bandwidth updates
            TcGuiMessage::BandwidthUpdate(bandwidth_update) => {
                handle_bandwidth_update(&mut self.backend_manager, bandwidth_update)
            }

            // Interface messages
            TcGuiMessage::TcInterfaceMessage(
                backend_name,
                namespace,
                interface_name,
                tc_message,
            ) => handle_tc_interface_message(
                &mut self.backend_manager,
                backend_name,
                namespace,
                interface_name,
                tc_message,
            ),

            // Query channel setup
            TcGuiMessage::SetupTcQueryChannel(sender) => {
                self.query_manager.setup_tc_query_channel(sender);
                Task::none()
            }
            TcGuiMessage::SetupInterfaceQueryChannel(sender) => {
                self.query_manager.setup_interface_query_channel(sender);
                Task::none()
            }
            TcGuiMessage::SetupScenarioQueryChannel(sender) => {
                self.scenario_manager.setup_scenario_query_channel(sender);
                Task::none()
            }
            TcGuiMessage::SetupScenarioExecutionQueryChannel(sender) => {
                self.scenario_manager.setup_execution_query_channel(sender);
                Task::none()
            }

            // Scenario events
            TcGuiMessage::ScenarioExecutionUpdate(update) => {
                use tcgui_shared::scenario::ExecutionState;

                // Check if execution is in a terminal state before updating
                let is_terminal = matches!(
                    update.execution.state,
                    ExecutionState::Completed
                        | ExecutionState::Stopped
                        | ExecutionState::Failed { .. }
                );

                if is_terminal {
                    // Remove completed/stopped/failed executions from tracking
                    self.scenario_manager.remove_execution(
                        &update.backend_name,
                        &update.namespace,
                        &update.interface,
                    );
                } else {
                    // Update active execution state (with timestamp-based deduplication)
                    self.scenario_manager.handle_execution_update(*update);
                }
                Task::none()
            }

            // TC operations
            TcGuiMessage::ApplyTc {
                backend_name,
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
            } => handle_apply_tc(
                &self.query_manager,
                backend_name,
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
            ),

            TcGuiMessage::RemoveTc {
                backend_name,
                namespace,
                interface,
            } => handle_remove_tc(&self.query_manager, backend_name, namespace, interface),

            // Interface operations
            TcGuiMessage::EnableInterface {
                backend_name,
                namespace,
                interface,
            } => handle_enable_interface(&self.query_manager, backend_name, namespace, interface),
            TcGuiMessage::DisableInterface {
                backend_name,
                namespace,
                interface,
            } => handle_disable_interface(&self.query_manager, backend_name, namespace, interface),

            // Scenario operations
            TcGuiMessage::ListScenarios { backend_name } => {
                self.scenario_manager.set_loading(&backend_name, true);
                if let Err(e) = self.scenario_manager.request_scenarios(&backend_name) {
                    tracing::error!("Failed to request scenarios: {}", e);
                    self.scenario_manager.set_loading(&backend_name, false);
                }
                Task::none()
            }
            TcGuiMessage::StopScenarioExecution {
                backend_name,
                namespace,
                interface,
            } => {
                if let Err(e) =
                    self.scenario_manager
                        .stop_execution(&backend_name, &namespace, &interface)
                {
                    tracing::error!("Failed to stop scenario execution: {}", e);
                }
                Task::none()
            }
            TcGuiMessage::PauseScenarioExecution {
                backend_name,
                namespace,
                interface,
            } => {
                if let Err(e) =
                    self.scenario_manager
                        .pause_execution(&backend_name, &namespace, &interface)
                {
                    tracing::error!("Failed to pause scenario execution: {}", e);
                }
                Task::none()
            }
            TcGuiMessage::ResumeScenarioExecution {
                backend_name,
                namespace,
                interface,
            } => {
                if let Err(e) =
                    self.scenario_manager
                        .resume_execution(&backend_name, &namespace, &interface)
                {
                    tracing::error!("Failed to resume scenario execution: {}", e);
                }
                Task::none()
            }
            TcGuiMessage::ToggleExecutionTimeline {
                backend_name,
                namespace,
                interface,
            } => {
                self.scenario_manager.toggle_execution_timeline(
                    &backend_name,
                    &namespace,
                    &interface,
                );
                Task::none()
            }

            TcGuiMessage::ShowScenarioDetails { scenario } => {
                self.scenario_manager.show_scenario_details(scenario);
                Task::none()
            }
            TcGuiMessage::HideScenarioDetails => {
                self.scenario_manager.hide_scenario_details();
                Task::none()
            }
            TcGuiMessage::ScenarioSearchFilterChanged(filter) => {
                self.scenario_manager.set_search_filter(filter);
                Task::none()
            }
            TcGuiMessage::ScenarioSortOptionChanged(option) => {
                self.scenario_manager.set_sort_option(option);
                Task::none()
            }
            // Interface selection dialog messages
            TcGuiMessage::ShowInterfaceSelectionDialog {
                backend_name,
                scenario_id,
            } => {
                self.ui_state
                    .show_interface_selection_dialog(backend_name, scenario_id);
                Task::none()
            }
            TcGuiMessage::HideInterfaceSelectionDialog => {
                self.ui_state.hide_interface_selection_dialog();
                Task::none()
            }
            TcGuiMessage::SelectExecutionNamespace(namespace) => {
                self.ui_state.select_execution_namespace(namespace);
                Task::none()
            }
            TcGuiMessage::ToggleExecutionInterface(interface) => {
                self.ui_state.toggle_execution_interface(interface);
                Task::none()
            }
            TcGuiMessage::ToggleLoopExecution => {
                self.ui_state.toggle_loop_execution();
                Task::none()
            }
            TcGuiMessage::ConfirmScenarioExecution => {
                let dialog = self.ui_state.interface_selection_dialog();
                if let Some(namespace) = &dialog.selected_namespace {
                    // Start execution on all selected interfaces
                    for interface in &dialog.selected_interfaces {
                        // Check if there's already an execution running on this interface
                        if self.scenario_manager.is_execution_active(
                            &dialog.backend_name,
                            namespace,
                            interface,
                        ) {
                            tracing::warn!(
                                "Scenario execution already active on {}:{}, skipping",
                                namespace,
                                interface
                            );
                            continue;
                        }

                        if let Err(e) = self.scenario_manager.start_execution(
                            &dialog.backend_name,
                            &dialog.scenario_id,
                            namespace,
                            interface,
                            dialog.loop_execution,
                        ) {
                            tracing::error!(
                                "Failed to start scenario execution on {}: {}",
                                interface,
                                e
                            );
                        }
                    }
                    // Hide the dialog after attempting execution
                    self.ui_state.hide_interface_selection_dialog();
                }
                Task::none()
            }
            TcGuiMessage::ScenarioListResponse {
                backend_name,
                response,
            } => {
                use tcgui_shared::scenario::ScenarioResponse;
                match response {
                    ScenarioResponse::Listed {
                        scenarios,
                        load_errors,
                    } => {
                        if !load_errors.is_empty() {
                            for load_error in &load_errors {
                                tracing::warn!(
                                    "Failed to load scenario from {}: {}",
                                    load_error.file_path,
                                    load_error.error.message
                                );
                            }
                        }
                        self.scenario_manager.handle_scenario_list_response(
                            backend_name,
                            scenarios,
                            load_errors,
                        );
                    }
                    ScenarioResponse::Error { error } => {
                        tracing::error!(
                            "Scenario query error from {}: {} ({})",
                            backend_name,
                            error.message,
                            error.category_str()
                        );
                    }
                    _ => {
                        tracing::debug!(
                            "Unhandled scenario response from {}: {:?}",
                            backend_name,
                            response
                        );
                    }
                }
                Task::none()
            }
            // UI operations
            TcGuiMessage::ToggleNamespaceVisibility(backend_name, namespace_name) => {
                handle_toggle_namespace_visibility(&mut self.ui_state, backend_name, namespace_name)
            }
            TcGuiMessage::ShowAllNamespaces => handle_show_all_namespaces(&mut self.ui_state),
            TcGuiMessage::ResetUiState => handle_reset_ui_state(&mut self.ui_state),
            TcGuiMessage::ShowAllBackends => handle_show_all_backends(&mut self.ui_state),
            TcGuiMessage::SwitchTab(tab) => {
                self.ui_state.set_current_tab(tab);

                // Auto-refresh scenarios and templates when switching to Scenarios tab
                if matches!(tab, crate::ui_state::AppTab::Scenarios) {
                    // Request scenarios and templates from all connected backends
                    for backend_name in self.backend_manager.backends().keys() {
                        // Request scenarios
                        if let Err(e) = self.scenario_manager.request_scenarios(backend_name) {
                            tracing::warn!(
                                "Failed to auto-refresh scenarios from {}: {}",
                                backend_name,
                                e
                            );
                        }

                        // Request scenarios
                        if let Err(e) = self.scenario_manager.request_scenarios(backend_name) {
                            tracing::warn!(
                                "Failed to auto-refresh scenarios from {}: {}",
                                backend_name,
                                e
                            );
                        }
                    }
                }

                Task::none()
            }

            // Zoom controls
            TcGuiMessage::ZoomIn => {
                self.ui_state.zoom_in();
                Task::none()
            }
            TcGuiMessage::ZoomOut => {
                self.ui_state.zoom_out();
                Task::none()
            }
            TcGuiMessage::ZoomReset => {
                self.ui_state.zoom_reset();
                Task::none()
            }

            // Maintenance operations
            TcGuiMessage::CleanupStaleBackends => handle_cleanup_stale_backends(
                &mut self.backend_manager,
                &mut self.query_manager,
                &mut self.ui_state,
                &mut self.scenario_manager,
            ),
        }
    }

    /// Renders the application view using the modular view system.
    pub fn view(&self) -> Element<'_, TcGuiMessage> {
        render_main_view(
            &self.backend_manager,
            &self.ui_state,
            &self.scenario_manager,
        )
    }

    /// Sets up subscriptions for Zenoh events and periodic cleanup.
    pub fn subscription(&self) -> Subscription<TcGuiMessage> {
        Subscription::batch(vec![
            // Keyboard and mouse shortcuts for zoom
            event::listen().filter_map(Self::handle_zoom_event),
            // Zenoh events subscription
            self.zenoh_manager.subscription().map(|event| match event {
                ZenohEvent::InterfaceListUpdate(interface_update) => {
                    TcGuiMessage::InterfaceListUpdate(interface_update)
                }
                ZenohEvent::BandwidthUpdate(bandwidth_update) => {
                    TcGuiMessage::BandwidthUpdate(bandwidth_update)
                }
                ZenohEvent::InterfaceStateEvent(state_event) => {
                    TcGuiMessage::InterfaceStateEvent(state_event)
                }
                ZenohEvent::BackendHealthUpdate(health_status) => {
                    TcGuiMessage::BackendHealthUpdate(health_status)
                }
                ZenohEvent::BackendLiveliness {
                    backend_name,
                    alive,
                } => TcGuiMessage::BackendLiveliness {
                    backend_name,
                    alive,
                },
                ZenohEvent::TcConfigUpdate(tc_config_update) => {
                    TcGuiMessage::TcConfigUpdate(tc_config_update)
                }
                ZenohEvent::ScenarioExecutionUpdate(execution_update) => {
                    TcGuiMessage::ScenarioExecutionUpdate(execution_update)
                }
                ZenohEvent::ConnectionStatus(connected) => TcGuiMessage::BackendConnectionStatus {
                    backend_name: "unknown".to_string(),
                    connected,
                },
                ZenohEvent::TcQueryChannelReady(sender) => {
                    TcGuiMessage::SetupTcQueryChannel(sender)
                }
                ZenohEvent::InterfaceQueryChannelReady(sender) => {
                    TcGuiMessage::SetupInterfaceQueryChannel(sender)
                }
                ZenohEvent::ScenarioQueryChannelReady(sender) => {
                    TcGuiMessage::SetupScenarioQueryChannel(sender)
                }
                ZenohEvent::ScenarioExecutionQueryChannelReady(sender) => {
                    TcGuiMessage::SetupScenarioExecutionQueryChannel(sender)
                }
                ZenohEvent::ScenarioResponse {
                    backend_name,
                    response,
                } => TcGuiMessage::ScenarioListResponse {
                    backend_name,
                    response,
                },
            }),
            // Timer for periodic backend cleanup (every 3 seconds)
            iced::time::every(std::time::Duration::from_secs(3))
                .map(|_| TcGuiMessage::CleanupStaleBackends),
        ])
    }
}

impl TcGui {
    /// Handles zoom events from keyboard and mouse.
    /// - Ctrl+Plus or Ctrl+= : Zoom in
    /// - Ctrl+Minus : Zoom out
    /// - Ctrl+0 : Reset zoom to 100%
    /// - Ctrl+Mouse Scroll Up : Zoom in
    /// - Ctrl+Mouse Scroll Down : Zoom out
    fn handle_zoom_event(event: Event) -> Option<TcGuiMessage> {
        match event {
            Event::Keyboard(KeyboardEvent::KeyPressed { key, modifiers, .. }) => {
                Self::handle_keyboard_shortcut(key, modifiers)
            }
            Event::Mouse(MouseEvent::WheelScrolled { delta }) => {
                // Check if Ctrl is pressed using keyboard modifiers
                // Note: We need to track modifier state separately for mouse events
                // For now, we'll use a different approach - listen to modifier key state
                Self::handle_mouse_scroll(delta)
            }
            Event::Keyboard(KeyboardEvent::ModifiersChanged(modifiers)) => {
                // Track modifier state changes
                CTRL_PRESSED.store(modifiers.control(), std::sync::atomic::Ordering::Relaxed);
                None
            }
            _ => None,
        }
    }

    /// Handles keyboard shortcuts for zoom controls.
    fn handle_keyboard_shortcut(key: Key, modifiers: Modifiers) -> Option<TcGuiMessage> {
        if !modifiers.control() {
            return None;
        }

        match key {
            Key::Character(c) => {
                let c_str = c.as_str();
                match c_str {
                    "+" | "=" => Some(TcGuiMessage::ZoomIn),
                    "-" | "_" | ")" => Some(TcGuiMessage::ZoomOut),
                    "0" => Some(TcGuiMessage::ZoomReset),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Handles mouse scroll for zoom (only when Ctrl is pressed).
    fn handle_mouse_scroll(delta: ScrollDelta) -> Option<TcGuiMessage> {
        if !CTRL_PRESSED.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }

        match delta {
            ScrollDelta::Lines { y, .. } => {
                if y > 0.0 {
                    Some(TcGuiMessage::ZoomIn)
                } else if y < 0.0 {
                    Some(TcGuiMessage::ZoomOut)
                } else {
                    None
                }
            }
            ScrollDelta::Pixels { y, .. } => {
                if y > 0.0 {
                    Some(TcGuiMessage::ZoomIn)
                } else if y < 0.0 {
                    Some(TcGuiMessage::ZoomOut)
                } else {
                    None
                }
            }
        }
    }
}

/// Global state to track if Ctrl key is pressed (for mouse scroll zoom).
static CTRL_PRESSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl Default for TcGui {
    fn default() -> Self {
        let (gui, _) = Self::new();
        gui
    }
}
