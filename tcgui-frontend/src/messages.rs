use tcgui_shared::{
    presets::{CustomPreset, PresetList},
    scenario::{
        NetworkScenario, ScenarioExecutionRequest, ScenarioExecutionResponse,
        ScenarioExecutionUpdate, ScenarioRequest, ScenarioResponse,
    },
    BackendHealthStatus, BandwidthUpdate, InterfaceControlRequest, InterfaceControlResponse,
    InterfaceListUpdate, InterfaceStateEvent, TcConfigUpdate, TcRequest, TcResponse,
};
use tokio::sync::mpsc;

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

/// Frontend application messages with new communication architecture
#[derive(Debug, Clone)]
pub enum TcGuiMessage {
    TcInterfaceMessage(String, String, String, TcInterfaceMessage), // (backend_name, namespace_name, interface_name, message)
    // New pub/sub messages
    InterfaceListUpdate(InterfaceListUpdate),
    BandwidthUpdate(BandwidthUpdate),
    InterfaceStateEvent(InterfaceStateEvent),
    BackendHealthUpdate(BackendHealthStatus),
    BackendLiveliness {
        backend_name: String,
        alive: bool,
    },
    TcConfigUpdate(TcConfigUpdate),
    // Preset list update from backend
    PresetListUpdate {
        backend_name: String,
        preset_list: PresetList,
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
    // Query channel setup
    SetupTcQueryChannel(mpsc::UnboundedSender<TcQueryMessage>),
    SetupInterfaceQueryChannel(mpsc::UnboundedSender<InterfaceControlQueryMessage>),
    SetupScenarioQueryChannel(mpsc::UnboundedSender<ScenarioQueryMessage>),
    SetupScenarioExecutionQueryChannel(mpsc::UnboundedSender<ScenarioExecutionQueryMessage>),
    ToggleNamespaceVisibility(String, String), // (backend_name, namespace_name)
    ShowAllNamespaces,                         // Show all hidden namespaces
    ResetUiState,                              // Reset all UI visibility state
    ShowAllBackends,                           // Show all hidden backends
    SwitchTab(crate::ui_state::AppTab),        // Switch application tab
    // Zoom controls
    ZoomIn,
    ZoomOut,
    ZoomReset,
    // Theme toggle
    ToggleTheme,
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

    // Backend cleanup
    CleanupStaleBackends,
}

/// Internal message type for Zenoh communication with new architecture
#[derive(Debug, Clone)]
pub enum ZenohEvent {
    // New pub/sub messages
    InterfaceListUpdate(InterfaceListUpdate),
    BandwidthUpdate(BandwidthUpdate),
    InterfaceStateEvent(InterfaceStateEvent),
    BackendHealthUpdate(BackendHealthStatus),
    // Backend liveliness detection
    BackendLiveliness {
        backend_name: String,
        alive: bool,
    },
    TcConfigUpdate(TcConfigUpdate),
    // Preset list update from backend
    PresetListUpdate {
        backend_name: String,
        preset_list: PresetList,
    },
    // Scenario events
    ScenarioExecutionUpdate(Box<ScenarioExecutionUpdate>),
    // Query channels
    TcQueryChannelReady(mpsc::UnboundedSender<TcQueryMessage>),
    InterfaceQueryChannelReady(mpsc::UnboundedSender<InterfaceControlQueryMessage>),
    ScenarioQueryChannelReady(mpsc::UnboundedSender<ScenarioQueryMessage>),
    ScenarioExecutionQueryChannelReady(mpsc::UnboundedSender<ScenarioExecutionQueryMessage>),
    // Scenario query responses
    ScenarioResponse {
        backend_name: String,
        response: tcgui_shared::scenario::ScenarioResponse,
    },
    // Query responses (commented out until response handling is implemented)
    // TcResponse { backend_name: String, request: TcRequest, response: TcResponse },
    // InterfaceControlResponse { backend_name: String, request: InterfaceControlRequest, response: InterfaceControlResponse },
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
}
