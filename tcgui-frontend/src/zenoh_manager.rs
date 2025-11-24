use iced::task::{sipper, Never, Sipper};
use iced::Subscription;
use std::sync::Arc;
use tcgui_shared::{
    scenario::ScenarioExecutionUpdate, topics, BackendHealthStatus, BandwidthUpdate,
    InterfaceControlResponse, InterfaceListUpdate, InterfaceStateEvent, TcConfigUpdate, TcResponse,
    ZenohConfig,
};
use tokio::sync::mpsc;
use tracing::{error, info};
use zenoh_ext::{AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig};

use crate::messages::{
    InterfaceControlQueryMessage, ScenarioExecutionQueryMessage, ScenarioQueryMessage,
    TcQueryMessage, ZenohEvent,
};

/// Common zenoh sample processing logic extracted to reduce code duplication
///
/// This macro eliminates repetitive error handling patterns by:
/// 1. Extracting backend name from topic
/// 2. Converting payload bytes to UTF-8 string
/// 3. Deserializing JSON to the target type
/// 4. Logging success/failure with contextual information
macro_rules! process_zenoh_sample {
    ($sample:expr, $message_type:ty, $type_name:expr, $log_block:expr, $event_constructor:expr) => {{
        let topic = $sample.key_expr().as_str();

        // Extract backend name from topic
        if let Some(backend_name) = topics::extract_backend_name(&$sample.key_expr()) {
            // Convert payload to bytes
            let payload_bytes = $sample.payload().to_bytes();

            // Convert bytes to UTF-8 string
            match std::str::from_utf8(&payload_bytes) {
                Ok(payload_str) => {
                    // Deserialize JSON to target type
                    match serde_json::from_str::<$message_type>(payload_str) {
                        Ok(message) => {
                            // Log successful processing
                            $log_block(&backend_name, &message);
                            // Create and send the event
                            Some($event_constructor(message))
                        }
                        Err(e) => {
                            error!("Failed to deserialize {}: {}", $type_name, e);
                            None
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to extract {} payload: {}", $type_name, e);
                    None
                }
            }
        } else {
            error!(
                "Could not extract backend name from {} topic: {}",
                $type_name, topic
            );
            None
        }
    }};
}

/// Handles zenoh sample processing for interface list updates
fn handle_interface_update_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    process_zenoh_sample!(
        sample,
        InterfaceListUpdate,
        "interface update",
        |backend_name: &str, update: &InterfaceListUpdate| {
            info!(
                "Frontend received interface update from '{}': {} namespaces",
                backend_name,
                update.namespaces.len()
            );
        },
        ZenohEvent::InterfaceListUpdate
    )
}

/// Handles zenoh sample processing for bandwidth updates
fn handle_bandwidth_update_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    process_zenoh_sample!(
        sample,
        BandwidthUpdate,
        "bandwidth update",
        |backend_name: &str, update: &BandwidthUpdate| {
            info!("Frontend received bandwidth update from '{}' for {}/{}: RX {:.2} B/s, TX {:.2} B/s",
                backend_name, update.namespace, update.interface,
                update.stats.rx_bytes_per_sec, update.stats.tx_bytes_per_sec);
        },
        ZenohEvent::BandwidthUpdate
    )
}

/// Handles zenoh sample processing for interface state events
fn handle_interface_event_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    process_zenoh_sample!(
        sample,
        InterfaceStateEvent,
        "interface event",
        |backend_name: &str, event: &InterfaceStateEvent| {
            info!(
                "Frontend received interface event from '{}': {:?} on {}",
                backend_name, event.event_type, event.interface.name
            );
        },
        ZenohEvent::InterfaceStateEvent
    )
}

/// Handles zenoh sample processing for backend health status
fn handle_health_status_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    process_zenoh_sample!(
        sample,
        BackendHealthStatus,
        "health status",
        |backend_name: &str, health: &BackendHealthStatus| {
            info!(
                "Frontend received health status from '{}': {}",
                backend_name, health.status
            );
        },
        ZenohEvent::BackendHealthUpdate
    )
}

