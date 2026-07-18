use tcgui_shared::{
    BackendHealthStatus, BandwidthUpdate, DiagnosticsRequest, DiagnosticsResponse,
    InterfaceControlRequest, InterfaceControlResponse, NetworkInterface, TcConfigUpdate, TcRequest,
    TcResponse, TcStatisticsUpdate,
    presets::CustomPreset,
    scenario::{
        NetworkScenario, ScenarioExecutionRequest, ScenarioExecutionResponse,
        ScenarioExecutionUpdate, ScenarioRequest, ScenarioResponse,
    },
};
use tokio::sync::mpsc;

// NOTE (keyspace-v2): every `backend_name` field on the query-message structs
// below now carries the **host origin** (`h-<12hex>`), not the operator-chosen
// display label. The zenoh_manager parses it with `RemoteOrigin::parse` to build
// the concrete `@rpc` call key; the display label lives only in `BackendGroup.name`.

/// Message for scenario management query operations
#[derive(Debug, Clone)]
pub struct ScenarioQueryMessage {
    pub backend_name: String,
    pub request: ScenarioRequest,
    /// Response channel (unused - responses handled via ZenohEvent::ScenarioResponse)
    #[allow(dead_code)]
    pub response_sender: Option<mpsc::UnboundedSender<(String, ScenarioResponse)>>,
}

/// Message for scenario execution query operations
#[derive(Debug, Clone)]
pub struct ScenarioExecutionQueryMessage {
    pub backend_name: String,
    pub request: ScenarioExecutionRequest,
    pub response_sender: Option<mpsc::UnboundedSender<(String, ScenarioExecutionResponse)>>,
}

/// Message for TC query operations
#[derive(Debug, Clone)]
pub struct TcQueryMessage {
    pub backend_name: String,
    pub request: TcRequest,
    pub response_sender: Option<mpsc::UnboundedSender<(String, TcResponse)>>,
}

/// Message for interface control query operations
#[derive(Debug, Clone)]
pub struct InterfaceControlQueryMessage {
    pub backend_name: String,
    pub request: InterfaceControlRequest,
    pub response_sender: Option<mpsc::UnboundedSender<(String, InterfaceControlResponse)>>,
}

/// Message for diagnostics query operations
#[derive(Debug, Clone)]
pub struct DiagnosticsQueryMessage {
    pub backend_name: String,
    pub request: DiagnosticsRequest,
    pub response_sender: Option<mpsc::UnboundedSender<(String, DiagnosticsResponse)>>,
}

