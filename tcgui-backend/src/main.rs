mod bandwidth;
pub mod config;
mod interfaces;
mod network;
pub mod scenario;
pub mod services;
mod tc_commands;
mod utils;

#[cfg(test)]
mod tc_commands_test;

use anyhow::Result;
use rtnetlink::new_connection;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{interval, Duration};
use tracing::{error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::{
    errors::{BackendError, TcguiError},
    topics, BackendHealthStatus, BackendMetadata, InterfaceControlOperation,
    InterfaceControlRequest, InterfaceControlResponse, NetworkInterface, TcConfigUpdate,
    TcConfiguration, TcOperation, TcRequest, TcResponse, ZenohConfig,
};

use bandwidth::BandwidthMonitor;
use network::NetworkManager;
use scenario::{ScenarioExecutionHandlers, ScenarioManager, ScenarioZenohHandlers};
use tc_commands::TcCommandManager;

struct TcBackend {
    session: Session,
    interfaces: HashMap<u32, NetworkInterface>,
    _liveliness_token: zenoh::liveliness::LivelinessToken,
    network_manager: NetworkManager,
    bandwidth_monitor: BandwidthMonitor,
    tc_manager: TcCommandManager,
    scenario_manager: Option<std::sync::Arc<ScenarioManager>>,
    scenario_handlers: Option<ScenarioZenohHandlers>,
    execution_handlers: Option<ScenarioExecutionHandlers>,
    exclude_loopback: bool,
    backend_name: String,
    tc_config_publishers: HashMap<String, AdvancedPublisher<'static>>, // namespace/interface -> publisher
}

impl TcBackend {
    #[instrument(skip(zenoh_config, scenario_dirs), fields(backend_name = %backend_name, exclude_loopback))]
    async fn new(
        exclude_loopback: bool,
        backend_name: String,
        zenoh_config: ZenohConfig,
        scenario_dirs: Vec<String>,
        no_default_scenarios: bool,
    ) -> Result<Self> {
        // Initialize Zenoh session
        let config = zenoh_config
            .to_zenoh_config()
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Zenoh configuration error: {}", e),
            })?;

        let session = zenoh::open(config)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to open Zenoh session: {}", e),
            })?;
        info!(
            "[BACKEND] Zenoh session opened with mode: {:?}, endpoints: {:?}",
            zenoh_config.mode, zenoh_config.endpoints
        );

        // Declare liveliness for this specific backend service
        let backend_health_topic = topics::backend_health(&backend_name);
        let liveliness_token = session
            .liveliness()
            .declare_token(&backend_health_topic)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to declare liveliness: {}", e),
            })?;
        info!(
            "[BACKEND] Backend '{}' health liveliness declared on topic: {}",
            backend_name,
            backend_health_topic.as_str()
        );

        // Initialize rtnetlink connection
        let (connection, handle, _messages) =
            new_connection().map_err(|e| BackendError::InitializationError {
                message: format!("Failed to create rtnetlink connection: {}", e),
            })?;
        tokio::spawn(connection);
        info!("[BACKEND] Rtnetlink connection established");

        // Initialize managers with backend name for topic routing
        let network_manager =
            NetworkManager::new(session.clone(), handle, backend_name.clone()).await?;
        let bandwidth_monitor = BandwidthMonitor::new(session.clone(), backend_name.clone());
        let tc_manager = TcCommandManager::new();

        // Initialize scenario management
        let session_arc = std::sync::Arc::new(session.clone());
        let scenario_dirs_paths: Vec<std::path::PathBuf> = scenario_dirs
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect();
        let scenario_manager = std::sync::Arc::new(ScenarioManager::with_options(
            session_arc.clone(),
            backend_name.clone(),
            tc_manager.clone(),
            scenario_dirs_paths,
            no_default_scenarios,
        ));

        // Initialize scenario Zenoh handlers
        let scenario_handlers = ScenarioZenohHandlers::new(
            scenario_manager.clone(),
            session_arc.clone(),
            backend_name.clone(),
        );

        let execution_handlers = ScenarioExecutionHandlers::new(
            scenario_manager.clone(),
            session_arc.clone(),
            backend_name.clone(),
        );

        Ok(Self {
            session,
            interfaces: HashMap::new(),
            _liveliness_token: liveliness_token,
            network_manager,
            bandwidth_monitor,
            tc_manager,
            scenario_manager: Some(scenario_manager),
            scenario_handlers: Some(scenario_handlers),
            execution_handlers: Some(execution_handlers),
            exclude_loopback,
            backend_name,
            tc_config_publishers: HashMap::new(),
        })
    }

    /// Filter interfaces based on configuration
    fn filter_interfaces(
        &self,
        interfaces: HashMap<u32, NetworkInterface>,
    ) -> HashMap<u32, NetworkInterface> {
        if self.exclude_loopback {
            interfaces
                .into_iter()
                .filter(|(_, interface)| {
                    // Filter out loopback interfaces (typically "lo")
                    interface.name != "lo"
                        && !matches!(
                            interface.interface_type,
                            tcgui_shared::InterfaceType::Loopback
                        )
                })
                .collect()
        } else {
            interfaces
        }
    }

    #[instrument(skip(self), fields(backend_name = %self.backend_name))]
    async fn run(&mut self) -> Result<()> {
        info!("[BACKEND] Starting TC backend");

        // Set up TC query handler
        let tc_query_topic = topics::tc_query_service(&self.backend_name);
        let tc_queryable = self
            .session
            .declare_queryable(&tc_query_topic)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to declare TC queryable: {}", e),
            })?;
        info!(
            "[BACKEND] Backend '{}' TC query handler declared on: {}",
            self.backend_name,
            tc_query_topic.as_str()
        );

        // Set up Interface control query handler
        let interface_query_topic = topics::interface_query_service(&self.backend_name);
        let interface_queryable = self
            .session
            .declare_queryable(&interface_query_topic)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to declare Interface queryable: {}", e),
            })?;
        info!(
            "[BACKEND] Backend '{}' Interface query handler declared on: {}",
            self.backend_name,
            interface_query_topic.as_str()
        );

        // Initialize scenario management services
        if let (Some(scenario_handlers), Some(execution_handlers)) = (
            self.scenario_handlers.as_ref(),
            self.execution_handlers.as_ref(),
        ) {
            // Log scenario manager backend name
            if let Some(scenario_manager) = &self.scenario_manager {
                info!(
                    "Scenario manager initialized for backend: {}",
                    scenario_manager.backend_name()
                );
            }

            // Start scenario query handlers
            if let Err(e) = scenario_handlers.start_query_handler().await {
                error!("Failed to start scenario query handler: {}", e);
            } else {
                info!("Scenario query handler started successfully");
            }

            if let Err(e) = execution_handlers.start_query_handler().await {
                error!("Failed to start execution query handler: {}", e);
            } else {
                info!("Execution query handler started successfully");
            }

            // Note: Scenario execution status publisher removed - the execution task
            // already publishes updates at each step transition. The periodic publisher
            // was using stale executor.execution data which caused step counter resets.
        }

        // Send initial backend status
        self.send_backend_status("Backend started").await?;

        // Initial interface discovery across all namespaces
        let discovered_interfaces = self
            .network_manager
            .discover_all_interfaces()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        self.interfaces = self.filter_interfaces(discovered_interfaces);

        // Send interface list (the network manager will organize by namespace)
        self.network_manager
            .send_interface_list(&self.interfaces)
            .await?;

        // Publish initial TC configuration states for all discovered interfaces
        // This ensures frontend gets current state on initial connection
        let interfaces_to_publish: Vec<_> = self
            .interfaces
            .values()
            .map(|i| (i.namespace.clone(), i.name.clone()))
            .collect();
        for (namespace, interface_name) in interfaces_to_publish {
            // Detect existing TC configuration on each interface
            let current_config = self
                .detect_current_tc_config(&namespace, &interface_name)
                .await;
            if let Err(e) = self
                .publish_tc_config(&namespace, &interface_name, current_config)
                .await
            {
                warn!(
                    "Failed to publish initial TC config state for {}:{}: {}",
                    namespace, interface_name, e
                );
            }
        }

        // Create intervals for periodic tasks
        let mut interface_monitor_interval = interval(Duration::from_secs(5));
        let mut bandwidth_monitor_interval = interval(Duration::from_secs(2));

        // Skip the first tick to avoid immediate execution
        interface_monitor_interval.tick().await;
        bandwidth_monitor_interval.tick().await;

        // Main event loop
        loop {
            tokio::select! {
                // Handle TC queries
                query = tc_queryable.recv_async() => {
                    match query {
                        Ok(query) => {
                            if let Err(e) = self.handle_tc_query(query).await {
                                error!("Failed to handle TC query: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error receiving TC query: {}", e);
                        }
                    }
                }

                // Handle Interface control queries
                query = interface_queryable.recv_async() => {
                    match query {
                        Ok(query) => {
                            if let Err(e) = self.handle_interface_query(query).await {
                                error!("Failed to handle Interface query: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Error receiving Interface query: {}", e);
                        }
                    }
                }

                // Periodic interface monitoring
                _ = interface_monitor_interval.tick() => {
                    // Refresh interfaces from all namespaces
                    tracing::info!("[BACKEND] Refreshing interfaces");
                    match self.network_manager.discover_all_interfaces().await {
                        Ok(discovered_interfaces) => {
                            let updated_interfaces = self.filter_interfaces(discovered_interfaces);
                            // Only send updates if interfaces actually changed
                            if self.interfaces != updated_interfaces {
                                tracing::info!("Interface changes detected, sending update to frontend");

                                // Find new interfaces to publish initial TC config states
                                let new_interfaces: Vec<_> = updated_interfaces.values()
                                    .filter(|new_iface| !self.interfaces.contains_key(&new_iface.index))
                                    .map(|i| (i.namespace.clone(), i.name.clone()))
                                    .collect();

                                // Clean up publishers for removed interfaces
                                self.cleanup_stale_publishers(&updated_interfaces);

                                self.interfaces = updated_interfaces;
                                if let Err(e) = self.network_manager.send_interface_list(&self.interfaces).await {
                                    error!("Failed to send updated interface list: {}", e);
                                }

                                // Publish initial TC config states for new interfaces
                                for (namespace, interface_name) in new_interfaces {
                                    let current_config = self.detect_current_tc_config(&namespace, &interface_name).await;
                                    if let Err(e) = self.publish_tc_config(&namespace, &interface_name, current_config).await {
                                        warn!("Failed to publish initial TC config state for new interface {}:{}: {}", namespace, interface_name, e);
                                    }
                                }
                            } else {
                                tracing::debug!("No interface changes detected, skipping update");
                            }
                        },
                        Err(e) => {
                            error!("Failed to refresh interfaces: {}", e);
                        }
                    }
                }

                // Periodic bandwidth monitoring (every 2 seconds)
                _ = bandwidth_monitor_interval.tick() => {
                    tracing::info!("[BACKEND] Monitoring bandwidth");
                    // Monitor bandwidth for all namespaces
                    if let Err(e) = self.bandwidth_monitor.monitor_and_send(&self.interfaces).await {
                        error!("Failed to monitor bandwidth: {}", e);
                    }
                }
            }
        }
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    async fn handle_tc_query(&mut self, query: zenoh::query::Query) -> Result<()> {
        let payload = query.payload().ok_or_else(|| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "TC query missing payload",
            ))
        })?;
        let payload_bytes = payload.to_bytes();
        let payload_str = std::str::from_utf8(&payload_bytes).map_err(|e| {
            TcguiError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid UTF-8: {}", e),
            ))
        })?;

        let request = serde_json::from_str::<TcRequest>(payload_str)?;
        info!("Received TC query: {:?}", request);

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

                        // Build command string for display
                        let mut cmd_parts = vec![format!(
                            "tc qdisc replace dev {} root netem",
                            request.interface
                        )];

                        if loss > 0.0 {
                            let loss_part = if let Some(corr) = correlation {
                                if corr > 0.0 {
                                    format!("loss {}% correlation {}%", loss, corr)
                                } else {
                                    format!("loss {}%", loss)
                                }
                            } else {
                                format!("loss {}%", loss)
                            };
                            cmd_parts.push(loss_part);
                        }

                        if let Some(delay) = delay_ms {
                            if delay > 0.0 {
                                let mut delay_part = format!("delay {}ms", delay);
                                if let Some(jitter) = delay_jitter_ms {
                                    if jitter > 0.0 {
                                        delay_part.push_str(&format!(" {}ms", jitter));
                                        if let Some(delay_corr) = delay_correlation {
                                            if delay_corr > 0.0 {
                                                delay_part.push_str(&format!(" {}%", delay_corr));
                                            }
                                        }
                                    }
                                }
                                cmd_parts.push(delay_part);
                            }
                        }

                        if let Some(duplicate) = duplicate_percent {
                            if duplicate > 0.0 {
                                let mut duplicate_part = format!("duplicate {}%", duplicate);
                                if let Some(dup_corr) = duplicate_correlation {
                                    if dup_corr > 0.0 {
                                        duplicate_part.push_str(&format!(" {}%", dup_corr));
                                    }
                                }
                                cmd_parts.push(duplicate_part);
                            }
                        }

                        if let Some(reorder) = reorder_percent {
                            if reorder > 0.0 {
                                let mut reorder_part = format!("reorder {}%", reorder);
                                if let Some(reorder_corr) = reorder_correlation {
                                    if reorder_corr > 0.0 {
                                        reorder_part.push_str(&format!(" {}%", reorder_corr));
                                    }
                                }
                                if let Some(gap) = reorder_gap {
                                    if gap > 0 {
                                        reorder_part.push_str(&format!(" gap {}", gap));
                                    }
                                }
                                cmd_parts.push(reorder_part);
                            }
                        }

                        if let Some(corrupt) = corrupt_percent {
                            if corrupt > 0.0 {
                                let mut corrupt_part = format!("corrupt {}%", corrupt);
                                if let Some(corrupt_corr) = corrupt_correlation {
                                    if corrupt_corr > 0.0 {
                                        corrupt_part.push_str(&format!(" {}%", corrupt_corr));
                                    }
                                }
                                cmd_parts.push(corrupt_part);
                            }
                        }

                        if let Some(rate) = rate_limit_kbps {
                            if rate > 0 {
                                let rate_part = if rate >= 1000 {
                                    format!("rate {}mbit", rate / 1000)
                                } else {
                                    format!("rate {}kbit", rate)
                                };
                                cmd_parts.push(rate_part);
                            }
                        }

                        let applied_config = TcConfiguration {
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
                            command: cmd_parts.join(" "),
                        };

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
                // Check if any meaningful TC parameters are present
                let has_meaningful_params = *loss > 0.0
                    || delay_ms.is_some_and(|d| d > 0.0)
                    || duplicate_percent.is_some_and(|d| d > 0.0)
                    || reorder_percent.is_some_and(|r| r > 0.0)
                    || corrupt_percent.is_some_and(|c| c > 0.0)
                    || rate_limit_kbps.is_some_and(|r| r > 0);

                let result = if has_meaningful_params {
                    // Apply TC with the meaningful parameters
                    self.tc_manager
                        .apply_tc_config_in_namespace(
                            &request.namespace,
                            &request.interface,
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
                        )
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
                            // Build command string for display
                            let mut cmd_parts = vec![format!(
                                "tc qdisc replace dev {} root netem",
                                request.interface
                            )];

                            if *loss > 0.0 {
                                let loss_part = if let Some(corr) = correlation {
                                    if *corr > 0.0 {
                                        format!("loss {}% correlation {}%", loss, corr)
                                    } else {
                                        format!("loss {}%", loss)
                                    }
                                } else {
                                    format!("loss {}%", loss)
                                };
                                cmd_parts.push(loss_part);
                            }

                            if let Some(delay) = delay_ms {
                                if *delay > 0.0 {
                                    let mut delay_part = format!("delay {}ms", delay);
                                    if let Some(jitter) = delay_jitter_ms {
                                        if *jitter > 0.0 {
                                            delay_part.push_str(&format!(" {}ms", jitter));
                                            if let Some(delay_corr) = delay_correlation {
                                                if *delay_corr > 0.0 {
                                                    delay_part
                                                        .push_str(&format!(" {}%", delay_corr));
                                                }
                                            }
                                        }
                                    }
                                    cmd_parts.push(delay_part);
                                }
                            }

                            if let Some(duplicate) = duplicate_percent {
                                if *duplicate > 0.0 {
                                    let mut duplicate_part = format!("duplicate {}%", duplicate);
                                    if let Some(dup_corr) = duplicate_correlation {
                                        if *dup_corr > 0.0 {
                                            duplicate_part.push_str(&format!(" {}%", dup_corr));
                                        }
                                    }
                                    cmd_parts.push(duplicate_part);
                                }
                            }

                            if let Some(reorder) = reorder_percent {
                                if *reorder > 0.0 {
                                    let mut reorder_part = format!("reorder {}%", reorder);
                                    if let Some(reorder_corr) = reorder_correlation {
                                        if *reorder_corr > 0.0 {
                                            reorder_part.push_str(&format!(" {}%", reorder_corr));
                                        }
                                    }
                                    if let Some(gap) = reorder_gap {
                                        if *gap > 0 {
                                            reorder_part.push_str(&format!(" gap {}", gap));
                                        }
                                    }
                                    cmd_parts.push(reorder_part);
                                }
                            }

                            if let Some(corrupt) = corrupt_percent {
                                if *corrupt > 0.0 {
                                    let mut corrupt_part = format!("corrupt {}%", corrupt);
                                    if let Some(corrupt_corr) = corrupt_correlation {
                                        if *corrupt_corr > 0.0 {
                                            corrupt_part.push_str(&format!(" {}%", corrupt_corr));
                                        }
                                    }
                                    cmd_parts.push(corrupt_part);
                                }
                            }

                            if let Some(rate) = rate_limit_kbps {
                                if *rate > 0 {
                                    let rate_part = if *rate >= 1000 {
                                        format!("rate {}mbit", rate / 1000)
                                    } else {
                                        format!("rate {}kbit", rate)
                                    };
                                    cmd_parts.push(rate_part);
                                }
                            }

                            let applied_config = TcConfiguration {
                                loss: *loss,
                                correlation: *correlation,
                                delay_ms: *delay_ms,
                                delay_jitter_ms: *delay_jitter_ms,
                                delay_correlation: *delay_correlation,
                                duplicate_percent: *duplicate_percent,
                                duplicate_correlation: *duplicate_correlation,
                                reorder_percent: *reorder_percent,
                                reorder_correlation: *reorder_correlation,
                                reorder_gap: *reorder_gap,
                                corrupt_percent: *corrupt_percent,
                                corrupt_correlation: *corrupt_correlation,
                                rate_limit_kbps: *rate_limit_kbps,
                                command: cmd_parts.join(" "),
                            };

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

        // Send response back to query
        let response_payload = serde_json::to_string(&response)?;
        query
            .reply(query.key_expr(), response_payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to reply to TC query: {}", e),
            })?;

        Ok(())
    }

    #[instrument(skip(self, query), fields(backend_name = %self.backend_name))]
    async fn handle_interface_query(&mut self, query: zenoh::query::Query) -> Result<()> {
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

        // Send response back to query
        let response_payload = serde_json::to_string(&response)?;
        query
            .reply(query.key_expr(), response_payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to reply to Interface query: {}", e),
            })?;

        Ok(())
    }

    #[instrument(skip(self), fields(backend_name = %self.backend_name, status))]
    async fn send_backend_status(&self, status: &str) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        let health_status = BackendHealthStatus {
            backend_name: self.backend_name.clone(),
            status: status.to_string(),
            timestamp,
            metadata: BackendMetadata::default(),
            namespace_count: 0, // Will be updated by network manager
            interface_count: self.interfaces.len(),
        };

        let payload = serde_json::to_string(&health_status)?;
        let backend_health_topic = topics::backend_health(&self.backend_name);
        self.session
            .put(&backend_health_topic, payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to send backend health status: {}", e),
            })?;

        Ok(())
    }

    /// Get or create a TC configuration publisher for a specific interface
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    async fn get_tc_config_publisher(
        &mut self,
        namespace: &str,
        interface: &str,
    ) -> Result<&AdvancedPublisher<'static>> {
        let key = format!("{}/{}", namespace, interface);

        if !self.tc_config_publishers.contains_key(&key) {
            let tc_config_topic = topics::tc_config(&self.backend_name, namespace, interface);
            info!(
                "Creating TC config publisher for {}/{} on: {}",
                namespace,
                interface,
                tc_config_topic.as_str()
            );

            let publisher = self
                .session
                .declare_publisher(tc_config_topic)
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
    fn cleanup_stale_publishers(&mut self, current_interfaces: &HashMap<u32, NetworkInterface>) {
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

    /// Parse TC parameters from qdisc info string
    fn parse_tc_parameters(&self, qdisc_info: &str) -> TcConfiguration {
        let mut config = TcConfiguration {
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
            command: format!("# Detected: {}", qdisc_info.trim()),
        };

        // Parse loss percentage
        if let Some(loss_start) = qdisc_info.find("loss ") {
            let loss_part = &qdisc_info[loss_start + 5..];
            if let Some(percent_pos) = loss_part.find('%') {
                if let Ok(loss_val) = loss_part[..percent_pos].trim().parse::<f32>() {
                    config.loss = loss_val;
                    info!("Parsed loss: {}%", loss_val);
                }
            }
        }

        // Parse delay (format: "delay 100ms 10ms 25%" for delay, jitter, correlation)
        // Also handle seconds format: "delay 2.95s"
        if let Some(delay_start) = qdisc_info.find("delay ") {
            let delay_part = &qdisc_info[delay_start + 6..];
            let delay_tokens: Vec<&str> = delay_part.split_whitespace().collect();

            // Parse base delay (first token)
            if !delay_tokens.is_empty() {
                let first_token = delay_tokens[0];

                // Handle milliseconds (ms)
                if first_token.ends_with("ms") {
                    let delay_str = first_token.trim_end_matches("ms");
                    if let Ok(delay_val) = delay_str.parse::<f32>() {
                        config.delay_ms = Some(delay_val);
                        info!("Parsed delay: {}ms", delay_val);
                    }
                }
                // Handle seconds (s) - convert to milliseconds
                else if first_token.ends_with("s") {
                    let delay_str = first_token.trim_end_matches("s");
                    if let Ok(delay_val) = delay_str.parse::<f32>() {
                        let delay_ms = delay_val * 1000.0; // Convert seconds to milliseconds
                        config.delay_ms = Some(delay_ms);
                        info!("Parsed delay: {}s ({}ms)", delay_val, delay_ms);
                    }
                }

                // Parse jitter (second token if it ends with ms)
                if delay_tokens.len() > 1 && delay_tokens[1].ends_with("ms") {
                    let jitter_str = delay_tokens[1].trim_end_matches("ms");
                    if let Ok(jitter_val) = jitter_str.parse::<f32>() {
                        config.delay_jitter_ms = Some(jitter_val);
                        info!("Parsed delay jitter: {}ms", jitter_val);
                    }

                    // Parse correlation (third token if it ends with %)
                    if delay_tokens.len() > 2 && delay_tokens[2].ends_with("%") {
                        let corr_str = delay_tokens[2].trim_end_matches("%");
                        if let Ok(corr_val) = corr_str.parse::<f32>() {
                            config.delay_correlation = Some(corr_val);
                            info!("Parsed delay correlation: {}%", corr_val);
                        }
                    }
                }
            }
        }

        // Parse duplicate percentage
        if let Some(dup_start) = qdisc_info.find("duplicate ") {
            let dup_part = &qdisc_info[dup_start + 10..];
            if let Some(percent_pos) = dup_part.find('%') {
                if let Ok(dup_val) = dup_part[..percent_pos].trim().parse::<f32>() {
                    config.duplicate_percent = Some(dup_val);
                    info!("Parsed duplicate: {}%", dup_val);
                }
            }
        }

        // Parse reorder percentage
        if let Some(reorder_start) = qdisc_info.find("reorder ") {
            let reorder_part = &qdisc_info[reorder_start + 8..];
            if let Some(percent_pos) = reorder_part.find('%') {
                if let Ok(reorder_val) = reorder_part[..percent_pos].trim().parse::<f32>() {
                    config.reorder_percent = Some(reorder_val);
                    info!("Parsed reorder: {}%", reorder_val);
                }
            }

            // Parse reorder gap
            if let Some(gap_start) = qdisc_info.find("gap ") {
                let gap_part = &qdisc_info[gap_start + 4..];
                let gap_end = gap_part.find(' ').unwrap_or(gap_part.len());
                if let Ok(gap_val) = gap_part[..gap_end].trim().parse::<u32>() {
                    config.reorder_gap = Some(gap_val);
                    info!("Parsed reorder gap: {}", gap_val);
                }
            }
        }

        // Parse corrupt percentage
        if let Some(corrupt_start) = qdisc_info.find("corrupt ") {
            let corrupt_part = &qdisc_info[corrupt_start + 8..];
            if let Some(percent_pos) = corrupt_part.find('%') {
                if let Ok(corrupt_val) = corrupt_part[..percent_pos].trim().parse::<f32>() {
                    config.corrupt_percent = Some(corrupt_val);
                    info!("Parsed corrupt: {}%", corrupt_val);
                }
            }
        }

        // Parse rate limiting (can be in kbit, Kbit, mbit, Mbit)
        if let Some(rate_start) = qdisc_info.find("rate ") {
            let rate_part = &qdisc_info[rate_start + 5..];
            let rate_part_lower = rate_part.to_lowercase();

            if let Some(kbit_pos) = rate_part_lower.find("kbit") {
                if let Ok(rate_val) = rate_part[..kbit_pos].trim().parse::<u32>() {
                    config.rate_limit_kbps = Some(rate_val);
                    info!("Parsed rate limit: {}kbps", rate_val);
                }
            } else if let Some(mbit_pos) = rate_part_lower.find("mbit") {
                if let Ok(rate_val) = rate_part[..mbit_pos].trim().parse::<u32>() {
                    config.rate_limit_kbps = Some(rate_val * 1000); // Convert mbit to kbit
                    info!(
                        "Parsed rate limit: {}mbit ({}kbps)",
                        rate_val,
                        rate_val * 1000
                    );
                }
            }
        }

        config
    }

    /// Detect current TC configuration on an interface
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    async fn detect_current_tc_config(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Option<TcConfiguration> {
        // Use tc_manager to check if there's an existing qdisc on the interface
        match self
            .tc_manager
            .check_existing_qdisc(namespace, interface)
            .await
        {
            Ok(qdisc_info) if !qdisc_info.is_empty() => {
                // Check if it's a netem qdisc (which is what we're interested in)
                if qdisc_info.contains("netem") {
                    info!(
                        "Detected existing netem qdisc on {}:{}: {}",
                        namespace,
                        interface,
                        qdisc_info.trim()
                    );

                    // Parse the actual TC parameters from the qdisc output
                    let config = self.parse_tc_parameters(&qdisc_info);
                    Some(config)
                } else {
                    // Non-netem qdisc (e.g., noqueue, mq, etc.) - not a TC configuration
                    None
                }
            }
            Ok(_) => {
                // Empty qdisc info - no qdisc found
                None
            }
            Err(e) => {
                warn!(
                    "Failed to detect TC configuration on {}:{}: {}",
                    namespace, interface, e
                );
                None
            }
        }
    }

    /// Publish current TC configuration for an interface
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    async fn publish_tc_config(
        &mut self,
        namespace: &str,
        interface: &str,
        configuration: Option<TcConfiguration>,
    ) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let backend_name = self.backend_name.clone();
        let publisher = self.get_tc_config_publisher(namespace, interface).await?;

        let tc_update = TcConfigUpdate {
            namespace: namespace.to_string(),
            interface: interface.to_string(),
            backend_name,
            timestamp,
            configuration: configuration.clone(),
            has_tc: configuration.is_some(),
        };

        let payload = serde_json::to_string(&tc_update)?;
        publisher
            .put(payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to publish TC config update: {}", e),
            })?;

        info!(
            "Published TC config update for {}/{}: has_tc={}",
            namespace, interface, tc_update.has_tc
        );
        Ok(())
    }
}

