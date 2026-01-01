mod bandwidth;
pub mod config;
mod container;
mod interfaces;
mod namespace_watcher;
mod netlink_events;
mod network;
pub mod preset_loader;
pub mod scenario;
pub mod services;
mod tc_commands;
mod tc_config;
mod utils;

#[cfg(test)]
mod tc_commands_test;

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{Duration, interval};
use tracing::{error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::{
    BackendHealthStatus, BackendMetadata, InterfaceControlOperation, InterfaceControlRequest,
    InterfaceControlResponse, NetworkInterface, TcConfigUpdate, TcConfiguration, TcNetemConfig,
    TcOperation, TcRequest, TcResponse, ZenohConfig, errors::TcguiError, presets::PresetList,
    topics,
};

use bandwidth::BandwidthMonitor;
use namespace_watcher::NamespaceWatcher;
use netlink_events::NetlinkEventListener;
use network::NetworkManager;
use preset_loader::PresetLoader;
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
    _preset_loader: PresetLoader,
    preset_list: PresetList,
    preset_list_publisher: AdvancedPublisher<'static>,
    exclude_loopback: bool,
    backend_name: String,
    tc_config_publishers: HashMap<String, AdvancedPublisher<'static>>, // namespace/interface -> publisher
}

impl TcBackend {
    #[instrument(skip(zenoh_config, scenario_dirs, preset_dirs), fields(backend_name = %backend_name, exclude_loopback))]
    async fn new(
        exclude_loopback: bool,
        backend_name: String,
        zenoh_config: ZenohConfig,
        scenario_dirs: Vec<String>,
        no_default_scenarios: bool,
        preset_dirs: Vec<String>,
        no_default_presets: bool,
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

        // Initialize managers with backend name for topic routing
        // NetworkManager now creates its own nlink connection internally
        let network_manager = NetworkManager::new(session.clone(), backend_name.clone()).await?;
        info!("[BACKEND] Network manager initialized with nlink");

        // Create bandwidth monitor and share container cache for container namespace support
        let mut bandwidth_monitor = BandwidthMonitor::new(session.clone(), backend_name.clone());
        bandwidth_monitor.set_container_cache(network_manager.container_cache());

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

        // Initialize preset loader with configuration
        let mut preset_loader = PresetLoader::with_defaults(!no_default_presets);
        let preset_dirs_paths: Vec<std::path::PathBuf> = preset_dirs
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect();
        preset_loader.add_directories(preset_dirs_paths);
        let (custom_presets, preset_errors) = preset_loader.load_all_with_errors();
        for error in preset_errors {
            warn!("Failed to load preset: {}", error);
        }
        let preset_list = PresetList::new(custom_presets.clone());
        if !custom_presets.is_empty() {
            info!(
                "[BACKEND] Loaded {} custom preset(s): {:?}",
                custom_presets.len(),
                custom_presets.iter().map(|p| &p.id).collect::<Vec<_>>()
            );
        }

        // Create preset list publisher with history cache for late-joining frontends
        let preset_list_topic = topics::preset_list(&backend_name);
        let preset_list_publisher = session
            .declare_publisher(preset_list_topic.clone())
            .cache(CacheConfig::default().max_samples(1))
            .sample_miss_detection(
                MissDetectionConfig::default().heartbeat(Duration::from_millis(2000)),
            )
            .publisher_detection()
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to create preset list publisher: {}", e),
            })?;
        info!(
            "[BACKEND] Created preset list publisher on topic: {}",
            preset_list_topic.as_str()
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
            _preset_loader: preset_loader,
            preset_list,
            preset_list_publisher,
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

        // Publish preset list to frontend
        self.publish_preset_list().await?;

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

        // Start netlink event listener for real-time interface change detection
        // This replaces frequent polling for the default namespace
        let (netlink_listener, mut netlink_events) = NetlinkEventListener::new(100);
        if let Err(e) = netlink_listener.start().await {
            warn!(
                "Failed to start netlink event listener: {}. Falling back to polling only.",
                e
            );
        }

        // Start inotify watcher for /var/run/netns to detect namespace changes immediately
        // The watcher must be kept alive for the entire duration of the event loop
        let (_namespace_watcher, mut namespace_events) = match NamespaceWatcher::new(100) {
            Some((watcher, rx)) => {
                info!("Namespace watcher started for /var/run/netns");
                (Some(watcher), Some(rx))
            }
            None => {
                warn!("Namespace watcher not available, using polling fallback");
                (None, None)
            }
        };

        // Create intervals for periodic tasks
        // Namespace polling interval increased to 60s since inotify handles immediate detection
        let mut namespace_monitor_interval = interval(Duration::from_secs(60));
        let mut bandwidth_monitor_interval = interval(Duration::from_secs(2));

        // Skip the first tick to avoid immediate execution
        namespace_monitor_interval.tick().await;
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

                // Handle real-time netlink events (default namespace only)
                Some(event) = netlink_events.recv() => {
                    tracing::debug!("Received netlink event: {:?}", event);
                    // Trigger a full interface refresh on any link change
                    // This ensures we capture all details including TC qdisc state
                    match self.network_manager.discover_all_interfaces().await {
                        Ok(discovered_interfaces) => {
                            let updated_interfaces = self.filter_interfaces(discovered_interfaces);
                            if self.interfaces != updated_interfaces {
                                tracing::info!("Netlink event triggered interface update");

                                let new_interfaces: Vec<_> = updated_interfaces.values()
                                    .filter(|new_iface| !self.interfaces.contains_key(&new_iface.index))
                                    .map(|i| (i.namespace.clone(), i.name.clone()))
                                    .collect();

                                self.cleanup_stale_publishers(&updated_interfaces);
                                self.interfaces = updated_interfaces;

                                if let Err(e) = self.network_manager.send_interface_list(&self.interfaces).await {
                                    error!("Failed to send updated interface list: {}", e);
                                }

                                for (namespace, interface_name) in new_interfaces {
                                    let current_config = self.detect_current_tc_config(&namespace, &interface_name).await;
                                    if let Err(e) = self.publish_tc_config(&namespace, &interface_name, current_config).await {
                                        warn!("Failed to publish TC config for new interface {}:{}: {}", namespace, interface_name, e);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            error!("Failed to refresh interfaces after netlink event: {}", e);
                        }
                    }
                }

                // Inotify-based namespace change detection (immediate)
                Some(ns_event) = async {
                    match &mut namespace_events {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match &ns_event {
                        namespace_watcher::NamespaceEvent::Created(name) => {
                            info!("Namespace created: {}", name);
                            // Small delay to let the namespace setup complete
                            // The inotify event fires before the bind mount is fully ready
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                        namespace_watcher::NamespaceEvent::Deleted(name) => {
                            info!("Namespace deleted: {}", name);
                        }
                    }
                    // Trigger a full interface refresh on namespace change
                    match self.network_manager.discover_all_interfaces().await {
                        Ok(discovered_interfaces) => {
                            let updated_interfaces = self.filter_interfaces(discovered_interfaces);
                            if self.interfaces != updated_interfaces {
                                info!("Namespace event triggered interface update");

                                let new_interfaces: Vec<_> = updated_interfaces.values()
                                    .filter(|new_iface| !self.interfaces.contains_key(&new_iface.index))
                                    .map(|i| (i.namespace.clone(), i.name.clone()))
                                    .collect();

                                self.cleanup_stale_publishers(&updated_interfaces);
                                self.interfaces = updated_interfaces;

                                if let Err(e) = self.network_manager.send_interface_list(&self.interfaces).await {
                                    error!("Failed to send updated interface list: {}", e);
                                }

                                for (namespace, interface_name) in new_interfaces {
                                    let current_config = self.detect_current_tc_config(&namespace, &interface_name).await;
                                    if let Err(e) = self.publish_tc_config(&namespace, &interface_name, current_config).await {
                                        warn!("Failed to publish TC config for new interface {}:{}: {}", namespace, interface_name, e);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            error!("Failed to refresh interfaces after namespace event: {}", e);
                        }
                    }
                }

                // Periodic namespace monitoring (fallback when inotify is not available)
                _ = namespace_monitor_interval.tick() => {
                    tracing::debug!("[BACKEND] Periodic namespace check");
                    match self.network_manager.discover_all_interfaces().await {
                        Ok(discovered_interfaces) => {
                            let updated_interfaces = self.filter_interfaces(discovered_interfaces);
                            if self.interfaces != updated_interfaces {
                                tracing::info!("Namespace poll detected interface changes");

                                let new_interfaces: Vec<_> = updated_interfaces.values()
                                    .filter(|new_iface| !self.interfaces.contains_key(&new_iface.index))
                                    .map(|i| (i.namespace.clone(), i.name.clone()))
                                    .collect();

                                self.cleanup_stale_publishers(&updated_interfaces);
                                self.interfaces = updated_interfaces;

                                if let Err(e) = self.network_manager.send_interface_list(&self.interfaces).await {
                                    error!("Failed to send updated interface list: {}", e);
                                }

                                for (namespace, interface_name) in new_interfaces {
                                    let current_config = self.detect_current_tc_config(&namespace, &interface_name).await;
                                    if let Err(e) = self.publish_tc_config(&namespace, &interface_name, current_config).await {
                                        warn!("Failed to publish TC config for new interface {}:{}: {}", namespace, interface_name, e);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            error!("Failed to refresh interfaces: {}", e);
                        }
                    }
                }

                // Periodic bandwidth monitoring (every 2 seconds)
                _ = bandwidth_monitor_interval.tick() => {
                    tracing::debug!("[BACKEND] Monitoring bandwidth");
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

    /// Publish the preset list to the frontend
    #[instrument(skip(self), fields(backend_name = %self.backend_name))]
    async fn publish_preset_list(&self) -> Result<()> {
        let payload = serde_json::to_string(&self.preset_list)?;
        self.preset_list_publisher
            .put(payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to publish preset list: {}", e),
            })?;

        info!(
            "[BACKEND] Published preset list with {} presets",
            self.preset_list.len()
        );
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
        tc_config::parse_tc_parameters(qdisc_info)
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
        config_manager.app.preset_dirs.clone(),
        config_manager.app.no_default_presets,
    )
    .await?;
    backend.run().await?;

    Ok(())
}

// Note: TC configuration parsing tests are in tc_config module
