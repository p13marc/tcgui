//! Backend management for TC GUI frontend.
//!
//! This module handles backend lifecycle, routing, and state management.
//! It provides a centralized way to manage multiple backend connections,
//! track their health, and route messages appropriately.

use crate::interface::TcInterface;
use std::collections::HashMap;
use tcgui_shared::{
    BackendHealthStatus, NamespaceType, NetworkInterface, NetworkNamespace,
    presets::{CustomPreset, PresetList},
};
use tracing::info;

/// Current unix time in seconds (best-effort, saturating on clock errors).
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Best-effort namespace-type inference from the namespace name.
///
/// The keyspace-v2 per-interface state record carries only the namespace name,
/// not the rich `NamespaceType` the old snapshot bundled. We synthesize a type
/// so host-vs-namespace filtering keeps working: `default`/empty is the host
/// root, everything else is treated as a traditional netns (container detail —
/// runtime/id — is unavailable from the interface record alone).
fn infer_namespace_type(name: &str) -> NamespaceType {
    if name.is_empty() || name == "default" {
        NamespaceType::Default
    } else {
        NamespaceType::Traditional
    }
}

/// Backend grouping structure for organizing namespace and interface components.
///
/// Keyed (in [`BackendManager::backends`]) on the **host origin** (`h-<12hex>`),
/// never the display name — that is the keyspace-v2 identity bridge (RFC 06 §6).
#[derive(Clone)]
pub struct BackendGroup {
    /// Operator-chosen display label from the health document (`backend_name`).
    /// Rendered in the UI; never used to route or key. Defaults to the origin
    /// until the first health document arrives.
    pub name: String,
    /// Backend connection status for UI indicators
    pub is_connected: bool,
    /// When this backend was last seen (for timeout detection)
    pub last_seen: u64,
    /// When this backend was disconnected (None if connected, Some(timestamp) if disconnected)
    pub disconnected_at: Option<u64>,
    /// Map of namespace name to NamespaceGroup for this backend
    pub namespaces: HashMap<String, NamespaceGroup>,
    /// Available presets (built-in and custom) from this backend
    pub preset_list: PresetList,
}

impl BackendGroup {
    /// A fresh, connected group with `name` defaulting to the origin.
    fn new(origin: &str) -> Self {
        Self {
            name: origin.to_string(),
            is_connected: true,
            last_seen: now_secs(),
            disconnected_at: None,
            namespaces: HashMap::new(),
            preset_list: PresetList::default(),
        }
    }
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
    /// Backend instances keyed by **host origin** (`h-<12hex>`) for routing.
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

    /// Returns a mutable reference to the backend group for `origin`, creating
    /// a fresh connected entry (with `name` defaulting to the origin) if absent.
    fn get_or_create(&mut self, origin: &str) -> &mut BackendGroup {
        self.backends
            .entry(origin.to_string())
            .or_insert_with(|| BackendGroup::new(origin))
    }

    /// Upserts a single interface from a `state/tc/interface/{ns}/{if}` Put.
    ///
    /// Replaces the old snapshot+events merge: one Put creates/updates exactly
    /// one interface; removal arrives as a Delete tombstone (see
    /// [`Self::handle_interface_removed`]). The payload's own `namespace` field
    /// selects the namespace group.
    pub fn handle_interface_upsert(&mut self, origin: &str, interface: NetworkInterface) {
        let namespace = interface.namespace.clone();
        let iface_name = interface.name.clone();

        let backend_group = self.get_or_create(origin);
        backend_group.is_connected = true;
        backend_group.last_seen = now_secs();
        backend_group.disconnected_at = None;

        let namespace_group = backend_group
            .namespaces
            .entry(namespace.clone())
            .or_insert_with(|| NamespaceGroup {
                namespace: NetworkNamespace {
                    name: namespace.clone(),
                    id: None,
                    is_active: true,
                    namespace_type: infer_namespace_type(&namespace),
                    interfaces: Vec::new(),
                },
                tc_interfaces: HashMap::new(),
            });

        // Keep the namespace's interface record list in sync (message handlers
        // look it up by name when applying TC config updates).
        if let Some(existing) = namespace_group
            .namespace
            .interfaces
            .iter_mut()
            .find(|i| i.name == iface_name)
        {
            *existing = interface.clone();
        } else {
            namespace_group.namespace.interfaces.push(interface.clone());
        }

        let tc_interface = namespace_group
            .tc_interfaces
            .entry(iface_name.clone())
            .or_insert_with(|| TcInterface::new(&iface_name));
        tc_interface.update_from_backend(&interface);

        info!(
            "Upserted interface '{}' in namespace '{}' of backend '{}'",
            iface_name, namespace, origin
        );
    }

