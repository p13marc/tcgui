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
use tracing::{info, instrument, warn};
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
        let payload = query.payload().ok_or_else(|| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "TC query missing payload",
            ))
        })?;
        let payload_bytes = payload.to_bytes();
        if payload_bytes.len() > tcgui_shared::validation::MAX_REQUEST_PAYLOAD_BYTES {
            return self
                .reply_tc_error(
                    &query,
                    format!(
                        "TC request payload too large ({} bytes)",
                        payload_bytes.len()
                    ),
                )
                .await;
        }
        let payload_str = std::str::from_utf8(&payload_bytes).map_err(|e| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid UTF-8: {}", e),
            ))
        })?;

        let request = serde_json::from_str::<TcRequest>(payload_str)?;
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

        let response = match &request.operation {
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

                        TcResponse {
                            success: true,
                            message: format!(
                                "Structured TC config applied successfully to {}:{}",
                                request.namespace, request.interface
                            ),
                            applied_config: Some(applied_config),
                            error_code: None,
                        }
                    }
                    Err(e) => TcResponse {
                        success: false,
                        message: format!("Failed to apply structured TC config: {}", e),
                        applied_config: None,
                        error_code: Some(-1),
                    },
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

                            TcResponse {
                                success: true,
                                message: format!(
                                    "TC applied successfully to {}:{}",
                                    request.namespace, request.interface
                                ),
                                applied_config: Some(applied_config),
                                error_code: None,
                            }
                        } else {
                            // No meaningful parameters - TC qdisc was removed
                            // Publish TC configuration removal (None config)
                            if let Err(e) = self
                                .publish_tc_config(&request.namespace, &request.interface, None)
                                .await
                            {
                                warn!("Failed to publish TC config removal: {}", e);
                            }

                            TcResponse {
                                success: true,
                                message: format!(
                                    "TC removed from {}:{} (no meaningful parameters)",
                                    request.namespace, request.interface
                                ),
                                applied_config: None,
                                error_code: None,
                            }
                        }
                    }
                    Err(e) => TcResponse {
                        success: false,
                        message: format!(
                            "Failed to {} TC: {}",
                            if has_meaningful_params {
                                "apply"
                            } else {
                                "remove"
                            },
                            e
                        ),
                        applied_config: None,
                        error_code: Some(-1),
                    },
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

                        TcResponse {
                            success: true,
                            message: format!(
                                "TC removed successfully from {}:{}",
                                request.namespace, request.interface
                            ),
                            applied_config: None,
                            error_code: None,
                        }
                    }
                    Err(e) => TcResponse {
                        success: false,
                        message: format!("Failed to remove TC: {}", e),
                        applied_config: None,
                        error_code: Some(-1),
                    },
                }
            }
        };

        // Success rides the value channel on our concrete key; failure rides
        // reply_err (RFC 05 §2.1 / §3).
        if response.success {
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
        } else {
            self.reply_query_error(&query, "error/tc/apply", &response.message)
                .await?;
        }

        Ok(())
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    pub(crate) async fn handle_interface_query(
        &mut self,
        query: zenoh::query::Query,
    ) -> Result<()> {
        let payload = query.payload().ok_or_else(|| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Interface query missing payload",
            ))
        })?;
        let payload_bytes = payload.to_bytes();
        let payload_str = std::str::from_utf8(&payload_bytes).map_err(|e| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid UTF-8: {}", e),
            ))
        })?;

        let request = serde_json::from_str::<InterfaceControlRequest>(payload_str)?;
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

        let response = match &request.operation {
            InterfaceControlOperation::Enable => {
                match self
                    .network_manager
                    .enable_interface(&request.namespace, &request.interface)
                    .await
                {
                    Ok(_) => InterfaceControlResponse {
                        success: true,
                        message: format!(
                            "Interface {} enabled successfully in namespace {}",
                            request.interface, request.namespace
                        ),
                        new_state: true,
                        error_code: None,
                    },
                    Err(e) => InterfaceControlResponse {
                        success: false,
                        message: format!("Failed to enable interface: {}", e),
                        new_state: false,
                        error_code: Some(-1),
                    },
                }
            }
            InterfaceControlOperation::Disable => {
                match self
                    .network_manager
                    .disable_interface(&request.namespace, &request.interface)
                    .await
                {
                    Ok(_) => InterfaceControlResponse {
                        success: true,
                        message: format!(
                            "Interface {} disabled successfully in namespace {}",
                            request.interface, request.namespace
                        ),
                        new_state: false,
                        error_code: None,
                    },
                    Err(e) => InterfaceControlResponse {
                        success: false,
                        message: format!("Failed to disable interface: {}", e),
                        new_state: true,
                        error_code: Some(-1),
                    },
                }
            }
        };

        if response.success {
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
        } else {
            self.reply_query_error(&query, "error/interface", &response.message)
                .await?;
        }

        Ok(())
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    pub(crate) async fn handle_diagnostics_query(&self, query: zenoh::query::Query) -> Result<()> {
        use tcgui_shared::{DiagnosticsRequest, DiagnosticsResponse, DiagnosticsResults};

        let payload = query.payload().ok_or_else(|| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Diagnostics query missing payload",
            ))
        })?;
        let payload_bytes = payload.to_bytes();
        let payload_str = std::str::from_utf8(&payload_bytes).map_err(|e| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid UTF-8: {}", e),
            ))
        })?;

        let request = serde_json::from_str::<DiagnosticsRequest>(payload_str)?;
        info!(
            "Received Diagnostics query for {}/{}",
            request.namespace, request.interface
        );

        // Validate the request target before touching the namespace/interface.
        let response = if let Err(reason) =
            tcgui_shared::validation::validate_target(&request.namespace, &request.interface)
        {
            warn!(
                "Rejecting diagnostics request for {}/{}: {}",
                request.namespace, request.interface, reason
            );
            DiagnosticsResponse {
                success: false,
                message: format!("Invalid request: {reason}"),
                results: DiagnosticsResults::default(),
                error_code: Some(22), // EINVAL
            }
        } else {
            // Create diagnostics service and run diagnostics
            let diagnostics_service =
                diagnostics::DiagnosticsService::new(&self.network_manager, &self.tc_manager);

            match diagnostics_service.run_diagnostics(&request).await {
                Ok(result) => result,
                Err(e) => DiagnosticsResponse {
                    success: false,
                    message: format!("Diagnostics failed: {}", e),
                    results: DiagnosticsResults::default(),
                    error_code: Some(-1),
                },
            }
        };

        if response.success {
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
        } else {
            self.reply_query_error(&query, "error/diagnostics", &response.message)
                .await?;
        }

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

    /// Remove publishers for interfaces that no longer exist
    pub(crate) fn cleanup_stale_publishers(
        &mut self,
        current_interfaces: &HashMap<u32, NetworkInterface>,
    ) {
        // Build set of valid keys from current interfaces
        let valid_keys: HashSet<String> = current_interfaces
            .values()
            .map(|iface| format!("{}/{}", iface.namespace, iface.name))
            .collect();

        // Find stale publishers
        let stale_keys: Vec<String> = self
            .tc_config_publishers
            .keys()
            .filter(|key| !valid_keys.contains(*key))
            .cloned()
            .collect();

        // Remove stale publishers
        for key in stale_keys {
            info!("Removing stale TC config publisher for: {}", key);
            self.tc_config_publishers.remove(&key);
        }
    }
}
