use tcgui_shared::{
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
    #[allow(dead_code)] // Not used since we handle responses via ZenohEvent
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
    #[allow(dead_code)] // TODO: Wire up scenario UI
    GetExecutionStatus {
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
    SelectExecutionInterface(String),
    ConfirmScenarioExecution,
    // Scenario response messages
    ScenarioListResponse {
        backend_name: String,
        response: tcgui_shared::scenario::ScenarioResponse,
    },
    #[allow(dead_code)] // TODO: Wire up scenario execution response handling
    ScenarioExecutionResponse {
        backend_name: String,
        response: tcgui_shared::scenario::ScenarioExecutionResponse,
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
    // Core messages actually constructed in code
    #[allow(dead_code)]
    LossChanged(f32), // Only used in tests but handled in update method
    LossToggled(bool),      // Used in UI
    InterfaceToggled(bool), // Used in UI
    DelayToggled(bool),     // Used in UI

    // Feature toggle messages (used in UI)
    DuplicateToggled(()), // Used in UI
    ReorderToggled(()),   // Used in UI
    CorruptToggled(()),   // Used in UI
    RateLimitToggled(()), // Used in UI

    // Parameter control messages for sliders (now actively used)
    CorrelationChanged(f32),      // Loss correlation slider
    DelayChanged(f32),            // Delay base slider
    DelayJitterChanged(f32),      // Delay jitter slider
    DelayCorrelationChanged(f32), // Delay correlation slider

    // Duplicate control messages
    DuplicatePercentageChanged(f32),  // Duplicate percentage slider
    DuplicateCorrelationChanged(f32), // Duplicate correlation slider

    // Reorder control messages
    ReorderPercentageChanged(f32),  // Reorder percentage slider
    ReorderCorrelationChanged(f32), // Reorder correlation slider
    ReorderGapChanged(u32),         // Reorder gap slider

    // Corrupt control messages
    CorruptPercentageChanged(f32),  // Corrupt percentage slider
    CorruptCorrelationChanged(f32), // Corrupt correlation slider

    // Rate limit control messages
    RateLimitChanged(u32), // Rate limit kbps slider

    // Preset messages (kept for future preset UI)
    #[allow(dead_code)]
    PresetSelected(tcgui_shared::presets::NetworkPreset), // For future preset selector
    #[allow(dead_code)]
    ApplyPreset, // For future preset apply button
    #[allow(dead_code)]
    TogglePresets, // For future preset toggle
}
