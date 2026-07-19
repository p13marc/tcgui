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
use nlink::netlink::{Connection, Route, namespace};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::registry::tc;
use tcgui_shared::{
    InterfaceType, NetworkInterface,
    errors::{BackendError, TcguiError},
    identity::LocalOrigin,
};

use crate::container::{Container, ContainerManager};

/// Drop kernel-default root qdiscs that carry no user intent, so the UI only
/// surfaces a qdisc kind worth noting (netem, tbf, htb, cake, fq_codel, …).
fn interesting_qdisc_kind(kind: Option<String>) -> Option<String> {
    match kind.as_deref() {
        None | Some("noqueue" | "pfifo_fast" | "mq" | "pfifo" | "bfifo") => None,
        Some(_) => kind,
    }
}

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
/// use tcgui_shared::identity::mint_local_origin;
/// use zenoh::Session;
///
/// async fn setup_manager(session: Session) -> anyhow::Result<()> {
///     let origin = mint_local_origin();
///     let manager = NetworkManager::new(session, origin, "test-backend".to_string()).await?;
///     let interfaces = manager.discover_all_interfaces().await?;
///     Ok(())
/// }
/// ```
pub struct NetworkManager {
    /// nlink connection for default namespace operations
    connection: Connection<Route>,
    /// Track interfaces per namespace for change detection (future namespace monitoring)
    /// Map: namespace_name -> (interface_index -> NetworkInterface)
    #[allow(dead_code)]
    namespace_interfaces: HashMap<String, HashMap<u32, NetworkInterface>>,
    /// Zenoh session for declaring per-interface state publishers on demand.
    session: Session,
    /// This host's origin — every published state key is built from it.
    local_origin: LocalOrigin,
    /// Operator-chosen display label, used only for logging/instrumentation.
    backend_name: String,
    /// Per-interface state publishers keyed by (namespace, interface). The key
    /// set doubles as the "currently published" set for delete diffing: an
    /// interface that disappears from a rescan gets a Delete tombstone and is
    /// dropped from the map.
    interface_publishers: HashMap<(String, String), AdvancedPublisher<'static>>,
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
    #[instrument(skip(session, local_origin), fields(backend_name = %backend_name))]
    pub async fn new(
        session: Session,
        local_origin: LocalOrigin,
        backend_name: String,
    ) -> Result<Self, TcguiError> {
        // Create nlink connection for default namespace
        let connection = Connection::<Route>::new().map_err(|e| TcguiError::NetworkError {
            message: format!("Failed to create nlink connection: {}", e),
        })?;
        info!("[BACKEND] nlink connection established for default namespace");

        // Per-interface state publishers are declared lazily in send_interface_list.

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
            session,
            local_origin,
            backend_name,
            interface_publishers: HashMap::new(),
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

        let addr_map = Self::address_map(conn).await;
        let names: Vec<String> = links
            .iter()
            .map(|l| l.name_or(&format!("unknown{}", l.ifindex())).to_string())
            .collect();
        let speed_map = self.link_speed_map(&names).await;

        for link in links {
            let index = link.ifindex();
            let name = link.name_or(&format!("unknown{}", index)).to_string();
            let is_up = link.is_up();
            let is_oper_up = link.has_carrier();

            // Determine interface type
            let interface_type = Self::determine_interface_type(&name, &link);

            // Check TC qdisc + detect the root qdisc kind in one query
            let (has_tc_qdisc, qdisc_kind) =
                self.qdisc_info(conn, &name).await.unwrap_or((false, None));

            discovered_interfaces.insert(
                index,
                NetworkInterface {
                    name: name.clone(),
                    index,
                    namespace: namespace.to_string(),
                    is_up,
                    is_oper_up,
                    has_tc_qdisc,
                    interface_type,
                    addresses: addr_map.get(&index).cloned().unwrap_or_default(),
                    qdisc_kind,
                    link_speed_mbps: speed_map.get(&name).copied(),
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
        let addr_map = Self::address_map(&conn).await;

        for link in links {
            let index = link.ifindex();
            let name = link.name_or(&format!("unknown{}", index)).to_string();
            let is_up = link.is_up();
            let is_oper_up = link.has_carrier();

            // Determine interface type
            let interface_type = Self::determine_interface_type(&name, &link);

            // Check TC qdisc + detect the root qdisc kind in one query
            let (has_tc_qdisc, qdisc_kind) =
                self.qdisc_info(&conn, &name).await.unwrap_or((false, None));

            interfaces.insert(
                index,
                NetworkInterface {
                    name,
                    index,
                    namespace: namespace.to_string(),
                    is_up,
                    is_oper_up,
                    has_tc_qdisc,
                    interface_type,
                    addresses: addr_map.get(&index).cloned().unwrap_or_default(),
                    qdisc_kind,
                    link_speed_mbps: None,
                },
            );
        }

        Ok(interfaces)
    }

    /// Fetch every address in the namespace this connection is bound to,
    /// grouped by ifindex and formatted as `"ip/prefix"`. Best-effort: a query
    /// failure logs and yields an empty map rather than failing discovery.
    async fn address_map(conn: &nlink::netlink::Connection<Route>) -> HashMap<u32, Vec<String>> {
        match conn.get_addresses().await {
            Ok(addrs) => {
                let mut map: HashMap<u32, Vec<String>> = HashMap::new();
                for a in &addrs {
                    // Prefer the local address (correct on point-to-point links;
                    // equal to `address` on broadcast links).
                    if let Some(ip) = a.local().or_else(|| a.address()) {
                        map.entry(a.ifindex()).or_default().push(format!(
                            "{}/{}",
                            ip,
                            a.prefix_len()
                        ));
                    }
                }
                map
            }
            Err(e) => {
                tracing::warn!("Failed to get addresses: {}", e);
                HashMap::new()
            }
        }
    }

    /// Best-effort map of interface name -> physical link speed (Mbit/s) via
    /// the ethtool GENL family, for the namespace this process runs in.
    ///
    /// Read-only and graceful: if the ethtool family is unavailable (older
    /// kernel) the map is empty; interfaces without a link speed (loopback,
    /// veth, bridges) are simply absent. Only queried for the default namespace
    /// — ethtool connections are netns-bound and virtual interfaces in other
    /// namespaces rarely report a meaningful speed.
    async fn link_speed_map(&self, names: &[String]) -> HashMap<String, u32> {
        use nlink::netlink::Ethtool;

        let mut map = HashMap::new();
        let conn = match Connection::<Ethtool>::new_async().await {
            Ok(conn) => conn,
            Err(e) => {
                debug!("ethtool link-speed probe unavailable: {}", e);
                return map;
            }
        };
        for name in names {
            if let Ok(modes) = conn.get_link_modes_by_name(name).await
                && let Some(speed) = modes.speed
                && speed > 0
            {
                map.insert(name.clone(), speed);
            }
        }
        map
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
        conn: &Connection<Route>,
        interface: &str,
    ) -> Result<bool> {
        let qdiscs = conn
            .get_qdiscs_by_name(interface)
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

    /// Inspect an interface's qdiscs in a single query, returning both whether a
    /// netem qdisc is present (drives `has_tc_qdisc`, preserving its prior
    /// meaning) and the root qdisc kind for display (filtered to drop plain
    /// kernel-default qdiscs that carry no user intent).
    async fn qdisc_info(
        &self,
        conn: &Connection<Route>,
        interface: &str,
    ) -> Result<(bool, Option<String>)> {
        let qdiscs = conn
            .get_qdiscs_by_name(interface)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get qdiscs for {}: {}", interface, e))?;

        let mut is_netem = false;
        let mut root_kind = None;
        for qdisc in qdiscs {
            let kind = qdisc.kind().map(|k| k.to_string());
            if kind.as_deref() == Some("netem") {
                is_netem = true;
            }
            if qdisc.parent().is_root() {
                root_kind = kind;
            }
        }

        Ok((is_netem, interesting_qdisc_kind(root_kind)))
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
                let conn = Connection::<Route>::new_in_namespace_path(ns_path).map_err(|e| {
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
        let conn = Connection::<Route>::new_in_namespace_path(ns_path).map_err(|e| {
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
        let addr_map = Self::address_map(&conn).await;

        for link in links {
            let index = link.ifindex();
            let name = link.name_or(&format!("eth{}", index)).to_string();
            let is_up = link.is_up();
            let is_oper_up = link.has_carrier();

            // Determine interface type
            let interface_type = if link.is_loopback() {
                InterfaceType::Loopback
            } else if name.starts_with("eth") || name.starts_with("veth") {
                InterfaceType::Veth
            } else {
                InterfaceType::Virtual
            };

            // Check TC qdisc + detect the root qdisc kind in one query
            let (has_tc_qdisc, qdisc_kind) =
                self.qdisc_info(&conn, &name).await.unwrap_or((false, None));

            interfaces.insert(
                index,
                NetworkInterface {
                    name,
                    index,
                    namespace: namespace_name.clone(),
                    is_up,
                    is_oper_up,
                    has_tc_qdisc,
                    interface_type,
                    addresses: addr_map.get(&index).cloned().unwrap_or_default(),
                    qdisc_kind,
                    link_speed_mbps: None,
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

    /// Reconciles the published per-interface state against `interfaces`.
    ///
    /// This is the single interface-state reconciliation feed (keyspace-v2): it
    /// replaces the old `interfaces/list` snapshot + `interfaces/events` delta
    /// split. Every present interface is (re)published as a `NetworkInterface`
    /// Put on its own `state/tc/interface/{ns}/{if}` key; any interface that was
    /// previously published but is now absent gets a Delete tombstone and is
    /// dropped from the publisher set. A disabled NIC stays published (with
    /// `is_up=false`) — only a NIC that has *gone away* is tombstoned.
    #[instrument(skip(self, interfaces), fields(backend_name = %self.backend_name, interface_count = interfaces.len()))]
    pub async fn send_interface_list(
        &mut self,
        interfaces: &HashMap<u32, NetworkInterface>,
    ) -> Result<()> {
        // Publish (or refresh) every present interface as its own state record.
        // The payload is the bare `NetworkInterface` — it already carries its
        // `namespace`/`name`/`is_up`; the host origin is in the key, not the body.
        let mut present: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::with_capacity(interfaces.len());
        for interface in interfaces.values() {
            let ns = interface.namespace.clone();
            let name = interface.name.clone();
            present.insert((ns.clone(), name.clone()));

            let payload =
                serde_json::to_string(interface).map_err(TcguiError::SerializationError)?;

            let publisher = self.get_interface_publisher(&ns, &name).await?;
            publisher
                .put(payload)
                .encoding(zenoh::bytes::Encoding::APPLICATION_JSON)
                .await
                .map_err(|e| TcguiError::ZenohError {
                    message: format!("Failed to publish interface record: {}", e),
                })?;
        }

        // Tombstone interfaces that were published before but are gone now.
        let stale: Vec<(String, String)> = self
            .interface_publishers
            .keys()
            .filter(|key| !present.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            if let Some(publisher) = self.interface_publishers.remove(&key) {
                info!("Interface {}:{} gone — publishing tombstone", key.0, key.1);
                publisher
                    .delete()
                    .await
                    .map_err(|e| TcguiError::ZenohError {
                        message: format!("Failed to publish interface tombstone: {}", e),
                    })?;
            }
        }

        info!("Reconciled {} interface state record(s)", present.len());
        Ok(())
    }

    /// Get or create the per-interface state publisher for `(namespace, interface)`.
    async fn get_interface_publisher(
        &mut self,
        namespace: &str,
        interface: &str,
    ) -> Result<&AdvancedPublisher<'static>> {
        let key = (namespace.to_string(), interface.to_string());
        if !self.interface_publishers.contains_key(&key) {
            let topic = tc::key(
                &self.local_origin,
                &tc::Subject::interface(namespace, interface),
            );
            let publisher = self
                .session
                .declare_publisher(zenoh::key_expr::OwnedKeyExpr::from(topic))
                .cache(CacheConfig::default().max_samples(1))
                .sample_miss_detection(
                    MissDetectionConfig::default().heartbeat(Duration::from_millis(500)),
                )
                .publisher_detection()
                .await
                .map_err(|e| TcguiError::ZenohError {
                    message: format!("Failed to declare interface state publisher: {}", e),
                })?;
            self.interface_publishers.insert(key.clone(), publisher);
        }
        Ok(self.interface_publishers.get(&key).unwrap())
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
                let conn = Connection::<Route>::new_in_namespace_path(ns_path).map_err(|e| {
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
                let conn = Connection::<Route>::new_in_namespace_path(ns_path).map_err(|e| {
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