    /// Removes a single interface in response to a Delete tombstone on
    /// `state/tc/interface/{ns}/{if}`. `namespace`/`interface` come from the key
    /// (the Delete carries no payload).
    pub fn handle_interface_removed(&mut self, origin: &str, namespace: &str, interface: &str) {
        if let Some(backend_group) = self.backends.get_mut(origin)
            && let Some(namespace_group) = backend_group.namespaces.get_mut(namespace)
        {
            namespace_group.tc_interfaces.remove(interface);
            namespace_group
                .namespace
                .interfaces
                .retain(|i| i.name != interface);
            info!(
                "Removed interface '{}' from namespace '{}' of backend '{}' (tombstone)",
                interface, namespace, origin
            );
            // Drop the namespace group once it has no interfaces left.
            if namespace_group.tc_interfaces.is_empty() {
                backend_group.namespaces.remove(namespace);
            }
        }
    }

    /// Handles backend health status updates. `origin` is the key-derived host
    /// origin; the display label is taken from the health document.
    pub fn handle_backend_health_update(
        &mut self,
        origin: &str,
        health_status: BackendHealthStatus,
    ) {
        let backend_group = self.get_or_create(origin);
        backend_group.last_seen = now_secs();
        backend_group.disconnected_at = None;
        backend_group.name = health_status.backend_name.clone();

        info!(
            "Backend '{}' (name '{}') health status: {}",
            origin, health_status.backend_name, health_status.status
        );
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
                .or_insert_with(|| {
                    let mut group = BackendGroup::new(&backend_name);
                    // Will be set to true when the first data message arrives.
                    group.is_connected = false;
                    group
                });

            // Clear disconnection timestamp since backend is alive
            backend_group.disconnected_at = None;

            info!("Backend '{}' is alive, waiting for data...", backend_name);
        } else {
            // Backend went offline - mark as disconnected and record timestamp
            if let Some(backend_group) = self.backends.get_mut(&backend_name) {
                backend_group.is_connected = false;
                let disconnected_timestamp = now_secs();
                backend_group.disconnected_at = Some(disconnected_timestamp);
                info!(
                    "Backend '{}' is now disconnected at timestamp {}",
                    backend_name, disconnected_timestamp
                );
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

    /// Upserts a single preset from a `state/tc/preset/{id}` Put.
    pub fn upsert_preset(&mut self, origin: &str, preset: CustomPreset) {
        let backend_group = self.get_or_create(origin);
        let presets = &mut backend_group.preset_list.presets;
        if let Some(existing) = presets.iter_mut().find(|p| p.id == preset.id) {
            *existing = preset;
        } else {
            presets.push(preset);
        }
    }

    /// Removes a single preset in response to a Delete tombstone on
    /// `state/tc/preset/{id}`.
    pub fn remove_preset(&mut self, origin: &str, id: &str) {
        if let Some(backend_group) = self.backends.get_mut(origin) {
            backend_group.preset_list.presets.retain(|p| p.id != id);
            info!(
                "Removed preset '{}' from backend '{}' (tombstone)",
                id, origin
            );
        }
    }

    /// Gets the preset list for a specific backend.
    /// This method is prepared for future frontend preset UI integration.
    #[allow(dead_code)]
    pub fn get_preset_list(&self, origin: &str) -> Option<&PresetList> {
        self.backends.get(origin).map(|bg| &bg.preset_list)
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
    use tcgui_shared::{BackendMetadata, InterfaceType, TcNetemConfig};

    // Origins are opaque `h-<12hex>` strings on the wire; the manager only ever
    // uses them as map keys, so the tests use recognizable stand-ins.
    const ORIGIN1: &str = "h-000000000001";
    const ORIGIN2: &str = "h-000000000002";

    fn create_test_interface(name: &str, namespace: &str) -> NetworkInterface {
        NetworkInterface {
            name: name.to_string(),
            index: 1,
            namespace: namespace.to_string(),
            is_up: true,
            is_oper_up: true,
            has_tc_qdisc: false,
            interface_type: InterfaceType::Virtual,
            addresses: Vec::new(),
            qdisc_kind: None,
            link_speed_mbps: None,
        }
    }

    /// Upsert a batch of interfaces under one namespace (mimics the per-key
    /// Puts that arrive on `state/tc/interface/{ns}/{if}`).
    fn upsert_ns(manager: &mut BackendManager, origin: &str, namespace: &str, ifaces: &[&str]) {
        for iface in ifaces {
            manager.handle_interface_upsert(origin, create_test_interface(iface, namespace));
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
    fn test_handle_interface_upsert() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0", "eth1"]);
        upsert_ns(&mut manager, ORIGIN1, "ns1", &["veth0"]);

        assert_eq!(manager.backend_count(), 1);
        assert_eq!(manager.total_interface_count(), 3);

        let backends = manager.backends();
        assert!(backends.contains_key(ORIGIN1));
        assert!(backends[ORIGIN1].is_connected);
        assert_eq!(backends[ORIGIN1].namespaces.len(), 2);
        // Name defaults to the origin until a health doc arrives.
        assert_eq!(backends[ORIGIN1].name, ORIGIN1);
    }

    #[test]
    fn test_multiple_backends() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);
        upsert_ns(&mut manager, ORIGIN2, "default", &["eth1", "eth2"]);

        assert_eq!(manager.backend_count(), 2);
        assert_eq!(manager.total_interface_count(), 3);
        assert_eq!(manager.connected_backend_names().len(), 2);
    }

    #[test]
    fn test_interface_removed_tombstone() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0", "eth1"]);
        assert_eq!(manager.total_interface_count(), 2);

        // Delete tombstone removes exactly one interface.
        manager.handle_interface_removed(ORIGIN1, "default", "eth1");

        assert_eq!(manager.total_interface_count(), 1);
        let ns = &manager.backends()[ORIGIN1].namespaces["default"];
        assert!(ns.tc_interfaces.contains_key("eth0"));
        assert!(!ns.tc_interfaces.contains_key("eth1"));
    }

    #[test]
    fn test_namespace_dropped_when_last_interface_removed() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "ns1", &["eth0"]);
        upsert_ns(&mut manager, ORIGIN1, "ns2", &["eth1"]);
        assert_eq!(manager.backends()[ORIGIN1].namespaces.len(), 2);

