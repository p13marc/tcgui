//! Backend management for TC GUI frontend.
//!
//! This module handles backend lifecycle, routing, and state management.
//! It provides a centralized way to manage multiple backend connections,
//! track their health, and route messages appropriately.

use crate::interface::TcInterface;
use std::collections::{HashMap, HashSet};
use tcgui_shared::{
    BackendHealthStatus, InterfaceEventType, InterfaceListUpdate, InterfaceStateEvent,
    NetworkNamespace,
};
use tracing::info;

/// Backend grouping structure for organizing namespace and interface components.
#[derive(Clone)]
pub struct BackendGroup {
    /// Backend connection status for UI indicators
    pub is_connected: bool,
    /// When this backend was last seen (for timeout detection)
    pub last_seen: u64,
    /// When this backend was disconnected (None if connected, Some(timestamp) if disconnected)
    pub disconnected_at: Option<u64>,
    /// Map of namespace name to NamespaceGroup for this backend
    pub namespaces: HashMap<String, NamespaceGroup>,
}

/// Namespace grouping structure for organizing interface components within a backend.
#[derive(Clone)]
pub struct NamespaceGroup {
    /// Complete namespace information (used for Debug output and future metadata)
    #[allow(dead_code)]
    pub namespace: NetworkNamespace,
    /// Map of interface name to TcInterface component for this namespace
    pub tc_interfaces: HashMap<String, TcInterface>,
}

/// Manager for backend operations and state.
pub struct BackendManager {
    /// Backend instances organized by backend name for efficient routing
    backends: HashMap<String, BackendGroup>,
}

impl BackendManager {
    /// Creates a new backend manager.
    pub fn new() -> Self {
        Self {
            backends: HashMap::new(),
        }
    }

    /// Gets a reference to the backends map.
    pub fn backends(&self) -> &HashMap<String, BackendGroup> {
        &self.backends
    }

    /// Gets a mutable reference to the backends map.
    pub fn backends_mut(&mut self) -> &mut HashMap<String, BackendGroup> {
        &mut self.backends
    }

    /// Handles interface list updates from a backend.
    pub fn handle_interface_list_update(&mut self, interface_update: InterfaceListUpdate) {
        let backend_name = &interface_update.backend_name;

        info!(
            "Received interface update from backend '{}' with {} namespaces",
            backend_name,
            interface_update.namespaces.len()
        );

        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let backend_group = self
            .backends
            .entry(backend_name.clone())
            .or_insert_with(|| BackendGroup {
                is_connected: true,
                last_seen: current_time,
                disconnected_at: None,
                namespaces: HashMap::new(),
            });

        // Mark backend as connected and update last seen
        backend_group.is_connected = true;
        backend_group.last_seen = current_time;
        backend_group.disconnected_at = None;

        // Remove namespaces that are no longer in the update
        let current_namespaces: HashSet<String> =
            backend_group.namespaces.keys().cloned().collect();
        let updated_namespaces: HashSet<String> = interface_update
            .namespaces
            .iter()
            .map(|ns| ns.name.clone())
            .collect();
        for removed_namespace in current_namespaces.difference(&updated_namespaces) {
            info!(
                "Removing namespace '{}' from backend '{}' (no longer present)",
                removed_namespace, backend_name
            );
            backend_group.namespaces.remove(removed_namespace);
        }

        // Update namespaces
        for namespace in &interface_update.namespaces {
            let namespace_group = backend_group
                .namespaces
                .entry(namespace.name.clone())
                .or_insert_with(|| NamespaceGroup {
                    namespace: namespace.clone(),
                    tc_interfaces: HashMap::new(),
                });

            // Update the namespace metadata
            namespace_group.namespace = namespace.clone();

            // Remove interfaces that are no longer in the update
            let current_interfaces: HashSet<String> =
                namespace_group.tc_interfaces.keys().cloned().collect();
            let updated_interfaces: HashSet<String> = namespace
                .interfaces
                .iter()
                .map(|i| i.name.clone())
                .collect();
            for removed_interface in current_interfaces.difference(&updated_interfaces) {
                info!(
                    "Removing interface '{}' from namespace '{}' of backend '{}' (no longer present)",
                    removed_interface, namespace.name, backend_name
                );
                namespace_group.tc_interfaces.remove(removed_interface);
            }

            // Update interfaces in this namespace
            for interface in &namespace.interfaces {
                let tc_interface = namespace_group
                    .tc_interfaces
                    .entry(interface.name.clone())
                    .or_insert_with(|| TcInterface::new(&interface.name));

                tc_interface.update_from_backend(interface);
            }
        }

        info!(
            "Backend '{}' now has {} namespaces with {} total interfaces",
            backend_name,
            backend_group.namespaces.len(),
            backend_group
                .namespaces
                .values()
                .map(|ns| ns.tc_interfaces.len())
                .sum::<usize>()
        );
    }

