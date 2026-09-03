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
use tracing::info;

use crate::backend_manager::BackendManager;
use crate::bandwidth_history::BandwidthHistoryManager;
use crate::confirm::{ConfirmRequest, ConfirmState};
use crate::message_handlers::*;
use crate::messages::{TcGuiMessage, TcInterfaceMessage, ZenohEvent};
use crate::query_manager::QueryManager;
use crate::scenario_manager::ScenarioManager;
use crate::settings::FrontendSettings;
use crate::shortcuts::{self};
use crate::ui_state::UiStateManager;
use crate::view::{ColorPalette, render_main_view};
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
/// A transient, user-dismissable notification (currently TC operation failures).
#[derive(Debug, Clone)]
pub struct UiNotification {
    /// The message to show.
    pub message: String,
}

/// Maximum notifications retained at once (oldest dropped beyond this).
const MAX_NOTIFICATIONS: usize = 5;

pub struct TcGui {
    /// Backend management and state
    backend_manager: BackendManager,
    /// Transient user notifications (e.g. failed TC operations)
    notifications: Vec<UiNotification>,
    /// Bandwidth history for time-series charts
    bandwidth_history: BandwidthHistoryManager,
    /// Query channel management for TC and interface operations
    query_manager: QueryManager,
    /// Scenario management and operations
    scenario_manager: ScenarioManager,
    /// UI state and visibility management
    ui_state: UiStateManager,
    /// Zenoh session management
    zenoh_manager: ZenohManager,
    /// Pending confirmation for a destructive action, if any
    confirm: ConfirmState,
    /// Whether the keyboard-shortcut help overlay is open
    show_shortcut_help: bool,
}

impl TcGui {
    /// Creates a new TcGui application instance with modular architecture.
    pub fn new() -> (Self, Task<TcGuiMessage>) {
        let settings = FrontendSettings::load();
        info!(
            "Loaded settings: theme={:?}, zoom={}",
            settings.theme_mode, settings.zoom_level
        );

        let app = Self {
            backend_manager: BackendManager::new(),
            notifications: Vec::new(),
            bandwidth_history: BandwidthHistoryManager::default(),
            query_manager: QueryManager::new(),
            scenario_manager: ScenarioManager::new(),
            ui_state: UiStateManager::from_settings(&settings),
            zenoh_manager: ZenohManager::new(ZenohConfig::default()),
            confirm: ConfirmState::default(),
            show_shortcut_help: false,
        };

        (app, Task::none())
    }

    /// Creates a new TcGui application instance with custom Zenoh configuration.
    pub fn new_with_config(zenoh_config: ZenohConfig) -> (Self, Task<TcGuiMessage>) {
        let settings = FrontendSettings::load();
        info!(
            "Loaded settings: theme={:?}, zoom={}",
            settings.theme_mode, settings.zoom_level
        );

        let app = Self {
            backend_manager: BackendManager::new(),
            notifications: Vec::new(),
            bandwidth_history: BandwidthHistoryManager::default(),
            query_manager: QueryManager::new(),
            scenario_manager: ScenarioManager::new(),
            ui_state: UiStateManager::from_settings(&settings),
            zenoh_manager: ZenohManager::new(zenoh_config),
            confirm: ConfirmState::default(),
            show_shortcut_help: false,
        };

        (app, Task::none())
    }

    /// Push a transient notification, dropping the oldest beyond the cap.
    fn notify(&mut self, message: String) {
        self.notifications.push(UiNotification { message });
        if self.notifications.len() > MAX_NOTIFICATIONS {
            let overflow = self.notifications.len() - MAX_NOTIFICATIONS;
            self.notifications.drain(0..overflow);
        }
    }

    /// Saves current UI settings to disk.
    fn save_settings(&self) {
        let settings = self.ui_state.to_settings();
        if let Err(e) = settings.save() {
            tracing::warn!("Failed to save settings: {}", e);
        }
    }

