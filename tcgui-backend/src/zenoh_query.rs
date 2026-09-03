//! Zenoh communication handlers for `TcBackend`: the TC / interface-control /
//! diagnostics query-reply handlers plus the backend-status, preset-list, and
//! publisher-management helpers.
//!
//! Extracted from `main.rs` (#20) to keep the entry point focused — behavior is
//! unchanged. These are inherent methods on `TcBackend`; the run loop in
//! `main.rs` dispatches to them.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::time::Duration;
use tracing::{debug, info, instrument, warn};
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::registry::tc;
use tcgui_shared::{
    BackendHealthStatus, BackendMetadata, InterfaceControlOperation, InterfaceControlRequest,
    InterfaceControlResponse, NetworkInterface, TcNetemConfig, TcOperation, TcRequest, TcResponse,
    errors::TcguiError,
};
use zenkey::ConcreteOrigin as _;

use crate::TcBackend;
use crate::{diagnostics, tc_config};

impl TcBackend {
    /// Reply to a query with a success value on the queryable's **own concrete
    /// key** — never the echoed `query.key_expr()`, which for a `*`-origin
    /// fan-in is the shared wildcard key that Zenoh consolidation collapses to a
    /// single surviving reply (RFC keyspace-v2 05 §2.1). Passing the concrete
    /// service key keeps every backend's reply distinct.
    async fn reply_value(
        &self,
        query: &zenoh::query::Query,
        concrete_key: zenoh::key_expr::OwnedKeyExpr,
        payload: String,
    ) -> Result<()> {
        query
            .reply(concrete_key, payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to reply to query: {e}"),
            })?;
        Ok(())
    }

    /// Signal a failure on Zenoh's **reply-error channel** with a namespaced
    /// error name (RFC keyspace-v2 05 §3: a value reply always means success; a
    /// failure always rides `reply_err`). `error_name` is a stable
    /// `error/<service>[/<kind>]` slug; `message` carries the human detail.
    async fn reply_query_error(
        &self,
        query: &zenoh::query::Query,
        error_name: &str,
        message: &str,
    ) -> Result<()> {
        query
            .reply_err(format!("{error_name}: {message}"))
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to reply_err to query: {e}"),
            })?;
        Ok(())
    }

    /// Reject a TC query on the reply-error channel (used for invalid input).
    async fn reply_tc_error(&self, query: &zenoh::query::Query, message: String) -> Result<()> {
        self.reply_query_error(query, "error/tc/invalid-request", &message)
            .await
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    pub(crate) async fn handle_tc_query(&mut self, query: zenoh::query::Query) -> Result<()> {
        // A decode failure MUST reply on the error channel, not `?` out of here:
        // the caller only logs, so bailing produced no reply at all and the GUI
        // simply timed out (RFC keyspace-v2 05 §3).
        let request: TcRequest = match tcgui_shared::rpc::decode_request(query.payload(), "tc") {
            Ok(request) => request,
            Err(fault) => {
                warn!("Rejecting malformed TC request: {}", fault.message);
                return self
                    .reply_query_error(&query, &fault.name, &fault.message)
                    .await;
            }
        };
        info!("Received TC query: {:?}", request);

        // Validate the request target before any privileged operation.
        if let Err(reason) =
            tcgui_shared::validation::validate_target(&request.namespace, &request.interface)
        {
            warn!(
                "Rejecting TC request for {}/{}: {}",
                request.namespace, request.interface, reason
            );
            return self
                .reply_tc_error(&query, format!("Invalid request: {reason}"))
                .await;
        }

        // `Ok` is a success value for the value channel; `Err` is the detail
        // for `reply_err`. Previously both rode a `TcResponse` with a `success`
        // flag, which meant the type could spell a failure that a consumer had
        // to remember to check (RFC 05 §3).
        let result: std::result::Result<TcResponse, String> = match &request.operation {
            TcOperation::ApplyConfig { config } => {
                let result = self
                    .tc_manager
                    .apply_tc_config_structured(&request.namespace, &request.interface, config)
                    .await;

                match result {
                    Ok(_) => {
                        // Convert structured config to legacy TcConfiguration for publishing
                        let (
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
                        ) = config.to_legacy_params();

                        // Use helper function to build configuration
                        let applied_config = tc_config::build_tc_configuration(
                            &request.interface,
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
                        );

                        // Publish TC configuration update with actual config
                        if let Err(e) = self
                            .publish_tc_config(
                                &request.namespace,
                                &request.interface,
                                Some(applied_config.clone()),
                            )
                            .await
                        {
                            warn!("Failed to publish TC config update: {}", e);
                        }

                        Ok(TcResponse {
                            message: format!(
                                "Structured TC config applied successfully to {}:{}",
                                request.namespace, request.interface
                            ),
                            applied_config: Some(applied_config),
                        })
                    }
                    Err(e) => Err(format!("Failed to apply structured TC config: {e}")),
                }
            }
            TcOperation::Apply {
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
            } => {
                // Convert legacy parameters to structured config
                let config = TcNetemConfig::from_legacy_params(
                    *loss,
                    *correlation,
                    *delay_ms,
                    *delay_jitter_ms,
                    *delay_correlation,
                    *duplicate_percent,
                    *duplicate_correlation,
                    *reorder_percent,
                    *reorder_correlation,
                    *reorder_gap,
                    *corrupt_percent,
                    *corrupt_correlation,
                    *rate_limit_kbps,
                );

                // Check if any features are enabled
                let has_meaningful_params = config.has_any_enabled();

                let result = if has_meaningful_params {
                    // Apply TC using structured API
                    self.tc_manager
                        .apply_tc_config_structured(&request.namespace, &request.interface, &config)
                        .await
                } else {
                    // No meaningful parameters - remove TC qdisc entirely
                    info!(
                        "No meaningful TC parameters provided, removing TC qdisc from {}:{}",
                        request.namespace, request.interface
                    );
                    self.tc_manager
                        .remove_tc_config_in_namespace(&request.namespace, &request.interface)
                        .await
                };

                match result {
                    Ok(_) => {
                        if has_meaningful_params {
                            // Convert back to legacy format for response/publishing
                            let (
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
                            ) = config.to_legacy_params();

                            // Use helper function to build configuration
                            let applied_config = tc_config::build_tc_configuration(
                                &request.interface,
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
                            );

                            // Publish TC configuration update so frontend knows the current state
                            if let Err(e) = self
                                .publish_tc_config(
                                    &request.namespace,
                                    &request.interface,
                                    Some(applied_config.clone()),
                                )
                                .await
                            {
                                warn!("Failed to publish TC config update: {}", e);
                            }

                            Ok(TcResponse {
                                message: format!(
                                    "TC applied successfully to {}:{}",
                                    request.namespace, request.interface
                                ),
                                applied_config: Some(applied_config),
                            })
                        } else {
                            // No meaningful parameters - TC qdisc was removed
                            // Publish TC configuration removal (None config)
                            if let Err(e) = self
                                .publish_tc_config(&request.namespace, &request.interface, None)
                                .await
                            {
                                warn!("Failed to publish TC config removal: {}", e);
                            }

                            Ok(TcResponse {
                                message: format!(
                                    "TC removed from {}:{} (no meaningful parameters)",
                                    request.namespace, request.interface
                                ),
                                applied_config: None,
                            })
                        }
                    }
                    Err(e) => Err(format!(
                        "Failed to {} TC: {e}",
                        if has_meaningful_params {
                            "apply"
                        } else {
                            "remove"
                        }
                    )),
                }
            }
            TcOperation::Remove => {
                let result = self
                    .tc_manager
                    .remove_tc_config_in_namespace(&request.namespace, &request.interface)
                    .await;

                match result {
                    Ok(_) => {
                        // Publish TC configuration removal (None config)
                        if let Err(e) = self
                            .publish_tc_config(&request.namespace, &request.interface, None)
                            .await
                        {
                            warn!("Failed to publish TC config removal: {}", e);
                        }

                        Ok(TcResponse {
                            message: format!(
                                "TC removed successfully from {}:{}",
                                request.namespace, request.interface
                            ),
                            applied_config: None,
                        })
                    }
                    Err(e) => Err(format!("Failed to remove TC: {e}")),
                }
            }
        };

        // Success rides the value channel on our concrete key; failure rides
        // reply_err (RFC 05 §2.1 / §3).
        match result {
            Ok(response) => {
                // Audit record for an operator-driven change only — a scenario
                // step is excluded by `is_auditable` (rate budget).
                if Self::is_auditable(&request.operation) {
                    self.publish_applied_event(
                        &request.namespace,
                        &request.interface,
                        Self::audited_config(&request.operation),
                    )
                    .await;
                }

                let payload = serde_json::to_string(&response)?;
                self.reply_value(
                    &query,
                    tc::config_ns_iface_set_key(
                        &self.local_origin,
                        &request.namespace,
                        &request.interface,
                    )
                    .into(),
                    payload,
                )
                .await?;
            }
            Err(message) => {
                self.reply_query_error(&query, "error/tc/apply", &message)
                    .await?;
            }
        }

        Ok(())
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    pub(crate) async fn handle_interface_query(
        &mut self,
        query: zenoh::query::Query,
    ) -> Result<()> {
        // Decode failures ride the error channel; this handler also had no
        // payload size guard at all, which `decode_request` now supplies.
        let request: InterfaceControlRequest =
            match tcgui_shared::rpc::decode_request(query.payload(), "interface") {
                Ok(request) => request,
                Err(fault) => {
                    warn!("Rejecting malformed interface request: {}", fault.message);
                    return self
                        .reply_query_error(&query, &fault.name, &fault.message)
                        .await;
                }
            };
        info!("Received Interface control query: {:?}", request);

        // Validate the request target before any privileged operation.
        if let Err(reason) =
            tcgui_shared::validation::validate_target(&request.namespace, &request.interface)
        {
            warn!(
                "Rejecting interface request for {}/{}: {}",
                request.namespace, request.interface, reason
            );
            return self
                .reply_query_error(
                    &query,
                    "error/interface/invalid-request",
                    &format!("Invalid request: {reason}"),
                )
                .await;
        }

        let result: std::result::Result<InterfaceControlResponse, String> = match &request.operation
        {
            InterfaceControlOperation::Enable => self
                .network_manager
                .enable_interface(&request.namespace, &request.interface)
                .await
                .map(|_| InterfaceControlResponse {
                    message: format!(
                        "Interface {} enabled successfully in namespace {}",
                        request.interface, request.namespace
                    ),
                    new_state: true,
                })
                .map_err(|e| format!("Failed to enable interface: {e}")),
            InterfaceControlOperation::Disable => self
                .network_manager
                .disable_interface(&request.namespace, &request.interface)
                .await
                .map(|_| InterfaceControlResponse {
                    message: format!(
                        "Interface {} disabled successfully in namespace {}",
                        request.interface, request.namespace
                    ),
                    new_state: false,
                })
                .map_err(|e| format!("Failed to disable interface: {e}")),
        };

        match result {
            Ok(response) => {
                let payload = serde_json::to_string(&response)?;
                self.reply_value(
                    &query,
                    tc::interface_ns_iface_set_key(
                        &self.local_origin,
                        &request.namespace,
                        &request.interface,
                    )
                    .into(),
                    payload,
                )
                .await?;
            }
            Err(message) => {
                self.reply_query_error(&query, "error/interface", &message)
                    .await?;
            }
        }

        Ok(())
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    pub(crate) async fn handle_diagnostics_query(&self, query: zenoh::query::Query) -> Result<()> {
        use tcgui_shared::DiagnosticsRequest;

        // Same as above: reply on the error channel, and gain the size guard
        // this handler never had.
        let request: DiagnosticsRequest =
            match tcgui_shared::rpc::decode_request(query.payload(), "diagnostics") {
                Ok(request) => request,
                Err(fault) => {
                    warn!("Rejecting malformed diagnostics request: {}", fault.message);
                    return self
                        .reply_query_error(&query, &fault.name, &fault.message)
                        .await;
                }
            };
        info!(
            "Received Diagnostics query for {}/{}",
            request.namespace, request.interface
        );

        // Validate the request target before touching the namespace/interface.
        // A bad target is an invalid *request*, so it gets the invalid-request
        // name — previously it was misfiled under the generic error/diagnostics.
        if let Err(reason) =
            tcgui_shared::validation::validate_target(&request.namespace, &request.interface)
        {
            warn!(
                "Rejecting diagnostics request for {}/{}: {}",
                request.namespace, request.interface, reason
            );
            return self
                .reply_query_error(
                    &query,
                    "error/diagnostics/invalid-request",
                    &format!("Invalid request: {reason}"),
                )
                .await;
        }

        let diagnostics_service =
            diagnostics::DiagnosticsService::new(&self.network_manager, &self.tc_manager);

        match diagnostics_service.run_diagnostics(&request).await {
            Ok(response) => {
                let payload = serde_json::to_string(&response)?;
                self.reply_value(
                    &query,
                    tc::diagnostics_key(&self.local_origin).into(),
                    payload,
                )
                .await?;
                info!(
                    "Diagnostics completed for {}/{}: {}",
                    request.namespace, request.interface, response.message
                );
            }
            Err(e) => {
                self.reply_query_error(&query, "error/diagnostics", &format!("{e}"))
                    .await?;
            }
        }

        Ok(())
    }

    /// Whether an operation earns an immutable audit record on
    /// `events/tc/applied/{ulid}`.
    ///
    /// Operator-driven applies do; a scenario **step** does not. This is the
    /// RFC 04 §1.3 rate budget — the `events` class is `rate = "low"` (<=1/min),
    /// and a scenario stepping every 500ms would blow it by two orders of
    /// magnitude. A step's state belongs in the LWW execution doc, which is
    /// where it already goes.
    ///
    /// The split is structural rather than a heuristic: `ApplyConfig` is
    /// constructed in exactly one place in the workspace — the scenario
    /// executor (`scenario/execution.rs`) — while the GUI only ever sends
    /// `Apply` and `Remove`.
    fn is_auditable(operation: &TcOperation) -> bool {
        match operation {
            TcOperation::Apply { .. } | TcOperation::Remove => true,
            TcOperation::ApplyConfig { .. } => false,
        }
    }

    /// The configuration to record in the audit event: what was applied, or
    /// `None` when the operation cleared shaping.
    ///
    /// An `Apply` carrying no enabled feature IS a clear — the handler routes
    /// it to `remove_tc_config_in_namespace` — so it records `None` too.
    fn audited_config(operation: &TcOperation) -> Option<TcNetemConfig> {
        match operation {
            TcOperation::Apply {
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
            } => {
                let config = TcNetemConfig::from_legacy_params(
                    *loss,
                    *correlation,
                    *delay_ms,
                    *delay_jitter_ms,
                    *delay_correlation,
                    *duplicate_percent,
                    *duplicate_correlation,
                    *reorder_percent,
                    *reorder_correlation,
                    *reorder_gap,
                    *corrupt_percent,
                    *corrupt_correlation,
                    *rate_limit_kbps,
                );
                config.has_any_enabled().then_some(config)
            }
            TcOperation::Remove | TcOperation::ApplyConfig { .. } => None,
        }
    }

    /// A lowercase Crockford-base32 ULID, safe to use as a key leaf.
    ///
    /// `Ulid::to_string()` is UPPERCASE, and `is_valid_plain_chunk` is
    /// lowercase-only — so an unlowered ULID would be slugged into a
    /// `x_x30__x31_…` string five times the length, unsortable, and still
    /// *valid*, meaning nothing would fail and it would only be noticed in
    /// production. Lowercasing is injective and order-preserving over the
    /// Crockford alphabet, so the leaf stays sortable and byte-identical to the
    /// `ulid` field in the payload.
    fn applied_ulid() -> String {
        ulid::Ulid::new().to_string().to_lowercase()
    }

    /// Emit the immutable audit record for an applied/removed TC config.
    ///
    /// A one-shot `put`, deliberately not a declared publisher: the key leaf is
    /// unique per event, so a publisher map would leak one publisher per apply
    /// for the lifetime of the process.
    async fn publish_applied_event(
        &self,
        namespace: &str,
        interface: &str,
        configuration: Option<TcNetemConfig>,
    ) {
        let ulid = Self::applied_ulid();
        let event = tcgui_shared::TcAppliedEvent {
            ulid: ulid.clone(),
            namespace: namespace.to_string(),
            interface: interface.to_string(),
            configuration,
            // Milliseconds since the epoch, matching ScenarioExecutionUpdate.
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };
        let key = tc::key(&self.local_origin, &tc::Subject::applied(&ulid));
        let payload = match serde_json::to_string(&event) {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to serialize applied event: {e}");
                return;
            }
        };
        if let Err(e) = self
            .session
            .put(key.as_keyexpr(), payload)
            .encoding(zenoh::bytes::Encoding::APPLICATION_JSON)
            .await
        {
            // Audit is best-effort: never fail the actual TC operation because
            // its record could not be published.
            warn!("Failed to publish applied event on {key}: {e}");
        }
    }

    /// Publish the producer registration document (`state/tc/sensor`).
    ///
    /// The namespace list is the instance-to-netns binding RFC 08 §6.1 asks
    /// for, and it carries the **raw** names: this document is the one place a
    /// non-chunk-clean namespace name survives losslessly, since the key
    /// position slugs it.
    pub(crate) async fn publish_sensor_doc(&self) -> Result<()> {
        let mut namespaces: Vec<String> = self
            .interfaces
            .values()
            .map(|i| i.namespace.clone())
            .collect();
        namespaces.push("default".to_string());
        namespaces.sort();
        namespaces.dedup();

        let doc = tcgui_shared::SensorDoc {
            // The producer chunk, not the literal "tc", so a future instance
            // suffix (`tc-2`) shows up here.
            name: tc::producer().chunk().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            namespaces,
        };
        self.sensor_publisher
            .put(serde_json::to_string(&doc)?)
            .encoding(zenoh::bytes::Encoding::APPLICATION_JSON)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to publish sensor doc: {e}"),
            })?;
        Ok(())
    }

    #[instrument(skip(self), fields(backend_name = %self.backend_name, status))]
    pub(crate) async fn send_backend_status(&self, status: &str) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let health_status = BackendHealthStatus {
            host_id: self.local_origin.chunk().to_string(),
            backend_name: self.backend_name.clone(),
            status: status.to_string(),
            timestamp,
            metadata: BackendMetadata::default(),
            namespace_count: 0, // Will be updated by network manager
            interface_count: self.interfaces.len(),
        };

        let payload = serde_json::to_string(&health_status)?;
        let backend_health_topic = tc::key(&self.local_origin, &tc::Subject::Health);
        self.session
            .put(backend_health_topic.as_keyexpr(), payload)
            .encoding(zenoh::bytes::Encoding::APPLICATION_JSON)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to send backend health status: {}", e),
            })?;

        Ok(())
    }

    /// Publish each available preset as its own state document keyed by preset id.
    #[instrument(skip(self), fields(backend_name = %self.backend_name))]
    pub(crate) async fn publish_preset_list(&self) -> Result<()> {
        for preset in self.preset_list.all() {
            let Some(publisher) = self.preset_publishers.get(&preset.id) else {
                warn!("No publisher for preset '{}', skipping", preset.id);
                continue;
            };
            let payload = serde_json::to_string(preset)?;
            publisher
                .put(payload)
                .encoding(zenoh::bytes::Encoding::APPLICATION_JSON)
                .await
                .map_err(|e| TcguiError::ZenohError {
                    message: format!("Failed to publish preset '{}': {}", preset.id, e),
                })?;
        }

        info!(
            "[BACKEND] Published {} preset(s) as state documents",
            self.preset_list.len()
        );
        Ok(())
    }

    /// Get or create a TC configuration publisher for a specific interface
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    pub(crate) async fn get_tc_config_publisher(
        &mut self,
        namespace: &str,
        interface: &str,
    ) -> Result<&AdvancedPublisher<'static>> {
        let key = format!("{}/{}", namespace, interface);

        if !self.tc_config_publishers.contains_key(&key) {
            let tc_config_topic = tc::key(
                &self.local_origin,
                &tc::Subject::config(namespace, interface),
            );
            info!(
                "Creating TC config publisher for {}/{} on: {}",
                namespace,
                interface,
                tc_config_topic.as_str()
            );

            let publisher = self
                .session
                .declare_publisher(zenoh::key_expr::OwnedKeyExpr::from(tc_config_topic))
                .cache(CacheConfig::default().max_samples(1))
                .sample_miss_detection(
                    MissDetectionConfig::default().heartbeat(Duration::from_millis(1000)),
                )
                .publisher_detection()
                .await
                .map_err(|e| TcguiError::ZenohError {
                    message: format!("Failed to declare TC config publisher: {}", e),
                })?;

            self.tc_config_publishers.insert(key.clone(), publisher);
        }

        Ok(self.tc_config_publishers.get(&key).unwrap())
    }

    /// Retract state and drop publishers for interfaces that no longer exist.
    ///
    /// The `state/tc/config/{ns}/{if}` key is LWW, so dropping the publisher
    /// without a Delete leaves the last-written config for a vanished NIC
    /// standing on the state plane forever — a late-joining GUI would show
    /// shaping for an interface that is gone. `network.rs` already tombstones
    /// the interface record itself; this does the same for its config.
    ///
    /// Telemetry publishers need no tombstone (superseded class, no LWW) but
    /// are dropped here too, so they stop leaking one publisher per vanished
    /// interface for the lifetime of the process.
    pub(crate) async fn cleanup_stale_publishers(
        &mut self,
        current_interfaces: &HashMap<u32, NetworkInterface>,
    ) {
        // Build set of valid keys from current interfaces
        let valid_keys: HashSet<String> = current_interfaces
            .values()
            .map(|iface| format!("{}/{}", iface.namespace, iface.name))
            .collect();

        let stale_keys: Vec<String> = self
            .tc_config_publishers
            .keys()
            .filter(|key| !valid_keys.contains(*key))
            .cloned()
            .collect();

        for key in stale_keys {
            info!("Interface {key} is gone — retracting its TC config state");
            if let Some(publisher) = self.tc_config_publishers.remove(&key)
                && let Err(e) = publisher.delete().await
            {
                warn!("Failed to publish TC config tombstone for {key}: {e}");
            }
        }

        let stale_stats: Vec<String> = self
            .tc_stats_publishers
            .keys()
            .filter(|key| !valid_keys.contains(*key))
            .cloned()
            .collect();
        for key in stale_stats {
            debug!("Dropping stale TC statistics publisher for {key}");
            self.tc_stats_publishers.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The events class is rate-budgeted (RFC 04 §1.3, `rate = "low"`, <=1/min).
    /// A scenario stepping every 500ms must never emit an audit record, and a
    /// new `TcOperation` variant must not start emitting one by accident — this
    /// match is exhaustive, so adding a variant fails the build here.
    #[test]
    fn only_operator_driven_operations_are_audited() {
        assert!(TcBackend::is_auditable(&TcOperation::Remove));
        assert!(TcBackend::is_auditable(&TcOperation::Apply {
            loss: 5.0,
            correlation: None,
            delay_ms: None,
            delay_jitter_ms: None,
            delay_correlation: None,
            duplicate_percent: None,
            duplicate_correlation: None,
            reorder_percent: None,
            reorder_correlation: None,
            reorder_gap: None,
            corrupt_percent: None,
            corrupt_correlation: None,
            rate_limit_kbps: None,
        }));
        // The scenario executor's operation — its only constructor is
        // scenario/execution.rs. A 200-step scenario must produce zero events.
        assert!(!TcBackend::is_auditable(&TcOperation::ApplyConfig {
            config: TcNetemConfig::new(),
        }));
    }

    /// `Ulid::to_string()` is UPPERCASE and `is_valid_plain_chunk` is
    /// lowercase-only, so an unlowered ULID would be slugged into `x_x30_…`
    /// soup — five times the length, unsortable, and still *valid*, so nothing
    /// would fail and it would only be noticed in production.
    #[test]
    fn applied_ulid_is_a_legal_key_chunk_verbatim() {
        for _ in 0..32 {
            let id = TcBackend::applied_ulid();
            assert_eq!(id, id.to_lowercase(), "ULID leaked uppercase: {id}");
            assert!(
                zenkey::Chunk::parse(&id).is_ok(),
                "ULID is not a legal plain chunk: {id}"
            );
            // Slugging must be the identity, so the key leaf is byte-identical
            // to the `ulid` field in the payload.
            assert_eq!(
                zenkey::Chunk::slug(&id).to_string(),
                id,
                "ULID would be escaped in the key: {id}"
            );
        }
    }

    /// Lowercasing must stay order-preserving, or audit keys stop sorting by
    /// time — the one property a ULID exists for.
    #[test]
    fn applied_ulids_sort_in_generation_order() {
        let mut previous = TcBackend::applied_ulid();
        for _ in 0..16 {
            std::thread::sleep(std::time::Duration::from_millis(2));
            let next = TcBackend::applied_ulid();
            assert!(next > previous, "{next} did not sort after {previous}");
            previous = next;
        }
    }

    /// An `Apply` with nothing enabled IS a clear — the handler routes it to
    /// `remove_tc_config_in_namespace` — so the audit record must say so.
    #[test]
    fn audited_config_reports_a_clear_as_none() {
        assert!(TcBackend::audited_config(&TcOperation::Remove).is_none());
        let empty = TcOperation::Apply {
            loss: 0.0,
            correlation: None,
            delay_ms: None,
            delay_jitter_ms: None,
            delay_correlation: None,
            duplicate_percent: None,
            duplicate_correlation: None,
            reorder_percent: None,
            reorder_correlation: None,
            reorder_gap: None,
            corrupt_percent: None,
            corrupt_correlation: None,
            rate_limit_kbps: None,
        };
        assert!(TcBackend::audited_config(&empty).is_none());

        let shaped = TcOperation::Apply {
            loss: 5.0,
            correlation: None,
            delay_ms: Some(20.0),
            delay_jitter_ms: None,
            delay_correlation: None,
            duplicate_percent: None,
            duplicate_correlation: None,
            reorder_percent: None,
            reorder_correlation: None,
            reorder_gap: None,
            corrupt_percent: None,
            corrupt_correlation: None,
            rate_limit_kbps: None,
        };
        let config = TcBackend::audited_config(&shaped).expect("shaped apply records its config");
        assert!(config.loss.enabled);
        assert!(config.delay.enabled);
    }
}
