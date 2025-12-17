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
    /// Complete namespace information from backend (for future metadata use)
    #[allow(dead_code)] // Keep for future use/debugging
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