    /// Updates application state in response to messages (Elm architecture update function).
    ///
    /// This simplified update function delegates to specialized message handlers,
    /// making the code much more maintainable and focused.
    pub fn update(&mut self, message: TcGuiMessage) -> Task<TcGuiMessage> {
        match message {
            // Backend-related messages
            TcGuiMessage::InterfaceUpsert {
                backend_name,
                interface,
            } => {
                self.backend_manager
                    .handle_interface_upsert(&backend_name, interface);
                Task::none()
            }
            TcGuiMessage::InterfaceRemoved {
                backend_name,
                namespace,
                interface,
            } => {
                self.backend_manager.handle_interface_removed(
                    &backend_name,
                    &namespace,
                    &interface,
                );
                Task::none()
            }
            TcGuiMessage::BackendHealthUpdate(health_status) => {
                let origin = health_status.host_id.clone();
                self.backend_manager
                    .handle_backend_health_update(&origin, health_status);
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
            TcGuiMessage::TcConfigUpdate(tc_config_update) => {
                handle_tc_config_update(&mut self.backend_manager, tc_config_update)
            }
            TcGuiMessage::TcStatisticsUpdate(tc_stats_update) => {
                handle_tc_statistics_update(&mut self.backend_manager, tc_stats_update)
            }
            TcGuiMessage::TcOperationResult {
                backend_name,
                response,
            } => {
                // Only surface failures — successes are already reflected by the
                // Tc config update that follows.
                if !response.success {
                    tracing::warn!(
                        "TC operation failed on '{}': {}",
                        backend_name,
                        response.message
                    );
                    self.notify(response.message);
                }
                Task::none()
            }
            TcGuiMessage::InterfaceControlResult {
                backend_name,
                response,
            } => {
                if !response.success {
                    tracing::warn!(
                        "Interface control failed on '{}': {}",
                        backend_name,
                        response.message
                    );
                    self.notify(response.message);
                }
                Task::none()
            }
            TcGuiMessage::QueryError {
                backend_name,
                error,
            } => {
                tracing::warn!("Backend '{}' query error: {}", backend_name, error);
                self.notify(error);
                Task::none()
            }
            TcGuiMessage::DismissNotification(index) => {
                if index < self.notifications.len() {
                    self.notifications.remove(index);
                }
                Task::none()
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
                // Record in history for charts
                self.bandwidth_history.record(
                    &bandwidth_update.backend_name,
                    &bandwidth_update.namespace,
                    &bandwidth_update.interface,
                    bandwidth_update.stats.rx_bytes_per_sec,
                    bandwidth_update.stats.tx_bytes_per_sec,
                );
                handle_bandwidth_update(&mut self.backend_manager, bandwidth_update)
            }

            // Interface messages. Destructive ones are intercepted HERE, at the
            // entry message — not at the effect they produce. `ClearAllFeatures`
            // wipes the local checkbox state inside `TcInterface::update` before
            // the `RemoveTc` task is issued, so gating `RemoveTc` would leave a
            // cancelled dialog showing "cleared" while the kernel still has
            // netem on the interface. Same for `InterfaceToggled(false)`.
            TcGuiMessage::TcInterfaceMessage(
                backend_name,
                namespace,
                interface_name,
                tc_message,
            ) => {
                if let Some(request) =
                    Self::confirmation_for(&backend_name, &namespace, &interface_name, &tc_message)
                {
                    self.confirm.request(request);
                    return Task::none();
                }
                handle_tc_interface_message(
                    &mut self.backend_manager,
                    backend_name,
                    namespace,
                    interface_name,
                    tc_message,
                )
            }

            // Already confirmed. Every message that has a confirmation gate is
            // performed directly here, so replaying it cannot re-enter its own
            // gate and loop. Anything ungated falls through to normal handling.
            TcGuiMessage::ConfirmedAction(inner) => match *inner {
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
                TcGuiMessage::ResetUiState => handle_reset_ui_state(&mut self.ui_state),
                other => self.update(other),
            },

            TcGuiMessage::RequestConfirm(request) => {
                self.confirm.request(*request);
                Task::none()
            }
            TcGuiMessage::ConfirmAccepted => self
                .confirm
                .take()
                .map_or_else(Task::none, |m| Task::done(*m)),
            TcGuiMessage::ConfirmCancelled => {
                self.confirm.cancel();
                Task::none()
            }
            TcGuiMessage::ToggleShortcutHelp => {
                self.show_shortcut_help = !self.show_shortcut_help;
                Task::none()
            }
            // Escape closes the topmost overlay, in render order.
            TcGuiMessage::DismissTopOverlay => {
                if self.show_shortcut_help {
                    self.show_shortcut_help = false;
                } else if self.confirm.is_open() {
                    self.confirm.cancel();
                } else {
                    self.ui_state.hide_interface_selection_dialog();
                }
                Task::none()
            }

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
            TcGuiMessage::SetupDiagnosticsQueryChannel(sender) => {
                self.query_manager.setup_diagnostics_query_channel(sender);
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
            TcGuiMessage::ScenarioExecutionRemoved {
                backend_name,
                namespace,
                interface,
            } => {
                self.scenario_manager
                    .remove_execution(&backend_name, &namespace, &interface);
                Task::none()
            }
            // State-plane scenario library upsert / removal
            TcGuiMessage::ScenarioUpsert {
                backend_name,
                scenario,
            } => {
                self.scenario_manager
                    .upsert_scenario(backend_name, *scenario);
                Task::none()
            }
            TcGuiMessage::ScenarioRemoved { backend_name, id } => {
                self.scenario_manager.remove_scenario(&backend_name, &id);
                Task::none()
            }
            // State-plane preset library upsert / removal
            TcGuiMessage::PresetUpsert {
                backend_name,
                preset,
            } => {
                self.backend_manager.upsert_preset(&backend_name, preset);
                Task::none()
            }
            TcGuiMessage::PresetRemoved { backend_name, id } => {
                self.backend_manager.remove_preset(&backend_name, &id);
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
            } if !self.confirm.is_open() => {
                self.confirm.request(ConfirmRequest::stop_scenario(
                    &interface,
                    TcGuiMessage::ConfirmedAction(Box::new(TcGuiMessage::StopScenarioExecution {
                        backend_name,
                        namespace,
                        interface: interface.clone(),
                    })),
                ));
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
            TcGuiMessage::ResetUiState => {
                self.confirm.request(ConfirmRequest::reset_ui_state(
                    TcGuiMessage::ConfirmedAction(Box::new(TcGuiMessage::ResetUiState)),
                ));
                Task::none()
            }
            TcGuiMessage::ShowAllBackends => handle_show_all_backends(&mut self.ui_state),
            TcGuiMessage::SetInterfaceSearch(search) => {
                self.ui_state.set_interface_search(search);
                Task::none()
            }
            TcGuiMessage::SwitchTab(tab) => {
                self.ui_state.set_current_tab(tab);
                self.save_settings();

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

            // Zoom controls (persistent)
            TcGuiMessage::ZoomIn => {
                self.ui_state.zoom_in();
                self.save_settings();
                Task::none()
            }
            TcGuiMessage::ZoomOut => {
                self.ui_state.zoom_out();
                self.save_settings();
                Task::none()
            }
            TcGuiMessage::ZoomReset => {
                self.ui_state.zoom_reset();
                self.save_settings();
                Task::none()
            }
            TcGuiMessage::ToggleTheme => {
                self.ui_state.toggle_theme();
                self.save_settings();
                Task::none()
            }
            TcGuiMessage::ToggleInterfaceViewMode => {
                self.ui_state.toggle_interface_view_mode();
                Task::none()
            }
            // Namespace filters (persistent)
            TcGuiMessage::ToggleHostFilter => {
                self.ui_state.toggle_host_filter();
                self.save_settings();
                Task::none()
            }
            TcGuiMessage::ToggleNamespaceTypeFilter => {
                self.ui_state.toggle_namespace_filter();
                self.save_settings();
                Task::none()
            }
            TcGuiMessage::ToggleContainerFilter => {
                self.ui_state.toggle_container_filter();
                self.save_settings();
                Task::none()
            }

            // Diagnostics operations
            TcGuiMessage::RunDiagnostics {
                backend_name,
                namespace,
                interface,
            } => handle_run_diagnostics(
                &self.query_manager,
                &mut self.backend_manager,
                backend_name,
                namespace,
                interface,
            ),
            TcGuiMessage::DiagnosticsResult {
                backend_name,
                namespace,
                interface,
                response,
            } => handle_diagnostics_result(
                &mut self.backend_manager,
                backend_name,
                namespace,
                interface,
                response,
            ),

            // Maintenance operations
            TcGuiMessage::CleanupStaleBackends => handle_cleanup_stale_backends(
                &mut self.backend_manager,
                &mut self.bandwidth_history,
                &mut self.query_manager,
                &mut self.ui_state,
                &mut self.scenario_manager,
            ),
        }
    }

    /// Renders the application view using the modular view system.
    pub fn view(&self) -> Element<'_, TcGuiMessage> {
        let main = render_main_view(
            &self.backend_manager,
            &self.bandwidth_history,
            &self.ui_state,
            &self.scenario_manager,
        );

        if self.notifications.is_empty() {
            return self.with_overlays(main);
        }

        // Stack dismissable error banners above the main view.
        use iced::widget::{Column, button, container, row, text};
        use iced::{Color, Length};

        let mut banners = Column::new().spacing(4).padding(6).width(Length::Fill);
        for (i, n) in self.notifications.iter().enumerate() {
            let banner = container(
                row![
                    text(format!("⚠ {}", n.message))
                        .size(13)
                        .width(Length::Fill)
                        .style(|_| text::Style {
                            color: Some(Color::WHITE),
                        }),
                    button(text("✕").size(13)).on_press(TcGuiMessage::DismissNotification(i)),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            )
            .padding(8)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.55, 0.12, 0.12).into()),
                text_color: Some(Color::WHITE),
                ..Default::default()
            });
            banners = banners.push(banner);
        }

        let stacked: Element<'_, TcGuiMessage> = Column::new().push(banners).push(main).into();
        self.with_overlays(stacked)
    }