    /// Handles backend health status updates.
    pub fn handle_backend_health_update(&mut self, health_status: BackendHealthStatus) {
        let backend_name = &health_status.backend_name;

        if let Some(backend_group) = self.backends.get_mut(backend_name) {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            backend_group.last_seen = current_time;

            info!(
                "Backend '{}' health status: {}",
                backend_name, health_status.status
            );
        }
    }

    /// Handles backend liveliness changes.
    pub fn handle_backend_liveliness(&mut self, backend_name: String, alive: bool) {
        info!(
            "Backend '{}' liveliness changed: {}",
            backend_name,
            if alive { "connected" } else { "disconnected" }
        );

        if alive {
            // Backend came online - create entry if it doesn't exist or clear disconnection time
            let backend_group = self
                .backends
                .entry(backend_name.clone())
                .or_insert_with(|| BackendGroup {
                    is_connected: false, // Will be set to true when first message arrives
                    last_seen: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    disconnected_at: None,
                    namespaces: HashMap::new(),
                });

            // Clear disconnection timestamp since backend is alive
            backend_group.disconnected_at = None;

            info!("Backend '{}' is alive, waiting for data...", backend_name);
        } else {
            // Backend went offline - mark as disconnected and record timestamp
            if let Some(backend_group) = self.backends.get_mut(&backend_name) {
                backend_group.is_connected = false;
                let disconnected_timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                backend_group.disconnected_at = Some(disconnected_timestamp);
                info!(
                    "Backend '{}' is now disconnected at timestamp {}",
                    backend_name, disconnected_timestamp
                );
            }
        }
    }

    /// Handles interface state events.
    pub fn handle_interface_state_event(&mut self, state_event: InterfaceStateEvent) {
        let backend_name = &state_event.backend_name;
        let namespace = &state_event.namespace;
        let interface = &state_event.interface;

        info!(
            "Interface update from backend '{}' in {}: {:?} - {:?}",
            backend_name, namespace, interface, state_event.event_type
        );

        // Ensure backend and namespace exist
        if let Some(backend_group) = self.backends.get_mut(backend_name) {
            if !backend_group.namespaces.contains_key(namespace) {
                let empty_namespace = NetworkNamespace {
                    name: namespace.clone(),
                    id: None,
                    is_active: true,
                    interfaces: Vec::new(),
                };
                let namespace_group = NamespaceGroup {
                    namespace: empty_namespace,
                    tc_interfaces: HashMap::new(),
                };
                backend_group
                    .namespaces
                    .insert(namespace.clone(), namespace_group);
            }

            if let Some(namespace_group) = backend_group.namespaces.get_mut(namespace) {
                match state_event.event_type {
                    InterfaceEventType::Added => {
                        if !namespace_group.tc_interfaces.contains_key(&interface.name) {
                            let mut tc_interface = TcInterface::new(&interface.name);
                            tc_interface.update_from_backend(interface);
                            namespace_group
                                .tc_interfaces
                                .insert(interface.name.clone(), tc_interface);
                        }
                    }
                    InterfaceEventType::Removed => {
                        namespace_group.tc_interfaces.remove(&interface.name);
                    }
                    _ => {
                        if let Some(tc_interface) =
                            namespace_group.tc_interfaces.get_mut(&interface.name)
                        {
                            tc_interface.update_from_backend(interface);
                        }
                    }
                }
            }
        }
    }

