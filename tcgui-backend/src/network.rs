//! Network interface management and monitoring for multiple namespaces.
//!
//! This module provides comprehensive network interface discovery, monitoring,
//! and management across multiple Linux network namespaces. It uses nlink
//! for efficient kernel communication with native namespace support.
//!
//! # Key Features
//!
//! * **Multi-namespace support**: Discovers interfaces in default and named namespaces
//! * **Real-time monitoring**: Detects interface additions, removals, and state changes
//! * **Traffic control detection**: Identifies interfaces with active TC qdisc configurations
//! * **Interface type classification**: Categorizes Physical, Virtual, Veth, Bridge, etc.
//! * **Robust error handling**: Graceful handling of namespace access permissions

use anyhow::Result;
use nlink::netlink::{Connection, Protocol, namespace};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::{
    InterfaceEventType, InterfaceListUpdate, InterfaceStateEvent, InterfaceType, NamespaceType,
    NetworkInterface, NetworkNamespace,
    errors::{BackendError, TcguiError},
    topics,
};

use crate::container::{Container, ContainerManager};

/// Network interface manager for multi-namespace operations.
///
/// This struct provides comprehensive network interface management across
/// multiple Linux network namespaces. It uses nlink for efficient netlink
/// operations with native namespace support.
///
/// # Architecture
///
/// * **Default namespace**: Uses nlink Connection for direct kernel communication
/// * **Named namespaces**: Uses nlink namespace API for namespace-isolated operations
/// * **Container namespaces**: Uses nlink with PID-based namespace paths
/// * **Interface tracking**: Maintains per-namespace interface maps for change detection
/// * **TC detection**: Checks for active traffic control configurations
///
/// # Usage
///
/// ```rust,no_run
/// use tcgui_backend::network::NetworkManager;
/// use zenoh::Session;
///
/// async fn setup_manager(session: Session) -> anyhow::Result<()> {
///     let manager = NetworkManager::new(session, "test-backend".to_string()).await?;
///     let interfaces = manager.discover_all_interfaces().await?;
///     Ok(())
/// }
/// ```
pub struct NetworkManager {
    /// nlink connection for default namespace operations
    connection: Connection,
    /// Track interfaces per namespace for change detection (future namespace monitoring)
    /// Map: namespace_name -> (interface_index -> NetworkInterface)
    #[allow(dead_code)]
    namespace_interfaces: HashMap<String, HashMap<u32, NetworkInterface>>,
    /// Backend name for topic routing in multi-backend scenarios
    backend_name: String,
    /// Publisher for interface list updates
    interface_list_publisher: AdvancedPublisher<'static>,
    /// Publisher for interface state events
    interface_events_publisher: AdvancedPublisher<'static>,
    /// Container runtime manager for Docker/Podman discovery
    container_manager: ContainerManager,
    /// Cache of last discovered containers for namespace type lookup
    /// Key is "container:<name>" to match namespace naming
    cached_containers: std::sync::Arc<tokio::sync::RwLock<HashMap<String, Container>>>,
}