    /// Stack the modal overlays above the page, in Escape precedence order:
    /// help sits above the confirmation, which sits above everything else.
    fn with_overlays<'a>(&'a self, base: Element<'a, TcGuiMessage>) -> Element<'a, TcGuiMessage> {
        use iced::widget::stack;

        let colors = ColorPalette::from_theme(self.ui_state.theme());
        let zoom = self.ui_state.zoom_level();
        let mut layers = vec![base];

        if let Some(pending) = self.confirm.pending() {
            layers.push(crate::confirm::render_confirm(
                pending,
                colors.clone(),
                zoom,
            ));
        }
        if self.show_shortcut_help {
            layers.push(Self::render_shortcut_help(colors, zoom));
        }

        if layers.len() == 1 {
            layers.pop().expect("one layer")
        } else {
            stack(layers).into()
        }
    }

    /// The shortcut help overlay, rendered from the same `SHORTCUTS` table the
    /// dispatcher matches on, so the two cannot drift apart.
    fn render_shortcut_help<'a>(colors: ColorPalette, zoom: f32) -> Element<'a, TcGuiMessage> {
        use crate::view::{scaled, scaled_padding, scaled_spacing};
        use iced::widget::{button, column, container, row, text};
        use iced::{Color, Length};

        let mut rows =
            column![
                text("Keyboard shortcuts")
                    .size(scaled(18, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary),
                    })
            ]
            .spacing(scaled_spacing(12, zoom));

