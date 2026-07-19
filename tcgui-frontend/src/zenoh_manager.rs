use iced::Subscription;
use iced::task::{Never, Sipper, sipper};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tcgui_shared::{
    BackendHealthStatus, BandwidthUpdate, InterfaceControlResponse, NetworkInterface,
    TcConfigUpdate, TcResponse, TcStatisticsUpdate, ZenohConfig,
    identity::RemoteOrigin,
    presets::CustomPreset,
    registry::tc,
    scenario::{NetworkScenario, ScenarioExecutionRequest, ScenarioExecutionUpdate},
    topics,
};
use tokio::sync::mpsc;
use tracing::{error, info, trace};
use zenoh::sample::{Sample, SampleKind};
use zenoh_ext::{AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig};

use crate::messages::{
    DiagnosticsQueryMessage, InterfaceControlQueryMessage, ScenarioExecutionQueryMessage,
    ScenarioQueryMessage, TcQueryMessage, ZenohEvent,
};

/// Extract a human-readable message from a Zenoh reply-error payload.
///
/// Backends signal query failures on the reply-error channel (RFC keyspace-v2
/// 05 §3) carrying a namespaced `error/<service>: <detail>` string; surface that
/// verbatim, falling back to a generic note if it is not valid UTF-8.
fn reply_error_message(err: &zenoh::query::ReplyError) -> String {
    let bytes = err.payload().to_bytes();
    match std::str::from_utf8(&bytes) {
        Ok(s) if !s.is_empty() => s.to_string(),
        _ => "backend reported an error".to_string(),
    }
}

/// Deserialize a sample's JSON payload into `T`, logging (and swallowing) any
/// UTF-8 or JSON error. Only called on `Put` samples — a `Delete` tombstone
/// carries no payload, so its routing is derived from the key instead.
fn deser_payload<T: DeserializeOwned>(sample: &Sample, ctx: &str) -> Option<T> {
    let bytes = sample.payload().to_bytes();
    match std::str::from_utf8(&bytes) {
        Ok(s) => match serde_json::from_str::<T>(s) {
            Ok(v) => Some(v),
            Err(e) => {
                error!("Failed to deserialize {}: {}", ctx, e);
                None
            }
        },
        Err(e) => {
            error!("Failed to read {} payload as UTF-8: {}", ctx, e);
            None
        }
    }
}

/// Route a `state`-plane sample (Put upsert or Delete tombstone).
///
/// Every family must inspect [`Sample::kind`] now that removal is a Delete
/// tombstone rather than a snapshot diff (RFC keyspace-v2 04 §1.2): a Put
/// deserializes the payload; a Delete carries none and its `ns`/`iface`/`id`
/// come from the key.
fn handle_state_sample(sample: Sample) -> Option<ZenohEvent> {
    let key = sample.key_expr().as_str();
    let sk = match topics::parse_state_key(key) {
        Some(sk) => sk,
        None => {
            // Foreign producers, the reserved `alive` leaf, and unregistered
            // subjects all refine to nothing — not an error (RFC 08 §1).
            trace!("Ignoring non-tc state key: {}", key);
            return None;
        }
    };
    let is_delete = sample.kind() == SampleKind::Delete;

    match sk.subject {
        // The sensor document is not consumed by the GUI.
        tc::Subject::Sensor => None,

        tc::Subject::Health => {
            if is_delete {
                return None;
            }
            let mut health: BackendHealthStatus = deser_payload(&sample, "health status")?;
            // The key origin is authoritative; backfill it if the payload omitted it.
            if health.host_id.is_empty() {
                health.host_id = sk.origin.clone();
            }
            info!(
                "Frontend received health status from '{}': {}",
                sk.origin, health.status
            );
            Some(ZenohEvent::BackendHealthUpdate(health))
        }

        tc::Subject::Interface { ns, iface } => {
            let namespace = ns.to_string();
            let interface = iface.to_string();
            if is_delete {
                info!(
                    "Interface removed (tombstone) on '{}': {}/{}",
                    sk.origin, namespace, interface
                );
                Some(ZenohEvent::InterfaceRemoved {
                    backend_name: sk.origin,
                    namespace,
                    interface,
                })
            } else {
                let record: NetworkInterface = deser_payload(&sample, "interface record")?;
                Some(ZenohEvent::InterfaceUpsert {
                    backend_name: sk.origin,
                    interface: record,
                })
            }
        }

        tc::Subject::Config { ns, iface } => {
            let namespace = ns.to_string();
            let interface = iface.to_string();
            if is_delete {
                // Clearing config is a Delete, never a `None` payload (04 §1.2).
                // Synthesize a "no TC" update so the interface UI resets.
                Some(ZenohEvent::TcConfigUpdate(TcConfigUpdate {
                    namespace,
                    interface,
                    backend_name: sk.origin,
                    timestamp: 0,
                    configuration: None,
                    has_tc: false,
                }))
            } else {
                let mut update: TcConfigUpdate = deser_payload(&sample, "TC config update")?;
                update.backend_name = sk.origin;
                Some(ZenohEvent::TcConfigUpdate(update))
            }
        }

        tc::Subject::Execution { .. } if is_delete => {
            let tc::Subject::Execution { ns, iface } = sk.subject else {
                return None;
            };
            Some(ZenohEvent::ScenarioExecutionRemoved {
                backend_name: sk.origin,
                namespace: ns.to_string(),
                interface: iface.to_string(),
            })
        }
        tc::Subject::Execution { .. } => {
            let mut update: ScenarioExecutionUpdate =
                deser_payload(&sample, "scenario execution update")?;
            update.backend_name = sk.origin;
            Some(ZenohEvent::ScenarioExecutionUpdate(Box::new(update)))
        }

        tc::Subject::Scenario { id } => {
            let id = id.to_string();
            if is_delete {
                Some(ZenohEvent::ScenarioRemoved {
                    backend_name: sk.origin,
                    id,
                })
            } else {
                let scenario: NetworkScenario = deser_payload(&sample, "scenario entry")?;
                Some(ZenohEvent::ScenarioUpsert {
                    backend_name: sk.origin,
                    scenario: Box::new(scenario),
                })
            }
        }

        tc::Subject::Preset { id } => {
            let id = id.to_string();
            if is_delete {
                Some(ZenohEvent::PresetRemoved {
                    backend_name: sk.origin,
                    id,
                })
            } else {
                let preset: CustomPreset = deser_payload(&sample, "preset entry")?;
                Some(ZenohEvent::PresetUpsert {
                    backend_name: sk.origin,
                    preset,
                })
            }
        }

        // Telemetry/events subjects cannot appear under the state class.
        _ => None,
    }
}