impl NetworkManager {
    /// Creates a new NetworkManager instance.
    ///
    /// # Arguments
    ///
    /// * `session` - Zenoh session for sending interface updates and responses
    /// * `backend_name` - Unique name for this backend instance for topic routing
    ///
    /// # Returns
    ///
    /// A new `NetworkManager` ready to discover and monitor network interfaces
    /// across multiple namespaces with backend-specific topic routing.
    #[instrument(skip(session), fields(backend_name = %backend_name))]
    pub async fn new(session: Session, backend_name: String) -> Result<Self, TcguiError> {
        // Create nlink connection for default namespace
        let connection =
            Connection::new(Protocol::Route).map_err(|e| TcguiError::NetworkError {
                message: format!("Failed to create nlink connection: {}", e),
            })?;
        info!("[BACKEND] nlink connection established for default namespace");

        // Declare advanced publishers for interface communications with history
        let interface_list_topic = topics::interface_list(&backend_name);
        info!(
            "Declaring interface list advanced publisher with history on: {}",
            interface_list_topic.as_str()
        );
        let interface_list_publisher = session
            .declare_publisher(interface_list_topic)
            .cache(CacheConfig::default().max_samples(1))
            .sample_miss_detection(
                MissDetectionConfig::default().heartbeat(Duration::from_millis(500)),
            )
            .publisher_detection()
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to declare interface list advanced publisher: {}", e),
            })?;

        let interface_events_topic = topics::interface_events(&backend_name);
        info!(
            "Declaring interface events advanced publisher with history on: {}",
            interface_events_topic.as_str()
        );
        let interface_events_publisher = session
            .declare_publisher(interface_events_topic)
            .cache(CacheConfig::default().max_samples(1))
            .sample_miss_detection(
                MissDetectionConfig::default().heartbeat(Duration::from_millis(500)),
            )
            .publisher_detection()
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!(
                    "Failed to declare interface events advanced publisher: {}",
                    e
                ),
            })?;

        // Initialize container manager for Docker/Podman discovery
        let container_manager = ContainerManager::new().await;
        if container_manager.is_available() {
            info!(
                "Container runtimes available: {:?}",
                container_manager.available_runtimes()
            );
        } else {
            info!("No container runtimes detected");
        }

        let cached_containers = std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new()));

        Ok(Self {
            connection,
            namespace_interfaces: HashMap::new(),
            backend_name,
            interface_list_publisher,
            interface_events_publisher,
            container_manager,
            cached_containers,
        })
    }

    /// Returns a reference to the container cache for sharing with other components.
    ///
    /// This allows components like BandwidthMonitor to access container namespace paths
    /// for executing commands inside container network namespaces.
    pub fn container_cache(
        &self,
    ) -> std::sync::Arc<tokio::sync::RwLock<HashMap<String, Container>>> {
        self.cached_containers.clone()
    }

    /// Discovers network interfaces within a specific namespace.
    ///
    /// This method handles both the default namespace and named namespaces
    /// using nlink's native namespace support.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The target namespace name ("default" for host namespace)
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap<u32, NetworkInterface>)` - Map of interface index to interface data
    /// * `Err(BackendError)` - On namespace access failures or system errors
    pub async fn discover_interfaces_in_namespace(
        &self,
        namespace: &str,
    ) -> Result<HashMap<u32, NetworkInterface>, BackendError> {
        info!("Discovering network interfaces in namespace: {}", namespace);

        let mut discovered_interfaces = HashMap::new();

        // Get the appropriate connection for this namespace
        let conn = if namespace == "default" {
            // Use the existing connection for default namespace
            &self.connection
        } else {
            // For named namespaces, we need to create a temporary connection
            // Since we can't store it, we'll handle this differently
            return self.discover_interfaces_in_named_namespace(namespace).await;
        };

        // Query interfaces using nlink
        let links = conn
            .get_links()
            .await
            .map_err(|e| BackendError::NetworkError {
                message: format!("Failed to get links: {}", e),
            })?;

        for link in links {
            let index = link.ifindex();
            let name = link.name_or(&format!("unknown{}", index)).to_string();
            let is_up = link.is_up();

            // Determine interface type
            let interface_type = Self::determine_interface_type(&name, &link);

            // Check if interface has TC qdisc
            let has_tc_qdisc = self
                .check_tc_qdisc_with_connection(conn, &name)
                .await
                .unwrap_or(false);

            discovered_interfaces.insert(
                index,
                NetworkInterface {
                    name,
                    index,
                    namespace: namespace.to_string(),
                    is_up,
                    has_tc_qdisc,
                    interface_type,
                },
            );
        }

        Ok(discovered_interfaces)
    }

    /// Discover interfaces in a named namespace using nlink namespace API.
    async fn discover_interfaces_in_named_namespace(
        &self,
        namespace: &str,
    ) -> Result<HashMap<u32, NetworkInterface>, BackendError> {
        // Create a connection in the target namespace
        let conn =
            namespace::connection_for(namespace).map_err(|e| BackendError::NetworkError {
                message: format!("Failed to connect to namespace {}: {}", namespace, e),
            })?;

        let links = conn
            .get_links()
            .await
            .map_err(|e| BackendError::NetworkError {
                message: format!("Failed to get links in namespace {}: {}", namespace, e),
            })?;

        let mut interfaces = HashMap::new();

        for link in links {
            let index = link.ifindex();
            let name = link.name_or(&format!("unknown{}", index)).to_string();
            let is_up = link.is_up();

            // Determine interface type
            let interface_type = Self::determine_interface_type(&name, &link);

            // Check TC qdisc in this namespace
            let has_tc_qdisc = self
                .check_tc_qdisc_with_connection(&conn, &name)
                .await
                .unwrap_or(false);

            interfaces.insert(
                index,
                NetworkInterface {
                    name,
                    index,
                    namespace: namespace.to_string(),
                    is_up,
                    has_tc_qdisc,
                    interface_type,
                },
            );
        }

        Ok(interfaces)
    }

    /// Determine interface type from name and link message
    fn determine_interface_type(
        name: &str,
        link: &nlink::netlink::messages::LinkMessage,
    ) -> InterfaceType {
        // Check for loopback
        if link.is_loopback() {
            return InterfaceType::Loopback;
        }

        // Check link kind from the message
        if let Some(kind) = link.kind() {
            return match kind {
                "bridge" => InterfaceType::Bridge,
                "veth" => InterfaceType::Veth,
                "tun" | "tap" => InterfaceType::Tun,
                "vlan" => InterfaceType::Virtual,
                "bond" => InterfaceType::Virtual,
                "dummy" => InterfaceType::Virtual,
                _ => InterfaceType::Physical,
            };
        }

        // Fallback to name-based detection
        if name.starts_with("br-") || name == "docker0" {
            InterfaceType::Bridge
        } else if name.starts_with("veth") {
            InterfaceType::Veth
        } else if name.starts_with("tun") || name.starts_with("tap") {
            InterfaceType::Tun
        } else if name == "lo" {
            InterfaceType::Loopback
        } else {
            InterfaceType::Physical
        }
    }

    /// Check TC qdisc using a connection
    async fn check_tc_qdisc_with_connection(
        &self,
        conn: &Connection,
        interface: &str,
    ) -> Result<bool> {
        let qdiscs = conn
            .get_qdiscs_for(interface)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get qdiscs for {}: {}", interface, e))?;

        // Check if any of the qdiscs is a netem qdisc
        for qdisc in qdiscs {
            if let Some(kind) = qdisc.kind()
                && kind == "netem"
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check if an interface has a netem qdisc configured.
    ///
    /// This is a public method that handles namespace resolution for checking
    /// TC configuration on any interface.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The namespace containing the interface
    /// * `interface` - The interface name to check
    ///
    /// # Returns
    ///
    /// `true` if the interface has a netem qdisc, `false` otherwise
    pub async fn check_interface_has_netem(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<bool> {
        if namespace == "default" {
            self.check_tc_qdisc_with_connection(&self.connection, interface)
                .await
        } else if namespace.starts_with("container:") {
            // Container namespace - get the container and use its namespace path
            let cache = self.cached_containers.read().await;
            if let Some(container) = cache.get(namespace) {
                let ns_path = container
                    .namespace_path
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Container has no namespace path"))?;
                let conn =
                    Connection::new_in_namespace_path(Protocol::Route, ns_path).map_err(|e| {
                        anyhow::anyhow!("Failed to connect to container namespace: {}", e)
                    })?;
                self.check_tc_qdisc_with_connection(&conn, interface).await
            } else {
                Ok(false) // Container not found, assume no netem
            }
        } else {
            // Named namespace
            let conn = namespace::connection_for(namespace)
                .map_err(|e| anyhow::anyhow!("Failed to connect to namespace: {}", e))?;
            self.check_tc_qdisc_with_connection(&conn, interface).await
        }
    }

    /// Monitor interfaces across all namespaces (future multi-namespace monitoring)
    #[allow(dead_code)]
    pub async fn monitor_all_namespaces(&mut self, namespaces: &[NetworkNamespace]) -> Result<()> {
        for namespace in namespaces {
            if let Err(e) = self.monitor_namespace_interfaces(&namespace.name).await {
                error!(
                    "Failed to monitor interfaces in namespace {}: {}",
                    namespace.name, e
                );
            }
        }
        Ok(())
    }

    /// Monitor interfaces in a specific namespace (future namespace-specific monitoring)
    #[allow(dead_code)]
    pub async fn monitor_namespace_interfaces(&mut self, namespace: &str) -> Result<()> {
        match self.discover_interfaces_in_namespace(namespace).await {
            Ok(discovered_interfaces) => {
                let current_interfaces = self
                    .namespace_interfaces
                    .entry(namespace.to_string())
                    .or_default();

                // Check for new interfaces
                let mut updates_to_send = Vec::new();
                for (index, interface) in &discovered_interfaces {
                    if !current_interfaces.contains_key(index) {
                        updates_to_send.push((interface.clone(), InterfaceEventType::Added));
                    }
                }

                // Check for removed interfaces
                for (index, interface) in current_interfaces.iter() {
                    if !discovered_interfaces.contains_key(index) {
                        updates_to_send.push((interface.clone(), InterfaceEventType::Removed));
                    }
                }

                *current_interfaces = discovered_interfaces;

                // Send updates after we're done with the mutable borrow
                for (interface, event_type) in updates_to_send {
                    self.send_interface_update(namespace, interface, event_type)
                        .await?;
                }

                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to monitor interfaces in namespace {}: {}",
                    namespace, e
                );
                Err(e.into())
            }
        }
    }

    /// Tests if a network namespace is accessible to the current process.
    async fn is_namespace_accessible(&self, namespace: &str) -> bool {
        if namespace == "default" {
            return true;
        }

        // Try to create a connection in the namespace
        match namespace::connection_for(namespace) {
            Ok(conn) => {
                // Try to query interfaces to verify it works
                match conn.get_links().await {
                    Ok(_) => {
                        debug!("Namespace '{}' is accessible", namespace);
                        true
                    }
                    Err(e) => {
                        debug!("Namespace '{}' query failed: {}", namespace, e);
                        false
                    }
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("EPERM") || err_str.contains("Operation not permitted") {
                    debug!(
                        "Namespace '{}' is not accessible due to permissions",
                        namespace
                    );
                } else if err_str.contains("No such file") {
                    debug!("Namespace '{}' not found", namespace);
                } else {
                    warn!(
                        "Failed to test accessibility of namespace '{}': {}",
                        namespace, e
                    );
                }
                false
            }
        }
    }

    /// Discovers accessible network namespaces on the system.
    pub async fn discover_all_namespaces(&self) -> Result<Vec<String>> {
        info!("Discovering accessible network namespaces");

        let mut accessible_namespaces = vec!["default".to_string()];

        // Use nlink's namespace::list() to discover named namespaces
        let discovered_namespaces = namespace::list().unwrap_or_default();

        info!(
            "Found {} namespaces in /var/run/netns: {:?}",
            discovered_namespaces.len(),
            discovered_namespaces
        );

        // Test accessibility for each discovered namespace
        for ns_name in discovered_namespaces {
            if accessible_namespaces.contains(&ns_name) {
                continue;
            }

            info!("Testing accessibility of namespace '{}'", ns_name);
            if self.is_namespace_accessible(&ns_name).await {
                info!("Namespace '{}' is accessible", ns_name);
                accessible_namespaces.push(ns_name);
            } else {
                warn!("Namespace '{}' is not accessible", ns_name);
            }
        }

        info!(
            "Discovered {} accessible namespaces: {:?}",
            accessible_namespaces.len(),
            accessible_namespaces
        );
        Ok(accessible_namespaces)
    }

    /// Discovers running containers and returns them with their network namespaces.
    pub async fn discover_containers(&self) -> Vec<Container> {
        if !self.container_manager.is_available() {
            return Vec::new();
        }

        match self.container_manager.discover_containers().await {
            Ok(containers) => {
                info!("Discovered {} running containers", containers.len());
                containers
            }
            Err(e) => {
                warn!("Failed to discover containers: {}", e);
                Vec::new()
            }
        }
    }

    /// Discovers interfaces inside a container's network namespace.
    ///
    /// # Arguments
    ///
    /// * `container` - The container to discover interfaces in
    ///
    /// # Returns
    ///
    /// A map of interface index to NetworkInterface for interfaces inside the container
    pub async fn discover_interfaces_in_container(
        &self,
        container: &Container,
    ) -> Result<HashMap<u32, NetworkInterface>, BackendError> {
        let namespace_name = format!("container:{}", container.name);
        info!(
            "Discovering interfaces in container {} ({})",
            container.name, container.short_id
        );

        // Get the namespace path for this container
        let ns_path =
            container
                .namespace_path
                .as_ref()
                .ok_or_else(|| BackendError::NetworkError {
                    message: format!("Container {} has no namespace path", container.name),
                })?;

        // Create a connection in the container's namespace
        let conn = Connection::new_in_namespace_path(Protocol::Route, ns_path).map_err(|e| {
            BackendError::NetworkError {
                message: format!(
                    "Failed to connect to container {} namespace: {}",
                    container.name, e
                ),
            }
        })?;

        // Query interfaces
        let links = conn
            .get_links()
            .await
            .map_err(|e| BackendError::NetworkError {
                message: format!("Failed to get links in container {}: {}", container.name, e),
            })?;

        let mut interfaces = HashMap::new();

        for link in links {
            let index = link.ifindex();
            let name = link.name_or(&format!("eth{}", index)).to_string();
            let is_up = link.is_up();

            // Determine interface type
            let interface_type = if link.is_loopback() {
                InterfaceType::Loopback
            } else if name.starts_with("eth") || name.starts_with("veth") {
                InterfaceType::Veth
            } else {
                InterfaceType::Virtual
            };

            // Check TC qdisc
            let has_tc_qdisc = self
                .check_tc_qdisc_with_connection(&conn, &name)
                .await
                .unwrap_or(false);

            interfaces.insert(
                index,
                NetworkInterface {
                    name,
                    index,
                    namespace: namespace_name.clone(),
                    is_up,
                    has_tc_qdisc,
                    interface_type,
                },
            );
        }

        Ok(interfaces)
    }

    /// Discovers network interfaces across all available namespaces.
    #[instrument(skip(self), fields(backend_name = %self.backend_name))]
    pub async fn discover_all_interfaces(
        &self,
    ) -> Result<HashMap<u32, NetworkInterface>, BackendError> {
        let namespaces =
            self.discover_all_namespaces()
                .await
                .map_err(|e| BackendError::NetworkError {
                    message: format!("Failed to discover namespaces: {}", e),
                })?;
        let mut all_interfaces = HashMap::new();

        let mut namespace_id = 0u32;
        for namespace in namespaces {
            match self.discover_interfaces_in_namespace(&namespace).await {
                Ok(interfaces) => {
                    info!(
                        "Found {} interfaces in namespace '{}'",
                        interfaces.len(),
                        namespace
                    );
                    for (index, interface) in interfaces {
                        // Use a composite key to avoid index conflicts between namespaces
                        let composite_key = index + (namespace_id * 1000000);
                        all_interfaces.insert(composite_key, interface);
                    }
                    namespace_id += 1;
                }
                Err(e) => {
                    error!(
                        "Failed to discover interfaces in namespace {}: {}",
                        namespace, e
                    );
                }
            }
        }

        // Also discover container interfaces
        let containers = self.discover_containers().await;

        // Update the container cache
        {
            let mut cache = self.cached_containers.write().await;
            cache.clear();
            for container in &containers {
                let key = format!("container:{}", container.name);
                cache.insert(key, container.clone());
            }
        }

        for container in &containers {
            match self.discover_interfaces_in_container(container).await {
                Ok(interfaces) => {
                    for (index, interface) in interfaces {
                        let composite_key = index + (interface.namespace.len() as u32 * 1000000);
                        all_interfaces.insert(composite_key, interface);
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to discover interfaces in container {}: {}",
                        container.name, e
                    );
                }
            }
        }

        info!(
            "Discovered {} interfaces across all namespaces and containers",
            all_interfaces.len()
        );
        Ok(all_interfaces)
    }

    /// Sends interface list to frontend organized by namespaces.
    #[instrument(skip(self, interfaces), fields(backend_name = %self.backend_name, interface_count = interfaces.len()))]
    pub async fn send_interface_list(
        &self,
        interfaces: &HashMap<u32, NetworkInterface>,
    ) -> Result<()> {
        // Group interfaces by namespace
        let mut namespace_map: HashMap<String, Vec<NetworkInterface>> = HashMap::new();

        for interface in interfaces.values() {
            namespace_map
                .entry(interface.namespace.clone())
                .or_default()
                .push(interface.clone());
        }

        // Get the container cache for namespace type lookup
        let container_cache = self.cached_containers.read().await;

        // Convert to NetworkNamespace structs with proper namespace types
        let namespaces: Vec<NetworkNamespace> = namespace_map
            .into_iter()
            .map(|(name, interfaces)| {
                let namespace_type = if name == "default" {
                    NamespaceType::Default
                } else if name.starts_with("container:") {
                    if let Some(container) = container_cache.get(&name) {
                        NamespaceType::Container {
                            runtime: format!("{:?}", container.runtime),
                            container_id: container.short_id.clone(),
                            image: container.image.clone(),
                        }
                    } else {
                        NamespaceType::Container {
                            runtime: "unknown".to_string(),
                            container_id: name
                                .strip_prefix("container:")
                                .unwrap_or(&name)
                                .to_string(),
                            image: "unknown".to_string(),
                        }
                    }
                } else {
                    NamespaceType::Traditional
                };

                NetworkNamespace {
                    name: name.clone(),
                    id: None,
                    is_active: true,
                    namespace_type,
                    interfaces,
                }
            })
            .collect();

        let interface_list_update = InterfaceListUpdate {
            namespaces,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            backend_name: self.backend_name.clone(),
        };

        let payload = serde_json::to_string(&interface_list_update)
            .map_err(TcguiError::SerializationError)?;

        self.interface_list_publisher
            .put(payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to send interface list: {}", e),
            })?;

        info!(
            "Sent interface list with {} namespaces via publisher",
            interface_list_update.namespaces.len()
        );

        Ok(())
    }

    async fn send_interface_update(
        &self,
        namespace: &str,
        interface: NetworkInterface,
        event_type: InterfaceEventType,
    ) -> Result<()> {
        let interface_event = InterfaceStateEvent {
            namespace: namespace.to_string(),
            interface,
            event_type,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
            backend_name: self.backend_name.clone(),
        };

        let payload =
            serde_json::to_string(&interface_event).map_err(TcguiError::SerializationError)?;

        self.interface_events_publisher
            .put(payload)
            .await
            .map_err(|e| TcguiError::ZenohError {
                message: format!("Failed to send interface event: {}", e),
            })?;

        info!(
            "Sent interface event for {}:{} via publisher",
            namespace, interface_event.interface.name
        );

        Ok(())
    }

    /// Enables a network interface by bringing it UP.
    ///
    /// Uses native nlink for both default and named namespaces.
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    pub async fn enable_interface(&self, namespace: &str, interface: &str) -> Result<()> {
        info!(
            "Enabling interface {} in namespace {}",
            interface, namespace
        );

        if namespace == "default" {
            self.connection
                .set_link_up(interface)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to enable interface: {}", e))?;
        } else if namespace.starts_with("container:") {
            // Container namespace - get the container and use its namespace path
            let container_name = namespace.strip_prefix("container:").unwrap();
            let cache = self.cached_containers.read().await;
            if let Some(container) = cache.get(namespace) {
                let ns_path = container.namespace_path.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Container {} has no namespace path", container_name)
                })?;
                let conn =
                    Connection::new_in_namespace_path(Protocol::Route, ns_path).map_err(|e| {
                        anyhow::anyhow!("Failed to connect to container namespace: {}", e)
                    })?;
                conn.set_link_up(interface)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to enable interface: {}", e))?;
            } else {
                return Err(anyhow::anyhow!(
                    "Container {} not found in cache",
                    container_name
                ));
            }
        } else {
            // Named namespace
            let conn = namespace::connection_for(namespace)
                .map_err(|e| anyhow::anyhow!("Failed to connect to namespace: {}", e))?;
            conn.set_link_up(interface)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to enable interface: {}", e))?;
        }

        info!(
            "Successfully enabled interface {} in namespace {}",
            interface, namespace
        );

        Ok(())
    }

    /// Disables a network interface by bringing it DOWN.
    ///
    /// Uses native nlink for both default and named namespaces.
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    pub async fn disable_interface(&self, namespace: &str, interface: &str) -> Result<()> {
        info!(
            "Disabling interface {} in namespace {}",
            interface, namespace
        );

        if namespace == "default" {
            self.connection
                .set_link_down(interface)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to disable interface: {}", e))?;
        } else if namespace.starts_with("container:") {
            // Container namespace
            let container_name = namespace.strip_prefix("container:").unwrap();
            let cache = self.cached_containers.read().await;
            if let Some(container) = cache.get(namespace) {
                let ns_path = container.namespace_path.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Container {} has no namespace path", container_name)
                })?;
                let conn =
                    Connection::new_in_namespace_path(Protocol::Route, ns_path).map_err(|e| {
                        anyhow::anyhow!("Failed to connect to container namespace: {}", e)
                    })?;
                conn.set_link_down(interface)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to disable interface: {}", e))?;
            } else {
                return Err(anyhow::anyhow!(
                    "Container {} not found in cache",
                    container_name
                ));
            }
        } else {
            // Named namespace
            let conn = namespace::connection_for(namespace)
                .map_err(|e| anyhow::anyhow!("Failed to connect to namespace: {}", e))?;
            conn.set_link_down(interface)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to disable interface: {}", e))?;
        }

        info!(
            "Successfully disabled interface {} in namespace {}",
            interface, namespace
        );

        Ok(())
    }
}