        for s in shortcuts::SHORTCUTS {
            rows = rows.push(
                row![
                    text(s.combo)
                        .size(scaled(13, zoom))
                        .width(Length::Fixed(scaled(110, zoom)))
                        .style(move |_| text::Style {
                            color: Some(colors.primary_blue),
                        }),
                    text(s.description)
                        .size(scaled(13, zoom))
                        .width(Length::Fill)
                        .style(move |_| text::Style {
                            color: Some(colors.text_secondary),
                        }),
                ]
                .spacing(scaled_spacing(12, zoom)),
            );
        }

        rows = rows.push(
            button(text("Close").size(scaled(13, zoom)))
                .on_press(TcGuiMessage::ToggleShortcutHelp)
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.background_card)),
                    text_color: colors.text_primary,
                    border: iced::Border {
                        radius: 6.0.into(),
                        width: 1.0,
                        color: colors.text_secondary,
                    },
                    ..button::Style::default()
                }),
        );

        let card = container(rows)
            .padding(scaled_padding(24, zoom))
            .max_width(520)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(colors.background_card)),
                border: iced::Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: colors.text_secondary,
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                    offset: iced::Vector::new(0.0, 8.0),
                    blur_radius: 16.0,
                },
                ..container::Style::default()
            });

        crate::confirm::modal_backdrop(card.into(), zoom)
    }

    /// Sets up subscriptions for Zenoh events and periodic cleanup.
    pub fn subscription(&self) -> Subscription<TcGuiMessage> {
        Subscription::batch(vec![
            // Keyboard and mouse shortcuts for zoom
            event::listen().filter_map(Self::handle_zoom_event),
            // Zenoh events subscription
            self.zenoh_manager.subscription().map(|event| match event {
                ZenohEvent::InterfaceUpsert {
                    backend_name,
                    interface,
                } => TcGuiMessage::InterfaceUpsert {
                    backend_name,
                    interface,
                },
                ZenohEvent::InterfaceRemoved {
                    backend_name,
                    namespace,
                    interface,
                } => TcGuiMessage::InterfaceRemoved {
                    backend_name,
                    namespace,
                    interface,
                },
                ZenohEvent::BandwidthUpdate(bandwidth_update) => {
                    TcGuiMessage::BandwidthUpdate(bandwidth_update)
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
                ZenohEvent::TcStatisticsUpdate(tc_stats_update) => {
                    TcGuiMessage::TcStatisticsUpdate(tc_stats_update)
                }
                ZenohEvent::ScenarioExecutionUpdate(execution_update) => {
                    TcGuiMessage::ScenarioExecutionUpdate(execution_update)
                }
                ZenohEvent::ScenarioExecutionRemoved {
                    backend_name,
                    namespace,
                    interface,
                } => TcGuiMessage::ScenarioExecutionRemoved {
                    backend_name,
                    namespace,
                    interface,
                },
                ZenohEvent::ScenarioUpsert {
                    backend_name,
                    scenario,
                } => TcGuiMessage::ScenarioUpsert {
                    backend_name,
                    scenario,
                },
                ZenohEvent::ScenarioRemoved { backend_name, id } => {
                    TcGuiMessage::ScenarioRemoved { backend_name, id }
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
                ZenohEvent::DiagnosticsQueryChannelReady(sender) => {
                    TcGuiMessage::SetupDiagnosticsQueryChannel(sender)
                }
                ZenohEvent::ScenarioResponse {
                    backend_name,
                    response,
                } => TcGuiMessage::ScenarioListResponse {
                    backend_name,
                    response,
                },
                ZenohEvent::DiagnosticsResponse {
                    backend_name,
                    namespace,
                    interface,
                    response,
                } => TcGuiMessage::DiagnosticsResult {
                    backend_name,
                    namespace,
                    interface,
                    response,
                },
                ZenohEvent::TcOperationResult {
                    backend_name,
                    response,
                } => TcGuiMessage::TcOperationResult {
                    backend_name,
                    response,
                },
                ZenohEvent::InterfaceControlResult {
                    backend_name,
                    response,
                } => TcGuiMessage::InterfaceControlResult {
                    backend_name,
                    response,
                },
                ZenohEvent::QueryError {
                    backend_name,
                    error,
                } => TcGuiMessage::QueryError {
                    backend_name,
                    error,
                },
                ZenohEvent::PresetUpsert {
                    backend_name,
                    preset,
                } => TcGuiMessage::PresetUpsert {
                    backend_name,
                    preset,
                },
                ZenohEvent::PresetRemoved { backend_name, id } => {
                    TcGuiMessage::PresetRemoved { backend_name, id }
                }
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

    /// Resolves a key press against the shortcut table in `shortcuts.rs`, which
    /// is the same table the help overlay renders — so a binding cannot exist
    /// undocumented, and the documentation cannot drift from the binding.
    fn handle_keyboard_shortcut(key: Key, modifiers: Modifiers) -> Option<TcGuiMessage> {
        shortcuts::dispatch(&key, modifiers).map(shortcuts::action_message)
    }

    /// The confirmation a destructive interface action needs, if it needs one.
    ///
    /// Returning `None` means the message proceeds untouched, which is the case
    /// for every non-destructive interface message.
    fn confirmation_for(
        backend_name: &str,
        namespace: &str,
        interface_name: &str,
        message: &TcInterfaceMessage,
    ) -> Option<ConfirmRequest> {
        let replay = || {
            TcGuiMessage::ConfirmedAction(Box::new(TcGuiMessage::TcInterfaceMessage(
                backend_name.to_string(),
                namespace.to_string(),
                interface_name.to_string(),
                message.clone(),
            )))
        };
        match message {
            // The only emitter of RemoveTc in the frontend, so gating this one
            // covers the whole clear path.
            TcInterfaceMessage::ClearAllFeatures => {
                Some(ConfirmRequest::clear_all_features(interface_name, replay()))
            }
            // Bringing an interface down can sever the operator's own access.
            TcInterfaceMessage::InterfaceToggled(false) => {
                Some(ConfirmRequest::disable_interface(interface_name, replay()))
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