/// Route a `telemetry`-plane sample. Payloads are self-describing (carry
/// ns/iface); the key's kind chunk selects the target type and its origin
/// becomes the routing backend id.
fn handle_telemetry_sample(sample: Sample) -> Option<ZenohEvent> {
    let key = sample.key_expr().as_str();
    let origin = topics::parse_origin(key)?;
    match topics::telemetry_kind(key)? {
        "bandwidth" => {
            let mut update: BandwidthUpdate = deser_payload(&sample, "bandwidth update")?;
            update.backend_name = origin;
            trace!(
                "Frontend received bandwidth update for {}/{}: RX {:.2} B/s, TX {:.2} B/s",
                update.namespace,
                update.interface,
                update.stats.rx_bytes_per_sec,
                update.stats.tx_bytes_per_sec
            );
            Some(ZenohEvent::BandwidthUpdate(update))
        }
        "qdisc" => {
            let mut update: TcStatisticsUpdate = deser_payload(&sample, "TC stats update")?;
            update.backend_name = origin;
            Some(ZenohEvent::TcStatisticsUpdate(update))
        }
        other => {
            trace!("Ignoring unknown telemetry kind '{}' on {}", other, key);
            None
        }
    }
}

/// Handles zenoh liveliness samples for backend presence detection. The whole
/// presence protocol is the reserved `state/*/alive` leaf: a Put means the
/// backend is present, a Delete means it is gone (04 §5).
fn handle_liveliness_sample(sample: Sample) -> Option<ZenohEvent> {
    let key = sample.key_expr().as_str();
    match topics::parse_origin(key) {
        Some(origin) => {
            let alive = sample.kind() == SampleKind::Put;
            info!(
                "Frontend received liveliness update for backend '{}': {}",
                origin,
                if alive { "alive" } else { "disconnected" }
            );
            Some(ZenohEvent::BackendLiveliness {
                backend_name: origin,
                alive,
            })
        }
        None => {
            error!("Could not extract origin from liveliness key: {}", key);
            None
        }
    }
}