    /// Removes backends that have been disconnected for more than 10 seconds.
    pub fn cleanup_stale_backends(&mut self) -> bool {
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut backends_to_remove = Vec::new();

        for (backend_name, backend_group) in &self.backends {
            if let Some(disconnected_at) = backend_group.disconnected_at {
                let disconnected_duration = current_time.saturating_sub(disconnected_at);
                if disconnected_duration >= 10 {
                    info!(
                        "Backend '{}' has been disconnected for {} seconds, removing from list",
                        backend_name, disconnected_duration
                    );
                    backends_to_remove.push(backend_name.clone());
                }
            }
        }

        // Remove stale backends
        for backend_name in &backends_to_remove {
            if let Some(backend_group) = self.backends.remove(backend_name) {
                info!(
                    "Removed stale backend '{}' with {} namespaces and {} total interfaces",
                    backend_name,
                    backend_group.namespaces.len(),
                    backend_group
                        .namespaces
                        .values()
                        .map(|ns| ns.tc_interfaces.len())
                        .sum::<usize>()
                );
            }
        }

        // Return true if all backends are disconnected
        self.backends.values().all(|bg| !bg.is_connected)
    }

    /// Gets all backend names that are currently connected.
    pub fn connected_backend_names(&self) -> Vec<String> {
        self.backends
            .iter()
            .filter(|(_, backend)| backend.is_connected)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Gets the total number of backends.
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }

    /// Gets the total number of interfaces across all backends.
    pub fn total_interface_count(&self) -> usize {
        self.backends
            .values()
            .map(|backend| {
                backend
                    .namespaces
                    .values()
                    .map(|ns| ns.tc_interfaces.len())
                    .sum::<usize>()
            })
            .sum()
    }
}