        // Removing ns2's only interface drops the empty namespace group.
        manager.handle_interface_removed(ORIGIN1, "ns2", "eth1");

        let backends = manager.backends();
        assert_eq!(backends[ORIGIN1].namespaces.len(), 1);
        assert!(backends[ORIGIN1].namespaces.contains_key("ns1"));
        assert!(!backends[ORIGIN1].namespaces.contains_key("ns2"));
    }

    #[test]
    fn test_backend_liveliness() {
        let mut manager = BackendManager::new();

        // Backend comes alive
        manager.handle_backend_liveliness(ORIGIN1.to_string(), true);
        assert!(manager.backends().contains_key(ORIGIN1));
        assert!(manager.backends()[ORIGIN1].disconnected_at.is_none());

        // Add some data
        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);
        assert!(manager.backends()[ORIGIN1].is_connected);

        // Backend goes offline
        manager.handle_backend_liveliness(ORIGIN1.to_string(), false);
        assert!(!manager.backends()[ORIGIN1].is_connected);
        assert!(manager.backends()[ORIGIN1].disconnected_at.is_some());
    }

    #[test]
    fn test_cleanup_stale_backends() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);

        // Mark as disconnected with old timestamp (simulate disconnection >10 seconds ago)
        if let Some(backend) = manager.backends_mut().get_mut(ORIGIN1) {
            backend.is_connected = false;
            backend.disconnected_at = Some(now_secs().saturating_sub(20));
        }

        // Cleanup should remove stale backend
        let all_disconnected = manager.cleanup_stale_backends();
        assert!(all_disconnected);
        assert_eq!(manager.backend_count(), 0);
    }

    #[test]
    fn test_cleanup_keeps_recent_disconnections() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);

        // Mark as disconnected with recent timestamp (just now)
        if let Some(backend) = manager.backends_mut().get_mut(ORIGIN1) {
            backend.is_connected = false;
            backend.disconnected_at = Some(now_secs());
        }

        // Cleanup should NOT remove recently disconnected backend
        manager.cleanup_stale_backends();
        assert_eq!(manager.backend_count(), 1);
    }

    #[test]
    fn test_backend_health_update_sets_name() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);
        let initial_last_seen = manager.backends()[ORIGIN1].last_seen;
        std::thread::sleep(std::time::Duration::from_millis(10));

        let health = BackendHealthStatus {
            host_id: ORIGIN1.to_string(),
            backend_name: "lab-router".to_string(),
            status: "healthy".to_string(),
            timestamp: 0,
            metadata: BackendMetadata::default(),
            namespace_count: 1,
            interface_count: 1,
        };
        manager.handle_backend_health_update(ORIGIN1, health);

        // Display name comes from the health doc; routing key stays the origin.
        assert_eq!(manager.backends()[ORIGIN1].name, "lab-router");
        assert!(manager.backends()[ORIGIN1].last_seen >= initial_last_seen);
    }

    #[test]
    fn test_connected_backend_names() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);
        upsert_ns(&mut manager, ORIGIN2, "default", &["eth1"]);

        // Both connected
        assert_eq!(manager.connected_backend_names().len(), 2);

        // Disconnect one
        manager.handle_backend_liveliness(ORIGIN1.to_string(), false);

        let connected = manager.connected_backend_names();
        assert_eq!(connected.len(), 1);
        assert!(connected.contains(&ORIGIN2.to_string()));
    }

    #[test]
    fn test_interface_upsert_preserves_tc_interface() {
        let mut manager = BackendManager::new();

        upsert_ns(&mut manager, ORIGIN1, "default", &["eth0"]);
        assert!(
            manager.backends()[ORIGIN1].namespaces["default"]
                .tc_interfaces
                .contains_key("eth0")
        );

        // Re-upsert the same interface with a changed backend flag.
        let mut new_iface = create_test_interface("eth0", "default");
        new_iface.has_tc_qdisc = true;
        manager.handle_interface_upsert(ORIGIN1, new_iface);

        // TC interface component is reused, not recreated.
        assert!(
            manager.backends()[ORIGIN1].namespaces["default"]
                .tc_interfaces
                .contains_key("eth0")
        );
        assert_eq!(manager.total_interface_count(), 1);
    }

    #[test]
    fn test_preset_upsert_and_remove() {
        let mut manager = BackendManager::new();

        let preset = CustomPreset {
            id: "sat".to_string(),
            name: "Satellite".to_string(),
            description: "test".to_string(),
            config: TcNetemConfig::default(),
        };
        manager.upsert_preset(ORIGIN1, preset.clone());
        assert_eq!(manager.get_preset_list(ORIGIN1).map(|p| p.len()), Some(1));

        // Upserting the same id replaces rather than duplicates.
        manager.upsert_preset(ORIGIN1, preset);
        assert_eq!(manager.get_preset_list(ORIGIN1).map(|p| p.len()), Some(1));

        // Tombstone removes it.
        manager.remove_preset(ORIGIN1, "sat");
        assert_eq!(manager.get_preset_list(ORIGIN1).map(|p| p.len()), Some(0));
    }
}