/// Extract the (namespace, interface) target from an execution request. Every
/// variant the frontend sends (Start/Stop/Pause/Resume) carries one; the
/// query-only variants (Status/ListActive) do not and are not routed here.
fn execution_target(request: &ScenarioExecutionRequest) -> Option<(&str, &str)> {
    match request {
        ScenarioExecutionRequest::Start {
            namespace,
            interface,
            ..
        }
        | ScenarioExecutionRequest::Stop {
            namespace,
            interface,
        }
        | ScenarioExecutionRequest::Pause {
            namespace,
            interface,
        }
        | ScenarioExecutionRequest::Resume {
            namespace,
            interface,
        }
        | ScenarioExecutionRequest::Status {
            namespace,
            interface,
        } => Some((namespace, interface)),
        ScenarioExecutionRequest::ListActive => None,
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
    pub fn create_sipper(self) -> impl Sipper<Never, ZenohEvent> {
        let config = self.config;
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
                        let (diagnostics_query_sender, mut diagnostics_query_receiver) =
                            mpsc::unbounded_channel::<DiagnosticsQueryMessage>();

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
                        let _ = output
                            .send(ZenohEvent::DiagnosticsQueryChannelReady(
                                diagnostics_query_sender,
                            ))
                            .await;

                        // Single state-plane subscriber (LWW; delete = tombstone).
                        // History detects late publishers; recovery uses
                        // periodic queries rather than a per-publisher heartbeat —
                        // RFC keyspace-v2 04 §3.3 prefers `periodic_queries` on a
                        // wide wildcard subscription, where an unaggregated
                        // heartbeat per publisher scales with fleet × interface.
                        let state_subscriber = match session
                            .declare_subscriber(topics::sel_state())
                            .history(HistoryConfig::default().detect_late_publishers())
                            .recovery(
                                RecoveryConfig::default()
                                    .periodic_queries(std::time::Duration::from_secs(5)),
                            )
                            .subscriber_detection()
                            .await
                        {
                            Ok(subscriber) => {
                                info!("Subscribed to state plane: {}", topics::sel_state());
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to state plane: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Telemetry-plane subscriber (superseded; best-effort).
                        let telemetry_subscriber = match session
                            .declare_subscriber(topics::sel_telemetry())
                            .await
                        {
                            Ok(subscriber) => {
                                info!("Subscribed to telemetry plane: {}", topics::sel_telemetry());
                                subscriber
                            }
                            Err(e) => {
                                error!("Failed to subscribe to telemetry plane: {}", e);
                                continue; // Retry connection
                            }
                        };

                        // Liveliness subscriber on the reserved alive leaf.
                        let liveliness_subscriber = match session
                            .liveliness()
                            .declare_subscriber(topics::sel_alive())
                            .history(true)
                            .await
                        {
                            Ok(subscriber) => {
                                info!("Subscribed to backend liveliness: {}", topics::sel_alive());
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
                                // Handle state-plane samples (put upsert / delete tombstone)
                                sample_result = state_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_state_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving state sample: {}", e);
                                            break;
                                        }
                                    }
                                }

                                // Handle telemetry-plane samples (bandwidth / qdisc stats)
                                sample_result = telemetry_subscriber.recv_async() => {
                                    match sample_result {
                                        Ok(sample) => {
                                            if let Some(event) = handle_telemetry_sample(sample) {
                                                let _ = output.send(event).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error receiving telemetry sample: {}", e);
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
                                    let origin = match RemoteOrigin::parse(&tc_query.backend_name) {
                                        Ok(o) => o,
                                        Err(_) => {
                                            error!("Refusing TC query: '{}' is not a concrete origin", tc_query.backend_name);
                                            continue;
                                        }
                                    };
                                    let topic = tc::config_ns_iface_set_key(&origin, &tc_query.request.namespace, &tc_query.request.interface);
                                    let mut output_clone = output.clone();
                                    let backend_name = tc_query.backend_name.clone();
                                    match serde_json::to_string(&tc_query.request) {
                                        Ok(payload) => {
                                            match session.get(topic.as_str()).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes)
                                                                            && let Ok(response) = serde_json::from_str::<TcResponse>(payload_str) {
                                                                                // Forward the result so the app can surface failures.
                                                                                let _ = output_clone.send(ZenohEvent::TcOperationResult {
                                                                                    backend_name: backend_name.clone(),
                                                                                    response,
                                                                                }).await;
                                                                            }
                                                                }
                                                                Err(e) => {
                                                                    let error = reply_error_message(&e);
                                                                    error!("TC query reply error: {}", error);
                                                                    let _ = output_clone.send(ZenohEvent::QueryError {
                                                                        backend_name: backend_name.clone(),
                                                                        error,
                                                                    }).await;
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
                                    let origin = match RemoteOrigin::parse(&interface_query.backend_name) {
                                        Ok(o) => o,
                                        Err(_) => {
                                            error!("Refusing interface control query: '{}' is not a concrete origin", interface_query.backend_name);
                                            continue;
                                        }
                                    };
                                    let topic = tc::interface_ns_iface_set_key(&origin, &interface_query.request.namespace, &interface_query.request.interface);
                                    let mut output_clone = output.clone();
                                    let backend_name = interface_query.backend_name.clone();
                                    match serde_json::to_string(&interface_query.request) {
                                        Ok(payload) => {
                                            match session.get(topic.as_str()).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes)
                                                                            && let Ok(response) = serde_json::from_str::<InterfaceControlResponse>(payload_str) {
                                                                                // Forward the result so the app can surface failures.
                                                                                let _ = output_clone.send(ZenohEvent::InterfaceControlResult {
                                                                                    backend_name: backend_name.clone(),
                                                                                    response,
                                                                                }).await;
                                                                            }
                                                                }
                                                                Err(e) => {
                                                                    let error = reply_error_message(&e);
                                                                    error!("Interface control query reply error: {}", error);
                                                                    let _ = output_clone.send(ZenohEvent::QueryError {
                                                                        backend_name: backend_name.clone(),
                                                                        error,
                                                                    }).await;
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
                                    let origin = match RemoteOrigin::parse(&scenario_query.backend_name) {
                                        Ok(o) => o,
                                        Err(_) => {
                                            error!("Refusing scenario query: '{}' is not a concrete origin", scenario_query.backend_name);
                                            continue;
                                        }
                                    };
                                    let topic = tc::scenario_set_key(&origin);
                                    let backend_name = scenario_query.backend_name.clone();
                                    let mut output_clone = output.clone();

                                    match serde_json::to_string(&scenario_query.request) {
                                        Ok(payload) => {
                                            match session.get(topic.as_str()).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes)
                                                                        && let Ok(response) = serde_json::from_str::<ScenarioResponse>(payload_str) {
                                                                            // Send response as ZenohEvent
                                                                            let event = ZenohEvent::ScenarioResponse {
                                                                                backend_name: backend_name.clone(),
                                                                                response,
                                                                            };
                                                                            let _ = output_clone.send(event).await;
                                                                        }
                                                                }
                                                                Err(e) => {
                                                                    let error = reply_error_message(&e);
                                                                    error!("Scenario query reply error: {}", error);
                                                                    let _ = output_clone.send(ZenohEvent::QueryError {
                                                                        backend_name: backend_name.clone(),
                                                                        error,
                                                                    }).await;
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
                                    let origin = match RemoteOrigin::parse(&execution_query.backend_name) {
                                        Ok(o) => o,
                                        Err(_) => {
                                            error!("Refusing scenario execution query: '{}' is not a concrete origin", execution_query.backend_name);
                                            continue;
                                        }
                                    };
                                    let (namespace, interface) = match execution_target(&execution_query.request) {
                                        Some(target) => target,
                                        None => {
                                            error!("Scenario execution request has no interface target; skipping");
                                            continue;
                                        }
                                    };
                                    let topic = tc::execution_ns_iface_set_key(&origin, namespace, interface);
                                    let mut output_clone = output.clone();
                                    let backend_name = execution_query.backend_name.clone();
                                    match serde_json::to_string(&execution_query.request) {
                                        Ok(payload) => {
                                            match session.get(topic.as_str()).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes)
                                                                            && let Ok(response) = serde_json::from_str::<ScenarioExecutionResponse>(payload_str) {
                                                                                // Send response back via original response channel if available
                                                                                if let Some(ref response_sender) = execution_query.response_sender {
                                                                                    let _ = response_sender.send((execution_query.backend_name.clone(), response));
                                                                                }
                                                                            }
                                                                }
                                                                Err(e) => {
                                                                    let error = reply_error_message(&e);
                                                                    error!("Scenario execution query reply error: {}", error);
                                                                    let _ = output_clone.send(ZenohEvent::QueryError {
                                                                        backend_name: backend_name.clone(),
                                                                        error,
                                                                    }).await;
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

                                // Handle outgoing diagnostics queries
                                Some(diag_query) = diagnostics_query_receiver.recv() => {
                                    use tcgui_shared::DiagnosticsResponse;
                                    let origin = match RemoteOrigin::parse(&diag_query.backend_name) {
                                        Ok(o) => o,
                                        Err(_) => {
                                            error!("Refusing diagnostics query: '{}' is not a concrete origin", diag_query.backend_name);
                                            continue;
                                        }
                                    };
                                    let topic = tc::diagnostics_key(&origin);
                                    let mut output_clone = output.clone();
                                    let backend_name = diag_query.backend_name.clone();
                                    let namespace = diag_query.request.namespace.clone();
                                    let interface = diag_query.request.interface.clone();

                                    match serde_json::to_string(&diag_query.request) {
                                        Ok(payload) => {
                                            match session.get(topic.as_str()).payload(payload).await {
                                                Ok(replies) => {
                                                    tokio::spawn(async move {
                                                        while let Ok(reply) = replies.recv_async().await {
                                                            match reply.into_result() {
                                                                Ok(sample) => {
                                                                    let payload_bytes = sample.payload().to_bytes();
                                                                    if let Ok(payload_str) = std::str::from_utf8(&payload_bytes)
                                                                        && let Ok(response) = serde_json::from_str::<DiagnosticsResponse>(payload_str)
                                                                    {
                                                                        info!("Received diagnostics response for {}/{}: {}", namespace, interface, response.message);
                                                                        let _ = output_clone.send(ZenohEvent::DiagnosticsResponse {
                                                                            backend_name: backend_name.clone(),
                                                                            namespace: namespace.clone(),
                                                                            interface: interface.clone(),
                                                                            response,
                                                                        }).await;
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    let error = reply_error_message(&e);
                                                                    error!("Diagnostics query reply error: {}", error);
                                                                    let _ = output_clone.send(ZenohEvent::QueryError {
                                                                        backend_name: backend_name.clone(),
                                                                        error,
                                                                    }).await;
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                                Err(e) => {
                                                    error!("Failed to send diagnostics query to '{}': {}", backend_name, e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to serialize diagnostics request: {}", e);
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
    ZenohManager { config }.create_sipper()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::identity::ConcreteOrigin as _;
    use tcgui_shared::topics;

    #[test]
    fn test_parse_origin_from_state_key() {
        let origin = tcgui_shared::identity::local_origin_from_seed("machine-a");
        let key = tc::key(&origin, &tc::Subject::interface("default", "eth0"));
        let parsed = topics::parse_origin(key.as_str());
        assert_eq!(parsed.as_deref(), Some(origin.chunk()));
    }

    #[test]
    fn test_parse_state_key_interface() {
        let origin = tcgui_shared::identity::local_origin_from_seed("machine-a");
        let key = tc::key(&origin, &tc::Subject::interface("default", "eth0"));
        let sk = topics::parse_state_key(key.as_str()).expect("parse");
        assert_eq!(sk.subject, tc::Subject::interface("default", "eth0"));
        assert_eq!(sk.subject.family(), tc::Family::Interface);
        assert_eq!(sk.origin, origin.chunk());
    }

    #[test]
    fn test_telemetry_kind_extraction() {
        let origin = tcgui_shared::identity::local_origin_from_seed("machine-a");
        let bw = tc::key(&origin, &tc::Subject::bandwidth("default", "eth0"));
        assert_eq!(topics::telemetry_kind(bw.as_str()), Some("bandwidth"));
        let qd = tc::key(&origin, &tc::Subject::qdisc("default", "eth0"));
        assert_eq!(topics::telemetry_kind(qd.as_str()), Some("qdisc"));
    }

    #[test]
    fn test_remote_origin_round_trip_builds_rpc_key() {
        let local = tcgui_shared::identity::local_origin_from_seed("machine-a");
        let remote = RemoteOrigin::parse(local.chunk()).expect("valid origin");
        let key = tc::config_ns_iface_set_key(&remote, "default", "eth0");
        assert!(key.as_str().contains(local.chunk()));
        assert!(key.as_str().ends_with("/set"));
    }

    #[test]
    fn test_execution_target_extraction() {
        let start = ScenarioExecutionRequest::Stop {
            namespace: "default".to_string(),
            interface: "eth0".to_string(),
        };
        assert_eq!(execution_target(&start), Some(("default", "eth0")));
        assert_eq!(
            execution_target(&ScenarioExecutionRequest::ListActive),
            None
        );
    }

    // JSON serialization sanity for the payload types the state/telemetry
    // handlers deserialize.
    #[test]
    fn test_health_status_serialization() {
        let health = BackendHealthStatus {
            host_id: "h-000000000001".to_string(),
            backend_name: "backend1".to_string(),
            status: "healthy".to_string(),
            timestamp: 1234567890,
            metadata: tcgui_shared::BackendMetadata::default(),
            namespace_count: 1,
            interface_count: 2,
        };
        let json = serde_json::to_string(&health).unwrap();
        let deserialized: BackendHealthStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.host_id, "h-000000000001");
        assert_eq!(deserialized.interface_count, 2);
    }

    #[test]
    fn test_invalid_json_deserialization() {
        assert!(serde_json::from_str::<BandwidthUpdate>("not valid json").is_err());
        assert!(serde_json::from_str::<BackendHealthStatus>("{}").is_err());
    }
}