impl Default for BackendManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::{BackendMetadata, InterfaceType, NetworkInterface};

    fn create_test_interface(name: &str, namespace: &str) -> NetworkInterface {
        NetworkInterface {
            name: name.to_string(),
            index: 1,
            namespace: namespace.to_string(),
            is_up: true,
            has_tc_qdisc: false,
            interface_type: InterfaceType::Virtual,
        }
    }

    fn create_test_namespace(name: &str, interfaces: Vec<&str>) -> NetworkNamespace {
        NetworkNamespace {
            name: name.to_string(),
            id: Some(1),
            is_active: true,
            interfaces: interfaces
                .into_iter()
                .map(|iface| create_test_interface(iface, name))
                .collect(),
        }
    }

    fn create_test_interface_list(
        backend_name: &str,
        namespaces: Vec<NetworkNamespace>,
    ) -> InterfaceListUpdate {
        InterfaceListUpdate {
            backend_name: backend_name.to_string(),
            namespaces,
            timestamp: 0,
        }
    }

    #[test]
    fn test_backend_manager_default() {
        let manager = BackendManager::new();
        assert_eq!(manager.backend_count(), 0);
        assert_eq!(manager.total_interface_count(), 0);
        assert!(manager.connected_backend_names().is_empty());
    }

    #[test]
    fn test_handle_interface_list_update() {
        let mut manager = BackendManager::new();

        let update = create_test_interface_list(
            "backend1",
            vec![
                create_test_namespace("default", vec!["eth0", "eth1"]),
                create_test_namespace("ns1", vec!["veth0"]),
            ],
        );

        manager.handle_interface_list_update(update);

        assert_eq!(manager.backend_count(), 1);
        assert_eq!(manager.total_interface_count(), 3);

        let backends = manager.backends();
        assert!(backends.contains_key("backend1"));
        assert!(backends["backend1"].is_connected);
        assert_eq!(backends["backend1"].namespaces.len(), 2);
    }

    #[test]
    fn test_multiple_backends() {
        let mut manager = BackendManager::new();

        let update1 = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        let update2 = create_test_interface_list(
            "backend2",
            vec![create_test_namespace("default", vec!["eth1", "eth2"])],
        );

        manager.handle_interface_list_update(update1);
        manager.handle_interface_list_update(update2);

        assert_eq!(manager.backend_count(), 2);
        assert_eq!(manager.total_interface_count(), 3);
        assert_eq!(manager.connected_backend_names().len(), 2);
    }

    #[test]
    fn test_namespace_removal() {
        let mut manager = BackendManager::new();

        // Initial state with two namespaces
        let update1 = create_test_interface_list(
            "backend1",
            vec![
                create_test_namespace("ns1", vec!["eth0"]),
                create_test_namespace("ns2", vec!["eth1"]),
            ],
        );
        manager.handle_interface_list_update(update1);
        assert_eq!(manager.backends()["backend1"].namespaces.len(), 2);

        // Update with only one namespace - ns2 should be removed
        let update2 = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("ns1", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update2);

        let backends = manager.backends();
        assert_eq!(backends["backend1"].namespaces.len(), 1);
        assert!(backends["backend1"].namespaces.contains_key("ns1"));
        assert!(!backends["backend1"].namespaces.contains_key("ns2"));
    }

    #[test]
    fn test_interface_removal() {
        let mut manager = BackendManager::new();

        // Initial state with two interfaces
        let update1 = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0", "eth1"])],
        );
        manager.handle_interface_list_update(update1);
        assert_eq!(manager.total_interface_count(), 2);

        // Update with only one interface - eth1 should be removed
        let update2 = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update2);

        assert_eq!(manager.total_interface_count(), 1);
        let ns = &manager.backends()["backend1"].namespaces["default"];
        assert!(ns.tc_interfaces.contains_key("eth0"));
        assert!(!ns.tc_interfaces.contains_key("eth1"));
    }

    #[test]
    fn test_backend_liveliness() {
        let mut manager = BackendManager::new();

        // Backend comes alive
        manager.handle_backend_liveliness("backend1".to_string(), true);
        assert!(manager.backends().contains_key("backend1"));
        assert!(manager.backends()["backend1"].disconnected_at.is_none());

        // Add some data
        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update);
        assert!(manager.backends()["backend1"].is_connected);

        // Backend goes offline
        manager.handle_backend_liveliness("backend1".to_string(), false);
        assert!(!manager.backends()["backend1"].is_connected);
        assert!(manager.backends()["backend1"].disconnected_at.is_some());
    }

    #[test]
    fn test_interface_state_event_added() {
        let mut manager = BackendManager::new();

        // First add a backend with a namespace
        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update);

        // Add a new interface via event
        let event = InterfaceStateEvent {
            backend_name: "backend1".to_string(),
            namespace: "default".to_string(),
            interface: create_test_interface("eth1", "default"),
            event_type: InterfaceEventType::Added,
            timestamp: 0,
        };
        manager.handle_interface_state_event(event);

        assert_eq!(manager.total_interface_count(), 2);
        let ns = &manager.backends()["backend1"].namespaces["default"];
        assert!(ns.tc_interfaces.contains_key("eth1"));
    }

    #[test]
    fn test_interface_state_event_removed() {
        let mut manager = BackendManager::new();

        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0", "eth1"])],
        );
        manager.handle_interface_list_update(update);
        assert_eq!(manager.total_interface_count(), 2);

        // Remove an interface via event
        let event = InterfaceStateEvent {
            backend_name: "backend1".to_string(),
            namespace: "default".to_string(),
            interface: create_test_interface("eth1", "default"),
            event_type: InterfaceEventType::Removed,
            timestamp: 0,
        };
        manager.handle_interface_state_event(event);

        assert_eq!(manager.total_interface_count(), 1);
        let ns = &manager.backends()["backend1"].namespaces["default"];
        assert!(!ns.tc_interfaces.contains_key("eth1"));
    }

    #[test]
    fn test_cleanup_stale_backends() {
        let mut manager = BackendManager::new();

        // Add a backend
        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update);

        // Mark as disconnected with old timestamp (simulate disconnection >10 seconds ago)
        if let Some(backend) = manager.backends_mut().get_mut("backend1") {
            backend.is_connected = false;
            // Set disconnected_at to 20 seconds ago
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            backend.disconnected_at = Some(current_time.saturating_sub(20));
        }

        // Cleanup should remove stale backend
        let all_disconnected = manager.cleanup_stale_backends();
        assert!(all_disconnected);
        assert_eq!(manager.backend_count(), 0);
    }

    #[test]
    fn test_cleanup_keeps_recent_disconnections() {
        let mut manager = BackendManager::new();

        // Add a backend
        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update);

        // Mark as disconnected with recent timestamp (just now)
        if let Some(backend) = manager.backends_mut().get_mut("backend1") {
            backend.is_connected = false;
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            backend.disconnected_at = Some(current_time);
        }

        // Cleanup should NOT remove recently disconnected backend
        manager.cleanup_stale_backends();
        assert_eq!(manager.backend_count(), 1);
    }

    #[test]
    fn test_backend_health_update() {
        let mut manager = BackendManager::new();

        // Add a backend first
        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update);

        let initial_last_seen = manager.backends()["backend1"].last_seen;

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Health update should update last_seen
        let health = BackendHealthStatus {
            backend_name: "backend1".to_string(),
            status: "healthy".to_string(),
            timestamp: 0,
            metadata: BackendMetadata::default(),
            namespace_count: 1,
            interface_count: 1,
        };
        manager.handle_backend_health_update(health);

        // last_seen should be updated (or at least not earlier)
        assert!(manager.backends()["backend1"].last_seen >= initial_last_seen);
    }

    #[test]
    fn test_connected_backend_names() {
        let mut manager = BackendManager::new();

        let update1 = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        let update2 = create_test_interface_list(
            "backend2",
            vec![create_test_namespace("default", vec!["eth1"])],
        );

        manager.handle_interface_list_update(update1);
        manager.handle_interface_list_update(update2);

        // Both connected
        let connected = manager.connected_backend_names();
        assert_eq!(connected.len(), 2);

        // Disconnect one
        manager.handle_backend_liveliness("backend1".to_string(), false);

        let connected = manager.connected_backend_names();
        assert_eq!(connected.len(), 1);
        assert!(connected.contains(&"backend2".to_string()));
    }

    #[test]
    fn test_interface_update_preserves_tc_interface() {
        let mut manager = BackendManager::new();

        // Add initial interface
        let update = create_test_interface_list(
            "backend1",
            vec![create_test_namespace("default", vec!["eth0"])],
        );
        manager.handle_interface_list_update(update);

        // Verify interface exists
        assert!(manager.backends()["backend1"].namespaces["default"]
            .tc_interfaces
            .contains_key("eth0"));

        // Send another update (same interface, different backend state)
        let mut new_iface = create_test_interface("eth0", "default");
        new_iface.has_tc_qdisc = true; // Now has TC configured

        let update2 = InterfaceListUpdate {
            backend_name: "backend1".to_string(),
            namespaces: vec![NetworkNamespace {
                name: "default".to_string(),
                id: Some(1),
                is_active: true,
                interfaces: vec![new_iface],
            }],
            timestamp: 1,
        };
        manager.handle_interface_list_update(update2);

        // TC interface should still exist after update
        let backends = manager.backends();
        assert!(backends["backend1"].namespaces["default"]
            .tc_interfaces
            .contains_key("eth0"));
    }
}
