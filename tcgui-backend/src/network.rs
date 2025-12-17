//! Network interface management and monitoring for multiple namespaces.
//!
//! This module provides comprehensive network interface discovery, monitoring,
//! and management across multiple Linux network namespaces. It uses both
//! rtnetlink for efficient default namespace operations and `ip` command
//! execution for named namespace access.
//!
//! # Key Features
//!
//! * **Multi-namespace support**: Discovers interfaces in default and named namespaces
//! * **Real-time monitoring**: Detects interface additions, removals, and state changes
//! * **Traffic control detection**: Identifies interfaces with active TC qdisc configurations
//! * **Interface type classification**: Categorizes Physical, Virtual, Veth, Bridge, etc.
//! * **Robust error handling**: Graceful handling of namespace access permissions

use anyhow::Result;
use futures_util::stream::TryStreamExt;
use netlink_packet_route::link::{LinkAttribute, LinkFlags, LinkMessage};
use rtnetlink::Handle;
use std::collections::HashMap;
use std::process::Command as StdCommand;
use std::time::Duration;
use tokio::process::Command;
use tracing::{error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::{
    errors::{BackendError, TcguiError},
    topics, InterfaceEventType, InterfaceListUpdate, InterfaceStateEvent, InterfaceType,
    NamespaceType, NetworkInterface, NetworkNamespace,
};

use crate::container::{Container, ContainerManager};

/// Network interface manager for multi-namespace operations.
///
/// This struct provides comprehensive network interface management across
/// multiple Linux network namespaces. It combines efficient rtnetlink operations
/// for the default namespace with `ip netns exec` commands for named namespaces.
///
/// # Architecture
///
/// * **Default namespace**: Uses rtnetlink handle for direct kernel communication
/// * **Named namespaces**: Uses `ip netns exec` for namespace-isolated operations
/// * **Interface tracking**: Maintains per-namespace interface maps for change detection
/// * **TC detection**: Checks for active traffic control configurations
///
/// # Usage
///
/// ```rust,no_run
/// use tcgui_backend::network::NetworkManager;
/// use zenoh::Session;
/// use rtnetlink::Handle;
///
/// async fn setup_manager(session: Session, handle: Handle) -> anyhow::Result<()> {
///     let manager = NetworkManager::new(session, handle, "test-backend".to_string()).await?;
///     let interfaces = manager.discover_all_interfaces().await?;
///     Ok(())
/// }
/// ```
pub struct NetworkManager {
    /// rtnetlink handle for efficient default namespace operations
    rt_handle: Handle,
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
    /// * `rt_handle` - rtnetlink handle for efficient default namespace operations
    /// * `backend_name` - Unique name for this backend instance for topic routing
    ///
    /// # Returns
    ///
    /// A new `NetworkManager` ready to discover and monitor network interfaces
    /// across multiple namespaces with backend-specific topic routing.
    #[instrument(skip(session, rt_handle), fields(backend_name = %backend_name))]
    pub async fn new(
        session: Session,
        rt_handle: Handle,
        backend_name: String,
    ) -> Result<Self, TcguiError> {
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
            rt_handle,
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
    /// This method handles both the default namespace (using rtnetlink for efficiency)
    /// and named namespaces (using `ip netns exec` commands). It performs comprehensive
    /// interface discovery including type detection, state checking, and TC configuration detection.
    ///
    /// # Arguments
    ///
    /// * `namespace` - The target namespace name ("default" for host namespace)
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap<u32, NetworkInterface>)` - Map of interface index to interface data
    /// * `Err(BackendError)` - On namespace access failures or system errors
    ///
    /// # Behavior
    ///
    /// * **Default namespace**: Uses rtnetlink for efficient kernel communication
    /// * **Named namespace**: Executes `ip netns exec <ns> ip -j link show`
    /// * **Type detection**: Classifies interfaces by type (Physical, Veth, Bridge, etc.)
    /// * **TC detection**: Checks for active traffic control qdisc configurations
    /// * **State monitoring**: Determines UP/DOWN status and configuration state
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tcgui_backend::network::NetworkManager;
    /// # async fn example(manager: &NetworkManager) -> anyhow::Result<()> {
    /// // Discover interfaces in default namespace
    /// let default_interfaces = manager.discover_interfaces_in_namespace("default").await?;
    ///
    /// // Discover interfaces in named namespace
    /// let ns_interfaces = manager.discover_interfaces_in_namespace("test-ns").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn discover_interfaces_in_namespace(
        &self,
        namespace: &str,
    ) -> Result<HashMap<u32, NetworkInterface>, BackendError> {
        info!("Discovering network interfaces in namespace: {}", namespace);

        let mut discovered_interfaces = HashMap::new();

        if namespace == "default" {
            // Use rtnetlink for default namespace
            let mut links = self.rt_handle.link().get().execute();

            while let Some(msg) = links.try_next().await? {
                let interface = self.parse_link_message(msg, namespace).await;
                discovered_interfaces.insert(interface.index, interface);
            }
        } else {
            // Use ip command for named namespaces
            match self.discover_interfaces_via_ip_command(namespace).await {
                Ok(interfaces) => discovered_interfaces = interfaces,
                Err(e) => {
                    error!(
                        "Failed to discover interfaces in namespace {}: {}",
                        namespace, e
                    );
                    return Err(BackendError::NetworkError {
                        message: format!(
                            "Failed to discover interfaces in namespace {}: {}",
                            namespace, e
                        ),
                    });
                }
            }
        }

        Ok(discovered_interfaces)
    }

    /// Discover interfaces via ip command (for named namespaces)
    async fn discover_interfaces_via_ip_command(
        &self,
        namespace: &str,
    ) -> Result<HashMap<u32, NetworkInterface>> {
        let output = Command::new("ip")
            .args(["netns", "exec", namespace, "ip", "-j", "link", "show"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to get interfaces: {}", stderr));
        }

        let stdout = String::from_utf8(output.stdout)?;
        let mut interfaces = HashMap::new();

        // Parse JSON output from ip command
        if let Ok(json_data) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(links) = json_data.as_array() {
                for link in links {
                    if let Some(interface) = self.parse_ip_json_interface(link, namespace).await {
                        interfaces.insert(interface.index, interface);
                    }
                }
            }
        } else {
            warn!(
                "Failed to parse JSON output from ip command for namespace {}",
                namespace
            );
        }

        Ok(interfaces)
    }

    /// Parse interface from ip command JSON output
    async fn parse_ip_json_interface(
        &self,
        json: &serde_json::Value,
        namespace: &str,
    ) -> Option<NetworkInterface> {
        let name = json["ifname"].as_str()?.to_string();
        let index = json["ifindex"].as_u64()? as u32;
        let flags = json["flags"].as_array()?;
        let is_up = flags.iter().any(|f| f.as_str() == Some("UP"));

        // Determine interface type
        let interface_type = if let Some(link_type) = json["link_type"].as_str() {
            match link_type {
                "loopback" => InterfaceType::Loopback,
                "veth" => InterfaceType::Veth,
                "bridge" => InterfaceType::Bridge,
                "tun" => InterfaceType::Tun,
                "tap" => InterfaceType::Tap,
                _ => {
                    if name.starts_with("veth") {
                        InterfaceType::Veth
                    } else if name.starts_with("br-") || name == "docker0" {
                        InterfaceType::Bridge
                    } else {
                        InterfaceType::Physical
                    }
                }
            }
        } else {
            // Fallback detection based on name
            if name == "lo" {
                InterfaceType::Loopback
            } else if name.starts_with("veth") {
                InterfaceType::Veth
            } else if name.starts_with("br-") || name == "docker0" {
                InterfaceType::Bridge
            } else {
                InterfaceType::Physical
            }
        };

        let has_tc_qdisc = self
            .check_tc_qdisc_in_namespace(namespace, &name)
            .await
            .unwrap_or(false);

        Some(NetworkInterface {
            name,
            index,
            namespace: namespace.to_string(),
            is_up,
            has_tc_qdisc,
            interface_type,
        })
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

    async fn parse_link_message(&self, msg: LinkMessage, namespace: &str) -> NetworkInterface {
        let mut name = format!("unknown_{}", msg.header.index);

        for attr in msg.attributes {
            if let LinkAttribute::IfName(n) = attr {
                name = n;
                break;
            }
        }

        let is_up = msg.header.flags.contains(LinkFlags::Up);

        // Determine interface type based on name and properties
        let interface_type = if name == "lo" {
            InterfaceType::Loopback
        } else if name.starts_with("veth") {
            InterfaceType::Veth
        } else if name.starts_with("br-") || name == "docker0" {
            InterfaceType::Bridge
        } else {
            InterfaceType::Physical
        };

        // Check if interface has TC qdisc
        let has_tc_qdisc = if namespace == "default" {
            self.check_tc_qdisc(&name).await.unwrap_or(false)
        } else {
            self.check_tc_qdisc_in_namespace(namespace, &name)
                .await
                .unwrap_or(false)
        };

        NetworkInterface {
            name,
            index: msg.header.index,
            namespace: namespace.to_string(),
            is_up,
            has_tc_qdisc,
            interface_type,
        }
    }

    async fn check_tc_qdisc(&self, interface: &str) -> Result<bool> {
        let output = StdCommand::new("tc")
            .args(["qdisc", "show", "dev", interface])
            .output()
            .map_err(|e| TcguiError::TcCommandError {
                message: format!("Failed to check TC qdisc: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("netem"))
    }

    /// Check TC qdisc in a specific namespace
    async fn check_tc_qdisc_in_namespace(&self, namespace: &str, interface: &str) -> Result<bool> {
        let output = if namespace == "default" {
            Command::new("tc")
                .args(["qdisc", "show", "dev", interface])
                .output()
                .await?
        } else {
            Command::new("ip")
                .args([
                    "netns", "exec", namespace, "tc", "qdisc", "show", "dev", interface,
                ])
                .output()
                .await?
        };

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.contains("netem"))
        } else {
            Ok(false)
        }
    }

    // Note: namespace-organized interface lists are now sent via send_interface_list method

    /// Discovers accessible network namespaces on the system.
    ///
    /// This method scans the system for existing network namespaces using `ip netns list`
    /// and tests accessibility to avoid permission-denied errors during interface discovery.
    /// It always includes the "default" namespace and only includes named namespaces that
    /// are actually accessible by the current process.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<String>)` - List of accessible namespace names, always includes "default"
    /// * `Err` - On command execution failures (gracefully continues with default only)
    ///
    /// # Behavior
    ///
    /// * **Always includes "default"**: The host namespace is always available
    /// * **Permission testing**: Tests each namespace for accessibility before including it
    /// * **Graceful failure**: If `ip netns list` fails, continues with default namespace only
    /// * **Access logging**: Logs which namespaces are accessible vs. permission-denied
    /// * **Deduplication**: Prevents duplicate namespace entries
    /// * **Parsing**: Handles both simple names and "name (id: N)" format
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tcgui_backend::network::NetworkManager;
    /// # async fn example(manager: &NetworkManager) -> anyhow::Result<()> {
    /// let namespaces = manager.discover_all_namespaces().await?;
    /// // Result might be: ["default", "user-ns1"] (excludes permission-denied namespaces)
    /// # Ok(())
    /// # }
    /// ```
    /// Tests if a network namespace is accessible to the current process.
    ///
    /// This method attempts a simple, non-intrusive operation in the namespace
    /// to determine if it's accessible without causing permission errors.
    async fn is_namespace_accessible(&self, namespace: &str) -> bool {
        if namespace == "default" {
            return true; // Default namespace is always accessible
        }

        // Test accessibility by trying to list interfaces with a timeout
        let result = Command::new("ip")
            .args(["netns", "exec", namespace, "ip", "link", "show"])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                info!("Namespace '{}' is accessible", namespace);
                true
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("Operation not permitted")
                    || stderr.contains("Permission denied")
                {
                    info!(
                        "Namespace '{}' is not accessible due to permissions",
                        namespace
                    );
                } else {
                    warn!("Namespace '{}' test failed: {}", namespace, stderr);
                }
                false
            }
            Err(e) => {
                warn!(
                    "Failed to test accessibility of namespace '{}': {}",
                    namespace, e
                );
                false
            }
        }
    }

    pub async fn discover_all_namespaces(&self) -> Result<Vec<String>> {
        info!("Discovering accessible network namespaces");

        let mut accessible_namespaces = vec!["default".to_string()];

        let output = Command::new("ip").args(["netns", "list"]).output().await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut discovered_namespaces = Vec::new();

            for line in stdout.lines() {
                // Each line is either just the namespace name or "name (id: N)"
                let namespace = line.split_whitespace().next().unwrap_or("").to_string();
                if !namespace.is_empty() && !accessible_namespaces.contains(&namespace) {
                    discovered_namespaces.push(namespace);
                }
            }

            info!(
                "Found {} potential namespaces, testing accessibility...",
                discovered_namespaces.len()
            );

            // Test accessibility for each discovered namespace
            for namespace in discovered_namespaces {
                if self.is_namespace_accessible(&namespace).await {
                    accessible_namespaces.push(namespace);
                }
            }
        } else {
            warn!("Failed to list network namespaces, continuing with default only");
        }

        info!(
            "Discovered {} accessible namespaces: {:?}",
            accessible_namespaces.len(),
            accessible_namespaces
        );
        Ok(accessible_namespaces)
    }

    /// Discovers running containers and returns them with their network namespaces.
    ///
    /// This method queries Docker and Podman runtimes (if available) to find
    /// running containers and their network namespace information.
    ///
    /// # Returns
    ///
    /// A vector of discovered containers with namespace paths
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

    /// Discovers interfaces inside a container's network namespace using nsenter.
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

        // Use the container manager to discover interfaces
        match self
            .container_manager
            .discover_container_interfaces(container)
            .await
        {
            Ok(interface_names) => {
                let mut interfaces = HashMap::new();

                for (idx, name) in interface_names.iter().enumerate() {
                    // Check if interface is UP by querying via nsenter
                    let is_up = self
                        .check_interface_up_in_container(container, name)
                        .await
                        .unwrap_or(false);

                    // Check TC qdisc
                    let has_tc_qdisc = self
                        .check_tc_qdisc_in_container(container, name)
                        .await
                        .unwrap_or(false);

                    // Determine interface type
                    // Container eth interfaces are typically veth pairs on the host side
                    let interface_type = if name == "lo" {
                        InterfaceType::Loopback
                    } else if name.starts_with("eth") || name.starts_with("veth") {
                        InterfaceType::Veth
                    } else {
                        InterfaceType::Virtual
                    };

                    let interface = NetworkInterface {
                        name: name.clone(),
                        index: idx as u32 + 1, // 1-based index within container
                        namespace: namespace_name.clone(),
                        is_up,
                        has_tc_qdisc,
                        interface_type,
                    };

                    interfaces.insert(interface.index, interface);
                }

                Ok(interfaces)
            }
            Err(e) => {
                error!(
                    "Failed to discover interfaces in container {}: {}",
                    container.name, e
                );
                Err(BackendError::NetworkError {
                    message: format!(
                        "Failed to discover interfaces in container {}: {}",
                        container.name, e
                    ),
                })
            }
        }
    }

    /// Check if an interface is UP inside a container
    async fn check_interface_up_in_container(
        &self,
        container: &Container,
        interface: &str,
    ) -> Result<bool> {
        let output = self
            .container_manager
            .exec_in_netns(container, &["ip", "-o", "link", "show", interface])
            .await?;

        Ok(output.contains("state UP") || output.contains(",UP,") || output.contains("<UP,"))
    }

    /// Check TC qdisc inside a container
    async fn check_tc_qdisc_in_container(
        &self,
        container: &Container,
        interface: &str,
    ) -> Result<bool> {
        let output = self
            .container_manager
            .exec_in_netns(container, &["tc", "qdisc", "show", "dev", interface])
            .await?;

        Ok(output.contains("netem"))
    }

    /// Discovers network interfaces across all available namespaces.
    ///
    /// This is the primary interface discovery method that combines namespace discovery
    /// with per-namespace interface enumeration. It provides a complete view of all
    /// network interfaces available on the system across all namespaces.
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap<u32, NetworkInterface>)` - Map of composite keys to interfaces
    /// * `Err(BackendError)` - On critical system failures (individual namespace failures are logged)
    ///
    /// # Key Generation
    ///
    /// To avoid interface index conflicts between namespaces, this method uses:
    /// * **Default namespace**: Uses original interface index
    /// * **Named namespaces**: Uses `index + (namespace.len() * 1000000)` for uniqueness
    ///
    /// # Error Handling
    ///
    /// * **Per-namespace resilience**: Failure in one namespace doesn't stop discovery in others
    /// * **Logging**: Failed namespace discoveries are logged as errors
    /// * **Graceful degradation**: Returns available interfaces even if some namespaces fail
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tcgui_backend::network::NetworkManager;
    /// # async fn example(manager: &NetworkManager) -> anyhow::Result<()> {
    /// let all_interfaces = manager.discover_all_interfaces().await?;
    /// println!("Found {} interfaces across all namespaces", all_interfaces.len());
    /// # Ok(())
    /// # }
    /// ```
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

        for namespace in namespaces {
            match self.discover_interfaces_in_namespace(&namespace).await {
                Ok(interfaces) => {
                    for (index, interface) in interfaces {
                        // Use a composite key to avoid index conflicts between namespaces
                        let composite_key = if namespace == "default" {
                            index
                        } else {
                            // Use high bits for namespace differentiation
                            index + (namespace.len() as u32 * 1000000)
                        };
                        all_interfaces.insert(composite_key, interface);
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to discover interfaces in namespace {}: {}",
                        namespace, e
                    );
                    // Continue with other namespaces
                }
            }
        }

        // Also discover container interfaces
        let containers = self.discover_containers().await;

        // Update the container cache for namespace type lookup
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
                        // Use a unique composite key for container interfaces
                        // Container namespace names start with "container:" prefix
                        let composite_key = index + (interface.namespace.len() as u32 * 1000000);
                        all_interfaces.insert(composite_key, interface);
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to discover interfaces in container {}: {}",
                        container.name, e
                    );
                    // Continue with other containers
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
    ///
    /// This method takes a flat map of interfaces and reorganizes them by namespace
    /// for structured display in the frontend. It creates `NetworkNamespace` objects
    /// containing their respective interfaces and sends the complete list via Zenoh.
    ///
    /// # Arguments
    ///
    /// * `interfaces` - Map of interface keys to NetworkInterface objects
    ///
    /// # Returns
    ///
    /// * `Ok(())` - On successful message sending
    /// * `Err` - On serialization or Zenoh communication failures
    ///
    /// # Behavior
    ///
    /// * **Namespace grouping**: Groups interfaces by their namespace field
    /// * **Structured response**: Creates NetworkNamespace objects with interface lists
    /// * **Zenoh messaging**: Sends via `BACKEND_TO_FRONTEND` topic
    /// * **JSON serialization**: Converts to JSON for frontend consumption
    ///
    /// # Message Format
    ///
    /// Sends `BackendMessage::InterfaceList { namespaces: Vec<NetworkNamespace> }`
    /// where each namespace contains its interfaces.
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
                    // Look up container metadata from cache
                    if let Some(container) = container_cache.get(&name) {
                        NamespaceType::Container {
                            runtime: format!("{:?}", container.runtime),
                            container_id: container.short_id.clone(),
                            image: container.image.clone(),
                        }
                    } else {
                        // Fallback if container not in cache
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
                    // Traditional ip netns namespace
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

        // Send on interface list topic using declared publisher
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

        // Send on interface events topic using declared publisher
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
    /// This method executes the appropriate `ip link set <interface> up` command
    /// in the specified namespace. It handles both default and named namespaces
    /// and provides comprehensive feedback to the frontend.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Target namespace ("default" or named namespace)
    /// * `interface` - Interface name to enable (e.g., "eth0", "fo")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Command executed (check frontend messages for actual success)
    /// * `Err` - On command execution system failures
    ///
    /// # Commands Executed
    ///
    /// * **Default namespace**: `ip link set <interface> up`
    /// * **Named namespace**: `ip netns exec <namespace> ip link set <interface> up`
    ///
    /// # Behavior
    ///
    /// * **Result reporting**: Sends `InterfaceStateResult` message to frontend
    /// * **Success feedback**: Reports successful interface enablement
    /// * **Error handling**: Captures and reports command stderr output
    /// * **Logging**: Comprehensive info/error logging for debugging
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    pub async fn enable_interface(&self, namespace: &str, interface: &str) -> Result<()> {
        info!(
            "Enabling interface {} in namespace {}",
            interface, namespace
        );

        let output = if namespace == "default" {
            Command::new("ip")
                .args(["link", "set", interface, "up"])
                .output()
                .await?
        } else {
            Command::new("ip")
                .args([
                    "netns", "exec", namespace, "ip", "link", "set", interface, "up",
                ])
                .output()
                .await?
        };

        if output.status.success() {
            info!(
                "Successfully enabled interface {} in namespace {}",
                interface, namespace
            );
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "Failed to enable interface {} in namespace {}: {}",
                interface, namespace, stderr
            );
            return Err(anyhow::anyhow!(
                "Failed to bring {} up: {}",
                interface,
                stderr
            ));
        }

        Ok(())
    }

    /// Disables a network interface by bringing it DOWN.
    ///
    /// This method executes the appropriate `ip link set <interface> down` command
    /// in the specified namespace. It provides the counterpart to `enable_interface`
    /// and handles both default and named namespaces with comprehensive feedback.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Target namespace ("default" or named namespace)
    /// * `interface` - Interface name to disable (e.g., "eth0", "fo")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Command executed (check frontend messages for actual success)
    /// * `Err` - On command execution system failures
    ///
    /// # Commands Executed
    ///
    /// * **Default namespace**: `ip link set <interface> down`
    /// * **Named namespace**: `ip netns exec <namespace> ip link set <interface> down`
    ///
    /// # Behavior
    ///
    /// * **Result reporting**: Sends `InterfaceStateResult` message to frontend
    /// * **Success feedback**: Reports successful interface disablement
    /// * **Error handling**: Captures and reports command stderr output
    /// * **Logging**: Comprehensive info/error logging for debugging
    ///
    /// # Warning
    ///
    /// Disabling network interfaces can interrupt network connectivity.
    /// Use with caution, especially on remote systems.
    #[instrument(skip(self), fields(backend_name = %self.backend_name, namespace, interface))]
    pub async fn disable_interface(&self, namespace: &str, interface: &str) -> Result<()> {
        info!(
            "Disabling interface {} in namespace {}",
            interface, namespace
        );

        let output = if namespace == "default" {
            Command::new("ip")
                .args(["link", "set", interface, "down"])
                .output()
                .await?
        } else {
            Command::new("ip")
                .args([
                    "netns", "exec", namespace, "ip", "link", "set", interface, "down",
                ])
                .output()
                .await?
        };

        if output.status.success() {
            info!(
                "Successfully disabled interface {} in namespace {}",
                interface, namespace
            );
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "Failed to disable interface {} in namespace {}: {}",
                interface, namespace, stderr
            );
            return Err(anyhow::anyhow!(
                "Failed to bring {} down: {}",
                interface,
                stderr
            ));
        }

        Ok(())
    }

    // Interface state results are now handled via query/reply pattern
    // No need for separate result messages
}