/// Frontend application messages with new communication architecture
#[derive(Debug, Clone)]
pub enum TcGuiMessage {
    TcInterfaceMessage(String, String, String, TcInterfaceMessage), // (origin, namespace_name, interface_name, message)
    // State-plane per-interface upsert (Put on `state/tc/interface/{ns}/{if}`).
    InterfaceUpsert {
        backend_name: String,
        interface: NetworkInterface,
    },
    // State-plane per-interface removal (Delete tombstone on the same key).
    InterfaceRemoved {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    BandwidthUpdate(BandwidthUpdate),
    BackendHealthUpdate(BackendHealthStatus),
    BackendLiveliness {
        backend_name: String,
        alive: bool,
    },
    TcConfigUpdate(TcConfigUpdate),
    TcStatisticsUpdate(TcStatisticsUpdate),
    // State-plane per-preset upsert / removal (state/tc/preset/{id}).
    PresetUpsert {
        backend_name: String,
        preset: CustomPreset,
    },
    PresetRemoved {
        backend_name: String,
        id: String,
    },
    // State-plane per-scenario upsert / removal (state/tc/scenario/{id}).
    ScenarioUpsert {
        backend_name: String,
        scenario: Box<NetworkScenario>,
    },
    ScenarioRemoved {
        backend_name: String,
        id: String,
    },
    // Query/Reply responses (commented out until response handling is implemented)
    // TcResponse { backend_name: String, request: TcRequest, response: TcResponse },
    // InterfaceControlResponse { backend_name: String, request: InterfaceControlRequest, response: InterfaceControlResponse },
    // Internal messages
    BackendConnectionStatus {
        backend_name: String,
        connected: bool,
    },
    // Scenario events
    ScenarioExecutionUpdate(Box<ScenarioExecutionUpdate>),
    // State-plane execution removal (Delete tombstone on state/tc/execution/{ns}/{if}).
    ScenarioExecutionRemoved {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    // Query channel setup
    SetupTcQueryChannel(mpsc::UnboundedSender<TcQueryMessage>),
    SetupInterfaceQueryChannel(mpsc::UnboundedSender<InterfaceControlQueryMessage>),
    SetupScenarioQueryChannel(mpsc::UnboundedSender<ScenarioQueryMessage>),
    SetupScenarioExecutionQueryChannel(mpsc::UnboundedSender<ScenarioExecutionQueryMessage>),
    SetupDiagnosticsQueryChannel(mpsc::UnboundedSender<DiagnosticsQueryMessage>),
    ToggleNamespaceVisibility(String, String), // (backend_name, namespace_name)
    ShowAllNamespaces,                         // Show all hidden namespaces
    ResetUiState,                              // Reset all UI visibility state
    ShowAllBackends,                           // Show all hidden backends
    SwitchTab(crate::ui_state::AppTab),        // Switch application tab
    SetInterfaceSearch(String),                // Update the interface-name search filter
    // Zoom controls
    ZoomIn,
    ZoomOut,
    ZoomReset,
    // Theme toggle
    ToggleTheme,
    // View mode toggle
    ToggleInterfaceViewMode,
    // Namespace type filter toggles
    ToggleHostFilter,
    ToggleNamespaceTypeFilter,
    ToggleContainerFilter,
    // TC and Interface operations (trigger queries)
    ApplyTc {
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
    },
    RemoveTc {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    EnableInterface {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    DisableInterface {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    // Scenario operations
    ListScenarios {
        backend_name: String,
    },
    StopScenarioExecution {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    PauseScenarioExecution {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    ResumeScenarioExecution {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    ToggleExecutionTimeline {
        backend_name: String,
        namespace: String,
        interface: String,
    },

    ShowScenarioDetails {
        scenario: NetworkScenario,
    },
    HideScenarioDetails,
    // Scenario list filter/sort messages
    ScenarioSearchFilterChanged(String),
    ScenarioSortOptionChanged(crate::scenario_manager::ScenarioSortOption),
    // Interface selection dialog messages
    ShowInterfaceSelectionDialog {
        backend_name: String,
        scenario_id: String,
    },
    HideInterfaceSelectionDialog,
    SelectExecutionNamespace(String),
    ToggleExecutionInterface(String),
    ToggleLoopExecution,
    ConfirmScenarioExecution,
    // Scenario response messages
    ScenarioListResponse {
        backend_name: String,
        response: tcgui_shared::scenario::ScenarioResponse,
    },

    // Diagnostics operations
    RunDiagnostics {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    DiagnosticsResult {
        backend_name: String,
        namespace: String,
        interface: String,
        response: DiagnosticsResponse,
    },

    /// Result of a TC apply/remove operation, used to surface failures.
    TcOperationResult {
        backend_name: String,
        response: TcResponse,
    },
    /// Result of an interface enable/disable operation, to surface failures.
    InterfaceControlResult {
        backend_name: String,
        response: InterfaceControlResponse,
    },
    /// A backend query failed on Zenoh's reply-error channel (RFC 05 §3).
    QueryError {
        backend_name: String,
        error: String,
    },
    /// Dismiss the notification at the given index.
    DismissNotification(usize),

    // Backend cleanup
    CleanupStaleBackends,
}

/// Internal message type for Zenoh communication with new architecture
#[derive(Debug, Clone)]
pub enum ZenohEvent {
    // State-plane per-interface upsert / removal.
    InterfaceUpsert {
        backend_name: String,
        interface: NetworkInterface,
    },
    InterfaceRemoved {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    BandwidthUpdate(BandwidthUpdate),
    BackendHealthUpdate(BackendHealthStatus),
    // Backend liveliness detection
    BackendLiveliness {
        backend_name: String,
        alive: bool,
    },
    TcConfigUpdate(TcConfigUpdate),
    TcStatisticsUpdate(TcStatisticsUpdate),
    // State-plane per-preset upsert / removal.
    PresetUpsert {
        backend_name: String,
        preset: CustomPreset,
    },
    PresetRemoved {
        backend_name: String,
        id: String,
    },
    // State-plane per-scenario upsert / removal.
    ScenarioUpsert {
        backend_name: String,
        scenario: Box<NetworkScenario>,
    },
    ScenarioRemoved {
        backend_name: String,
        id: String,
    },
    // Scenario events
    ScenarioExecutionUpdate(Box<ScenarioExecutionUpdate>),
    // State-plane execution removal (Delete tombstone).
    ScenarioExecutionRemoved {
        backend_name: String,
        namespace: String,
        interface: String,
    },
    // Query channels
    TcQueryChannelReady(mpsc::UnboundedSender<TcQueryMessage>),
    InterfaceQueryChannelReady(mpsc::UnboundedSender<InterfaceControlQueryMessage>),
    ScenarioQueryChannelReady(mpsc::UnboundedSender<ScenarioQueryMessage>),
    ScenarioExecutionQueryChannelReady(mpsc::UnboundedSender<ScenarioExecutionQueryMessage>),
    DiagnosticsQueryChannelReady(mpsc::UnboundedSender<DiagnosticsQueryMessage>),
    // Scenario query responses
    ScenarioResponse {
        backend_name: String,
        response: tcgui_shared::scenario::ScenarioResponse,
    },
    // Diagnostics query responses
    DiagnosticsResponse {
        backend_name: String,
        namespace: String,
        interface: String,
        response: tcgui_shared::DiagnosticsResponse,
    },
    /// Result of a TC apply/remove query (used to surface failures in the UI).
    TcOperationResult {
        backend_name: String,
        response: TcResponse,
    },
    /// Result of an interface enable/disable query (to surface failures).
    InterfaceControlResult {
        backend_name: String,
        response: InterfaceControlResponse,
    },
    /// A query failed on Zenoh's reply-error channel (RFC keyspace-v2 05 §3).
    /// `error` is the backend's namespaced `error/...: message` string.
    QueryError {
        backend_name: String,
        error: String,
    },
    // Connection status
    ConnectionStatus(bool),
}

/// Individual interface component messages
#[derive(Debug, Clone)]
pub enum TcInterfaceMessage {
    // Loss control
    LossChanged(f32),
    LossToggled(bool),
    CorrelationChanged(f32),

    // Interface state
    InterfaceToggled(bool),

    // Delay control
    DelayToggled(bool),
    DelayChanged(f32),
    DelayJitterChanged(f32),
    DelayCorrelationChanged(f32),

    // Duplicate control
    DuplicateToggled(()),
    DuplicatePercentageChanged(f32),
    DuplicateCorrelationChanged(f32),

    // Reorder control
    ReorderToggled(()),
    ReorderPercentageChanged(f32),
    ReorderCorrelationChanged(f32),
    ReorderGapChanged(u32),

    // Corrupt control
    CorruptToggled(()),
    CorruptPercentageChanged(f32),
    CorruptCorrelationChanged(f32),

    // Rate limit control
    RateLimitToggled(()),
    RateLimitChanged(u32),

    // Preset control
    PresetSelected(CustomPreset),
    TogglePresetDropdown,
    ClearAllFeatures,

    // Chart control
    ToggleChart,

    // Diagnostics control
    StartDiagnostics,
    DiagnosticsComplete(DiagnosticsResponse),
    DismissDiagnostics,
}