/// Handles zenoh sample processing for TC configuration updates
fn handle_tc_config_update_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    process_zenoh_sample!(
        sample,
        TcConfigUpdate,
        "TC config update",
        |backend_name: &str, update: &TcConfigUpdate| {
            info!(
                "Frontend received TC config update from '{}' for {}/{}: has_tc={}",
                backend_name, update.namespace, update.interface, update.has_tc
            );
        },
        ZenohEvent::TcConfigUpdate
    )
}

/// Handles zenoh sample processing for scenario execution updates
fn handle_scenario_execution_update_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    let topic = sample.key_expr().as_str();

    // Extract backend name from topic
    if let Some(backend_name) = topics::extract_backend_name(sample.key_expr()) {
        // Convert payload to bytes
        let payload_bytes = sample.payload().to_bytes();

        // Convert bytes to UTF-8 string
        match std::str::from_utf8(&payload_bytes) {
            Ok(payload_str) => {
                // Deserialize JSON to target type
                match serde_json::from_str::<ScenarioExecutionUpdate>(payload_str) {
                    Ok(update) => {
                        // Log successful processing
                        info!(
                            "Frontend received scenario execution update from '{}' for {}/{}: {:?}",
                            backend_name,
                            update.namespace,
                            update.interface,
                            update.execution.state
                        );
                        // Create and send the event with Box::new
                        Some(ZenohEvent::ScenarioExecutionUpdate(Box::new(update)))
                    }
                    Err(e) => {
                        error!("Failed to deserialize scenario execution update: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                error!("Failed to extract scenario execution update payload: {}", e);
                None
            }
        }
    } else {
        error!(
            "Could not extract backend name from scenario execution update topic: {}",
            topic
        );
        None
    }
}

/// Handles zenoh liveliness samples for backend presence detection
fn handle_liveliness_sample(sample: zenoh::sample::Sample) -> Option<ZenohEvent> {
    let topic = sample.key_expr().as_str();

    // Extract backend name from topic (tcgui/{backend_name}/health)
    if let Some(backend_name) = tcgui_shared::topics::extract_backend_name(sample.key_expr()) {
        let alive = sample.kind() == zenoh::sample::SampleKind::Put;

        info!(
            "Frontend received liveliness update for backend '{}': {}",
            backend_name,
            if alive { "alive" } else { "disconnected" }
        );

        Some(ZenohEvent::BackendLiveliness {
            backend_name: backend_name.to_string(),
            alive,
        })
    } else {
        error!(
            "Could not extract backend name from liveliness topic: {}",
            topic
        );
        None
    }
}

/// Zenoh session manager with configuration dependency injection
pub struct ZenohManager {
    config: Arc<ZenohConfig>,
}

impl ZenohManager {
    /// Create a new zenoh manager with the given configuration
    pub fn new(config: ZenohConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }

    /// Create an Iced subscription for zenoh events with dependency-injected configuration
    ///
    /// Creates a subscription that uses the configured Zenoh settings.
    /// This properly respects the configuration passed during ZenohManager construction.
    pub fn subscription(&self) -> Subscription<ZenohEvent> {
        let config = Arc::clone(&self.config);
        Subscription::run_with((*config).clone(), |config| {
            zenoh_manager_with_arc(Arc::new(config.clone()))
        })
    }

    /// Create the zenoh sipper with the configured settings
    pub fn create_sipper(&self) -> impl Sipper<Never, ZenohEvent> {
        let config = Arc::clone(&self.config);
        sipper(async move |mut output| {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

                let zenoh_config = match config.to_zenoh_config() {
                    Ok(config) => config,
                    Err(e) => {
                        error!("Failed to create zenoh config: {}", e);
                        continue;
                    }
                };

                match zenoh::open(zenoh_config).await {
                    Ok(session) => {
                        info!(
                            "Frontend Zenoh session opened with mode: {:?}, endpoints: {:?}",
                            config.mode, config.endpoints
                        );
                        let _ = output.send(ZenohEvent::ConnectionStatus(true)).await;

                        // Create separate channels for TC queries, interface control queries, and scenario queries
                        let (tc_query_sender, mut tc_query_receiver) =
                            mpsc::unbounded_channel::<TcQueryMessage>();
                        let (interface_control_sender, mut interface_control_receiver) =
                            mpsc::unbounded_channel::<InterfaceControlQueryMessage>();
                        let (scenario_query_sender, mut scenario_query_receiver) =
                            mpsc::unbounded_channel::<ScenarioQueryMessage>();
                        let (
                            scenario_execution_query_sender,
                            mut scenario_execution_query_receiver,
                        ) = mpsc::unbounded_channel::<ScenarioExecutionQueryMessage>();

                        let _ = output
                            .send(ZenohEvent::TcQueryChannelReady(tc_query_sender))
                            .await;
                        let _ = output
                            .send(ZenohEvent::InterfaceQueryChannelReady(
                                interface_control_sender,
                            ))
                            .await;
                        let _ = output
                            .send(ZenohEvent::ScenarioQueryChannelReady(scenario_query_sender))
                            .await;
                        let _ = output
                            .send(ZenohEvent::ScenarioExecutionQueryChannelReady(
                                scenario_execution_query_sender,
                            ))
                            .await;

                        // Declare subscriber for interface list updates with history
                        let interface_topic = "tcgui/*/interfaces/list";
                        let interface_subscriber = match session
                            .declare_subscriber(interface_topic)
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(RecoveryConfig::default().heartbeat())
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to all interface updates with history: {}",
                                    interface_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to interface updates: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Declare subscriber for bandwidth updates with history
                        let bandwidth_topic = "tcgui/*/bandwidth/*/**";
                        let bandwidth_subscriber = match session
                            .declare_subscriber(bandwidth_topic)
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(RecoveryConfig::default().heartbeat())
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to all bandwidth updates with history: {}",
                                    bandwidth_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to bandwidth updates: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Declare subscriber for interface state events with history
                        let interface_events_topic = "tcgui/*/interfaces/events";
                        let interface_event_subscriber = match session
                            .declare_subscriber(interface_events_topic)
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(RecoveryConfig::default().heartbeat())
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to all interface events with history: {}",
                                    interface_events_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to interface events: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Declare subscriber for backend health status with history
                        let health_topic = "tcgui/*/health";
                        let health_subscriber = match session
                            .declare_subscriber(health_topic)
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(RecoveryConfig::default().heartbeat())
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to all backend health with history: {}",
                                    health_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to backend health: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Declare subscriber for TC configuration updates with history
                        let tc_config_topic = "tcgui/*/tc/*/*";
                        let tc_config_subscriber = match session
                            .declare_subscriber(tc_config_topic)
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(RecoveryConfig::default().heartbeat())
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to all TC config updates with history: {}",
                                    tc_config_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to TC config updates: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Declare subscriber for scenario execution updates with history
                        let scenario_execution_topic = "tcgui/*/scenario/execution/*/*";
                        let scenario_execution_subscriber = match session
                            .declare_subscriber(scenario_execution_topic)
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(RecoveryConfig::default().heartbeat())
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to all scenario execution updates with history: {}",
                                    scenario_execution_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to scenario execution updates: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Declare liveliness subscriber to detect backend presence
                        let liveliness_topic = "tcgui/*/health";
                        let liveliness_subscriber = match session
                            .liveliness()
                            .declare_subscriber(liveliness_topic)
                            .history(true)
                            .await
                        {
                            Ok(subscriber) => {
                                info!(
                                    "Subscribed to backend liveliness with topic: {}",
                                    liveliness_topic
                                );
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to backend liveliness: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Main communication loop
                        loop {
                            tokio::select! {
                                // Handle interface list updates
                                sample_result = interface_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_interface_update_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving interface update: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle bandwidth updates
                                sample_result = bandwidth_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_bandwidth_update_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving bandwidth update: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle interface state events
                                sample_result = interface_event_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_interface_event_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving interface event: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle backend health status
                                sample_result = health_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_health_status_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving health status: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle TC configuration updates
                                sample_result = tc_config_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_tc_config_update_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving TC config update: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle scenario execution updates
                                sample_result = scenario_execution_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_scenario_execution_update_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving scenario execution update: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle backend liveliness changes (presence detection)
                                sample_result = liveliness_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_liveliness_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving liveliness update: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle outgoing TC queries
                                Some(tc_query) = tc_query_receiver.recv() => {
                                    let topic = topics::tc_query_service(&tc_query.backend_name);
                                    match serde_json::to_string(&tc_query.request) {
                                        Ok(payload) => {
                                            match session.get(&topic).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes) {
                                                                            if let Ok(response) = serde_json::from_str::<TcResponse>(payload_str) {
                                                                                // Send response back via original response channel if available
                                                                                if let Some(ref response_sender) = tc_query.response_sender {
                                                                                    let _ = response_sender.send((tc_query.backend_name.clone(), response));
                                                                                }
                                                                            }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("TC query reply error: {}", e);
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                                Err(e) => {
                                                    error!("Failed to send TC query to '{}': {}", tc_query.backend_name, e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to serialize TC request: {}", e);
                                        }
                                    }
                                }

                                // Handle outgoing interface control queries
                                Some(interface_query) = interface_control_receiver.recv() => {
                                    let topic = topics::interface_query_service(&interface_query.backend_name);
                                    match serde_json::to_string(&interface_query.request) {
                                        Ok(payload) => {
                                            match session.get(&topic).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes) {
                                                                            if let Ok(response) = serde_json::from_str::<InterfaceControlResponse>(payload_str) {
                                                                                // Send response back via original response channel if available
                                                                                if let Some(ref response_sender) = interface_query.response_sender {
                                                                                    let _ = response_sender.send((interface_query.backend_name.clone(), response));
                                                                                }
                                                                            }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("Interface control query reply error: {}", e);
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                                Err(e) => {
                                                    error!("Failed to send interface control query to '{}': {}", interface_query.backend_name, e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to serialize interface control request: {}", e);
                                        }
                                    }
                                }

                                // Handle outgoing scenario queries
                                Some(scenario_query) = scenario_query_receiver.recv() => {
                                    use tcgui_shared::scenario::ScenarioResponse;
                                    let topic = topics::scenario_query_service(&scenario_query.backend_name);
                                    let backend_name = scenario_query.backend_name.clone();
                                    let mut output_clone = output.clone();

                                    match serde_json::to_string(&scenario_query.request) {
                                        Ok(payload) => {
                                            match session.get(&topic).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes) {
                                                                        if let Ok(response) = serde_json::from_str::<ScenarioResponse>(payload_str) {
                                                                            // Send response as ZenohEvent
                                                                            let event = ZenohEvent::ScenarioResponse {
                                                                                backend_name: backend_name.clone(),
                                                                                response,
                                                                            };
                                                                            let _ = output_clone.send(event).await;
                                                                        }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("Scenario query reply error: {}", e);
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                                Err(e) => {
                                                    error!("Failed to send scenario query to '{}': {}", scenario_query.backend_name, e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to serialize scenario request: {}", e);
                                        }
                                    }
                                }

                                // Handle outgoing scenario execution queries
                                Some(execution_query) = scenario_execution_query_receiver.recv() => {
                                    use tcgui_shared::scenario::ScenarioExecutionResponse;
                                    let topic = topics::scenario_execution_query_service(&execution_query.backend_name);
                                    match serde_json::to_string(&execution_query.request) {
                                        Ok(payload) => {
                                            match session.get(&topic).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes) {
                                                                            if let Ok(response) = serde_json::from_str::<ScenarioExecutionResponse>(payload_str) {
                                                                                // Send response back via original response channel if available
                                                                                if let Some(ref response_sender) = execution_query.response_sender {
                                                                                    let _ = response_sender.send((execution_query.backend_name.clone(), response));
                                                                                }
                                                                            }
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    error!("Scenario execution query reply error: {}", e);
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                                Err(e) => {
                                                    error!("Failed to send scenario execution query to '{}': {}", execution_query.backend_name, e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to serialize scenario execution request: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to open Zenoh session: {}", e);
                        let _ = output.send(ZenohEvent::ConnectionStatus(false)).await;
                    }
                }

                // Connection lost, notify and retry
                let _ = output.send(ZenohEvent::ConnectionStatus(false)).await;
            }
        })
    }
}

/// Creates a zenoh manager sipper with the provided configuration (takes Arc)
pub fn zenoh_manager_with_arc(config: Arc<ZenohConfig>) -> impl Sipper<Never, ZenohEvent> {
    let manager = ZenohManager { config };
    manager.create_sipper()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::{
        topics, BackendMetadata, InterfaceType, NetworkInterface, NetworkNamespace,
    };
    use zenoh::key_expr::{KeyExpr, OwnedKeyExpr};

    // Test the backend name extraction logic that our macro uses
    #[test]
    fn test_backend_name_extraction() {
        // Test valid topic with backend name
        let key_str = "tcgui/backend1/interfaces/list";
        let key_expr = OwnedKeyExpr::try_from(key_str).unwrap();
        let key_ref = KeyExpr::from(&*key_expr);
        let backend_name = topics::extract_backend_name(&key_ref);
        assert_eq!(backend_name, Some("backend1".to_string()));

        // Test invalid topic without backend name
        let invalid_key_str = "invalid/topic";
        let invalid_key_expr = OwnedKeyExpr::try_from(invalid_key_str).unwrap();
        let invalid_key_ref = KeyExpr::from(&*invalid_key_expr);
        let no_backend = topics::extract_backend_name(&invalid_key_ref);
        assert!(no_backend.is_none());
    }

    // Test JSON serialization/deserialization of message types
    #[test]
    fn test_interface_update_serialization() {
        let interface = NetworkInterface {
            name: "test0".to_string(),
            index: 1,
            namespace: "default".to_string(),
            is_up: true,
            has_tc_qdisc: false,
            interface_type: InterfaceType::Physical,
        };

        let namespace = NetworkNamespace {
            name: "default".to_string(),
            id: Some(0),
            is_active: true,
            interfaces: vec![interface],
        };

        let update = InterfaceListUpdate {
            namespaces: vec![namespace],
            timestamp: 1234567890,
            backend_name: "backend1".to_string(),
        };

        let json = serde_json::to_string(&update).unwrap();
        let deserialized: InterfaceListUpdate = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.namespaces.len(), 1);
        assert_eq!(deserialized.namespaces[0].name, "default");
        assert_eq!(deserialized.backend_name, "backend1");
    }

    #[test]
    fn test_bandwidth_update_serialization() {
        let bandwidth_stats = tcgui_shared::NetworkBandwidthStats {
            rx_bytes: 50000,
            rx_packets: 500,
            rx_errors: 0,
            rx_dropped: 0,
            tx_bytes: 25000,
            tx_packets: 250,
            tx_errors: 0,
            tx_dropped: 0,
            timestamp: 1234567890,
            rx_bytes_per_sec: 1000.0,
            tx_bytes_per_sec: 500.0,
        };

        let update = BandwidthUpdate {
            namespace: "default".to_string(),
            interface: "eth0".to_string(),
            stats: bandwidth_stats,
            backend_name: "backend1".to_string(),
        };

        let json = serde_json::to_string(&update).unwrap();
        let deserialized: BandwidthUpdate = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.namespace, "default");
        assert_eq!(deserialized.interface, "eth0");
        assert_eq!(deserialized.stats.rx_bytes_per_sec, 1000.0);
        assert_eq!(deserialized.backend_name, "backend1");
    }

    #[test]
    fn test_health_status_serialization() {
        let metadata = BackendMetadata {
            version: Some("1.0.0".to_string()),
            hostname: Some("test-host".to_string()),
            started_at: Some(1234567000),
            capabilities: vec!["tc_netem".to_string()],
        };

        let health = BackendHealthStatus {
            backend_name: "backend1".to_string(),
            status: "healthy".to_string(),
            timestamp: 1234567890,
            metadata,
            namespace_count: 1,
            interface_count: 2,
        };

        let json = serde_json::to_string(&health).unwrap();
        let deserialized: BackendHealthStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.status, "healthy");
        assert_eq!(deserialized.metadata.version, Some("1.0.0".to_string()));
        assert_eq!(deserialized.interface_count, 2);
        assert_eq!(deserialized.backend_name, "backend1");
    }

    #[test]
    fn test_invalid_json_deserialization() {
        let result = serde_json::from_str::<InterfaceListUpdate>("invalid json");
        assert!(result.is_err());

        let result = serde_json::from_str::<BandwidthUpdate>("not valid json at all");
        assert!(result.is_err());

        let result = serde_json::from_str::<BackendHealthStatus>("{}");
        assert!(result.is_err()); // Should fail due to missing required fields
    }
}