#[tokio::main]
#[instrument]
async fn main() -> Result<()> {
    // Load configuration from CLI and environment
    let config_manager = config::ConfigManager::from_cli_and_env()?;

    // Validate configuration
    config_manager.validate()?;

    // Initialize logging
    config_manager.init_logging()?;

    // Validate zenoh configuration with detailed error reporting
    config::ZenohConfigManager::validate_and_report(&config_manager.zenoh)?;

    info!(
        "[BACKEND] Starting tcgui-backend with name: {}",
        config_manager.app.backend_name
    );
    info!(
        "[BACKEND] Zenoh configuration - Mode: {:?}, Endpoints: {:?}",
        config_manager.zenoh.mode, config_manager.zenoh.endpoints
    );
    if config_manager.app.exclude_loopback {
        info!("[BACKEND] Loopback interface filtering enabled");
    }

    let mut backend = TcBackend::new(
        config_manager.app.exclude_loopback,
        config_manager.app.backend_name.clone(),
        config_manager.zenoh,
        config_manager.app.scenario_dirs.clone(),
        config_manager.app.no_default_scenarios,
    )
    .await?;
    backend.run().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to test parsing without needing full TcBackend
    fn parse_tc_parameters_test(qdisc_info: &str) -> TcConfiguration {
        let mut config = TcConfiguration {
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
            command: format!("# Detected: {}", qdisc_info.trim()),
        };

        // This is the same parsing logic as in TcBackend::parse_tc_parameters

        // Parse loss percentage
        if let Some(loss_start) = qdisc_info.find("loss ") {
            let loss_part = &qdisc_info[loss_start + 5..];
            if let Some(percent_pos) = loss_part.find('%') {
                if let Ok(loss_val) = loss_part[..percent_pos].trim().parse::<f32>() {
                    config.loss = loss_val;
                }
            }
        }

        // Parse delay (format: "delay 100ms 10ms 25%" for delay, jitter, correlation)
        // Also handle seconds format: "delay 2.95s"
        if let Some(delay_start) = qdisc_info.find("delay ") {
            let delay_part = &qdisc_info[delay_start + 6..];
            let delay_tokens: Vec<&str> = delay_part.split_whitespace().collect();

            // Parse base delay (first token)
            if !delay_tokens.is_empty() {
                let first_token = delay_tokens[0];

                // Handle milliseconds (ms)
                if first_token.ends_with("ms") {
                    let delay_str = first_token.trim_end_matches("ms");
                    if let Ok(delay_val) = delay_str.parse::<f32>() {
                        config.delay_ms = Some(delay_val);
                    }
                }
                // Handle seconds (s) - convert to milliseconds
                else if first_token.ends_with("s") {
                    let delay_str = first_token.trim_end_matches("s");
                    if let Ok(delay_val) = delay_str.parse::<f32>() {
                        let delay_ms = delay_val * 1000.0; // Convert seconds to milliseconds
                        config.delay_ms = Some(delay_ms);
                    }
                }

                // Parse jitter (second token if it ends with ms)
                if delay_tokens.len() > 1 && delay_tokens[1].ends_with("ms") {
                    let jitter_str = delay_tokens[1].trim_end_matches("ms");
                    if let Ok(jitter_val) = jitter_str.parse::<f32>() {
                        config.delay_jitter_ms = Some(jitter_val);
                    }

                    // Parse correlation (third token if it ends with %)
                    if delay_tokens.len() > 2 && delay_tokens[2].ends_with("%") {
                        let corr_str = delay_tokens[2].trim_end_matches("%");
                        if let Ok(corr_val) = corr_str.parse::<f32>() {
                            config.delay_correlation = Some(corr_val);
                        }
                    }
                }
            }
        }

        // Parse duplicate percentage
        if let Some(dup_start) = qdisc_info.find("duplicate ") {
            let dup_part = &qdisc_info[dup_start + 10..];
            if let Some(percent_pos) = dup_part.find('%') {
                if let Ok(dup_val) = dup_part[..percent_pos].trim().parse::<f32>() {
                    config.duplicate_percent = Some(dup_val);
                }
            }
        }

        // Parse reorder percentage
        if let Some(reorder_start) = qdisc_info.find("reorder ") {
            let reorder_part = &qdisc_info[reorder_start + 8..];
            if let Some(percent_pos) = reorder_part.find('%') {
                if let Ok(reorder_val) = reorder_part[..percent_pos].trim().parse::<f32>() {
                    config.reorder_percent = Some(reorder_val);
                }
            }

            // Parse reorder gap
            if let Some(gap_start) = qdisc_info.find("gap ") {
                let gap_part = &qdisc_info[gap_start + 4..];
                let gap_end = gap_part.find(' ').unwrap_or(gap_part.len());
                if let Ok(gap_val) = gap_part[..gap_end].trim().parse::<u32>() {
                    config.reorder_gap = Some(gap_val);
                }
            }
        }

        // Parse corrupt percentage
        if let Some(corrupt_start) = qdisc_info.find("corrupt ") {
            let corrupt_part = &qdisc_info[corrupt_start + 8..];
            if let Some(percent_pos) = corrupt_part.find('%') {
                if let Ok(corrupt_val) = corrupt_part[..percent_pos].trim().parse::<f32>() {
                    config.corrupt_percent = Some(corrupt_val);
                }
            }
        }

        // Parse rate limiting (can be in kbit, Kbit, mbit, Mbit)
        if let Some(rate_start) = qdisc_info.find("rate ") {
            let rate_part = &qdisc_info[rate_start + 5..];
            let rate_part_lower = rate_part.to_lowercase();

            if let Some(kbit_pos) = rate_part_lower.find("kbit") {
                if let Ok(rate_val) = rate_part[..kbit_pos].trim().parse::<u32>() {
                    config.rate_limit_kbps = Some(rate_val);
                }
            } else if let Some(mbit_pos) = rate_part_lower.find("mbit") {
                if let Ok(rate_val) = rate_part[..mbit_pos].trim().parse::<u32>() {
                    config.rate_limit_kbps = Some(rate_val * 1000); // Convert mbit to kbit
                }
            }
        }

        config
    }

    #[test]
    fn test_parse_tc_basic_parameters() {
        let qdisc_info = "qdisc netem 802d: root refcnt 2 limit 1000 delay 1ms reorder 25% 50% corrupt 15% rate 100Kbit seed 6860218008241482725 gap 1";

        let config = parse_tc_parameters_test(qdisc_info);

        // Test delay parsing
        assert_eq!(config.delay_ms, Some(1.0), "Should parse delay 1ms");

        // Test reorder parsing
        assert_eq!(
            config.reorder_percent,
            Some(25.0),
            "Should parse reorder 25%"
        );
        assert_eq!(config.reorder_gap, Some(1), "Should parse gap 1");

        // Test corrupt parsing
        assert_eq!(
            config.corrupt_percent,
            Some(15.0),
            "Should parse corrupt 15%"
        );

        // Test rate limit parsing (100Kbit should be parsed as 100 kbps)
        assert_eq!(
            config.rate_limit_kbps,
            Some(100),
            "Should parse rate 100Kbit as 100 kbps"
        );

        // Test parameters that should not be present
        assert_eq!(config.loss, 0.0, "Should not have loss");
        assert_eq!(config.duplicate_percent, None, "Should not have duplicate");
    }

    #[test]
    fn test_parse_tc_loss_and_duplicate() {
        let qdisc_info = "qdisc netem 8030: root refcnt 2 limit 1000 loss 5.5% duplicate 10.2%";

        let config = parse_tc_parameters_test(qdisc_info);

        // Test loss parsing
        assert_eq!(config.loss, 5.5, "Should parse loss 5.5%");

        // Test duplicate parsing
        assert_eq!(
            config.duplicate_percent,
            Some(10.2),
            "Should parse duplicate 10.2%"
        );

        // Test parameters that should not be present
        assert_eq!(config.delay_ms, None, "Should not have delay");
        assert_eq!(config.reorder_percent, None, "Should not have reorder");
        assert_eq!(config.corrupt_percent, None, "Should not have corrupt");
        assert_eq!(config.rate_limit_kbps, None, "Should not have rate limit");
    }

    #[test]
    fn test_parse_tc_complex_delay() {
        let qdisc_info = "qdisc netem 8031: root refcnt 2 limit 1000 delay 100ms 10ms 25%";

        let config = parse_tc_parameters_test(qdisc_info);

        // Test complex delay parsing (delay + jitter + correlation)
        assert_eq!(config.delay_ms, Some(100.0), "Should parse delay 100ms");
        assert_eq!(
            config.delay_jitter_ms,
            Some(10.0),
            "Should parse jitter 10ms"
        );
        assert_eq!(
            config.delay_correlation,
            Some(25.0),
            "Should parse delay correlation 25%"
        );
    }

    #[test]
    fn test_parse_tc_rate_limit_variations() {
        // Test Kbit format
        let qdisc_info_kbit = "qdisc netem 8032: root rate 500Kbit";
        let config_kbit = parse_tc_parameters_test(qdisc_info_kbit);
        assert_eq!(
            config_kbit.rate_limit_kbps,
            Some(500),
            "Should parse rate 500Kbit as 500 kbps"
        );

        // Test Mbit format (should convert to kbps)
        let qdisc_info_mbit = "qdisc netem 8033: root rate 2Mbit";
        let config_mbit = parse_tc_parameters_test(qdisc_info_mbit);
        assert_eq!(
            config_mbit.rate_limit_kbps,
            Some(2000),
            "Should parse rate 2Mbit as 2000 kbps"
        );

        // Test lowercase formats
        let qdisc_info_lower = "qdisc netem 8034: root rate 1000kbit";
        let config_lower = parse_tc_parameters_test(qdisc_info_lower);
        assert_eq!(
            config_lower.rate_limit_kbps,
            Some(1000),
            "Should parse rate 1000kbit as 1000 kbps"
        );
    }

    #[test]
    fn test_parse_tc_reorder_with_correlation() {
        let qdisc_info = "qdisc netem 8035: root reorder 30% 75% gap 5";

        let config = parse_tc_parameters_test(qdisc_info);

        // Test reorder with correlation parsing
        assert_eq!(
            config.reorder_percent,
            Some(30.0),
            "Should parse reorder 30%"
        );
        // Note: The current parsing logic only parses the first percentage after "reorder "
        // The correlation parsing would need additional logic to handle "reorder 30% 75%" format
        assert_eq!(config.reorder_gap, Some(5), "Should parse gap 5");
    }

    #[test]
    fn test_parse_tc_empty_and_invalid() {
        // Test empty string
        let config_empty = parse_tc_parameters_test("");
        assert_eq!(config_empty.loss, 0.0);
        assert_eq!(config_empty.delay_ms, None);
        assert_eq!(config_empty.duplicate_percent, None);

        // Test non-netem qdisc
        let config_noqueue = parse_tc_parameters_test("qdisc noqueue 0: dev lo root refcnt 2");
        assert_eq!(config_noqueue.loss, 0.0);
        assert_eq!(config_noqueue.delay_ms, None);

        // Test malformed parameters
        let config_malformed =
            parse_tc_parameters_test("qdisc netem root delay notanumber corrupt invalid%");
        assert_eq!(
            config_malformed.delay_ms, None,
            "Should not parse invalid delay"
        );
        assert_eq!(
            config_malformed.corrupt_percent, None,
            "Should not parse invalid corrupt"
        );
    }

    #[test]
    fn test_parse_tc_exact_test_case() {
        // Test the exact qdisc output from our live test
        let qdisc_info = "qdisc netem 802d: root refcnt 2 limit 1000 delay 1ms reorder 25% 50% corrupt 15% rate 100Kbit seed 6860218008241482725 gap 1";

        let config = parse_tc_parameters_test(qdisc_info);

        println!("Parsed config: delay_ms={:?}, reorder_percent={:?}, corrupt_percent={:?}, rate_limit_kbps={:?}",
                config.delay_ms, config.reorder_percent, config.corrupt_percent, config.rate_limit_kbps);

        // These are the parameters that should enable checkboxes in frontend
        assert_eq!(config.delay_ms, Some(1.0), "Should parse delay 1ms");
        assert_eq!(
            config.reorder_percent,
            Some(25.0),
            "Should parse reorder 25%"
        );
        assert_eq!(
            config.corrupt_percent,
            Some(15.0),
            "Should parse corrupt 15%"
        );
        assert_eq!(
            config.rate_limit_kbps,
            Some(100),
            "Should parse rate 100Kbit as 100 kbps"
        );
        assert_eq!(config.reorder_gap, Some(1), "Should parse gap 1");

        // Parameters that should not be present
        assert_eq!(config.loss, 0.0, "Should not have loss");
        assert_eq!(config.duplicate_percent, None, "Should not have duplicate");
    }

    #[test]
    fn test_parse_tc_real_namespace_config() {
        // Test the real qdisc output from test-ns namespace
        let qdisc_info = "qdisc netem 802b: root refcnt 9 limit 1000 delay 2.95s loss 49.1% 30.1% duplicate 27.8% reorder 71.8% corrupt 25.3% rate 1Mbit seed 10478122975723631342";

        let config = parse_tc_parameters_test(qdisc_info);

        println!("Real namespace config: loss={}, delay_ms={:?}, duplicate={:?}, reorder={:?}, corrupt={:?}, rate={:?}",
                config.loss, config.delay_ms, config.duplicate_percent, config.reorder_percent, config.corrupt_percent, config.rate_limit_kbps);

        // All these should enable their respective checkboxes
        assert_eq!(config.loss, 49.1, "Should parse loss 49.1%");
        assert_eq!(
            config.delay_ms,
            Some(2950.0),
            "Should parse delay 2.95s as 2950ms"
        );
        assert_eq!(
            config.duplicate_percent,
            Some(27.8),
            "Should parse duplicate 27.8%"
        );
        assert_eq!(
            config.reorder_percent,
            Some(71.8),
            "Should parse reorder 71.8%"
        );
        assert_eq!(
            config.corrupt_percent,
            Some(25.3),
            "Should parse corrupt 25.3%"
        );
        assert_eq!(
            config.rate_limit_kbps,
            Some(1000),
            "Should parse rate 1Mbit as 1000 kbps"
        );
    }
}
