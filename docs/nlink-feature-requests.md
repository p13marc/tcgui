# nlink Feature Requests from tcgui

This document outlines features and API improvements that would benefit tcgui based on analysis of how the codebase currently uses nlink.

## Summary

| Priority | Feature | Impact |
|----------|---------|--------|
| High | Netem parameter detection API | Eliminates fragile string parsing |
| High | Unified namespace connection API | Removes duplicated namespace handling |
| High | Qdisc reset/atomic update | Simplifies 60+ lines of workaround code |
| Medium | Convenience TC query methods | Reduces boilerplate |
| Medium | Granular error types | Better error handling in frontend |
| Medium | Multi-namespace event stream | Simplifies event management |
| Low | Rate unit conversion helpers | Cleaner code |
| Low | StatsTracker rate caching | Less boilerplate |

---

## High Priority

### 1. Netem Parameter Detection API

**Current Problem:**

tcgui uses fragile string parsing to detect which TC parameters are currently configured:

```rust
// tcgui-backend/src/tc_commands.rs:790-800
fn parse_current_tc_config(&self, qdisc_info: &str) -> CurrentTcConfig {
    CurrentTcConfig {
        has_loss: qdisc_info.contains("loss"),
        has_delay: qdisc_info.contains("delay"),
        has_duplicate: qdisc_info.contains("duplicate"),
        has_reorder: qdisc_info.contains("reorder"),
        has_corrupt: qdisc_info.contains("corrupt"),
        has_rate: qdisc_info.contains("rate"),
    }
}
```

This is error-prone and depends on string formatting remaining consistent.

**Proposed nlink API:**

```rust
impl NetemOptions {
    pub fn has_loss(&self) -> bool;
    pub fn has_delay(&self) -> bool;
    pub fn has_duplicate(&self) -> bool;
    pub fn has_reorder(&self) -> bool;
    pub fn has_corrupt(&self) -> bool;
    pub fn has_rate(&self) -> bool;
    
    // Or a more generic approach:
    pub fn configured_parameters(&self) -> HashSet<NetemParameter>;
}

pub enum NetemParameter {
    Loss,
    Delay,
    Jitter,
    Duplicate,
    Reorder,
    Corrupt,
    Rate,
}
```

---

### 2. Unified Namespace Connection API

**Current Problem:**

Every TC/bandwidth operation repeats the same namespace handling pattern across multiple files:

```rust
// Pattern repeated in tc_commands.rs, network.rs, bandwidth.rs
fn create_connection(namespace: &str, namespace_path: Option<&Path>) -> Result<Connection> {
    if namespace == "default" {
        Connection::new(Protocol::Route)
    } else if Self::is_container_namespace(namespace) {
        if let Some(ns_path) = namespace_path {
            Connection::new_in_namespace_path(Protocol::Route, ns_path)
        } else { /* error */ }
    } else {
        namespace::connection_for(namespace)
    }
}
```

**Proposed nlink API:**

```rust
pub enum NamespaceSpec<'a> {
    Default,
    Named(&'a str),
    Path(&'a Path),
}

impl Connection {
    pub fn new_for_namespace(protocol: Protocol, spec: NamespaceSpec<'_>) -> Result<Self> {
        match spec {
            NamespaceSpec::Default => Connection::new(protocol),
            NamespaceSpec::Named(name) => namespace::connection_for(name),
            NamespaceSpec::Path(path) => Connection::new_in_namespace_path(protocol, path),
        }
    }
}
```

---

### 3. Qdisc Reset/Atomic Update

**Current Problem:**

tcgui has 60+ lines of complex logic to determine whether to use `replace` vs `delete+add`:

```rust
// tcgui-backend/src/tc_commands.rs:803-861
fn needs_qdisc_recreation(
    &self,
    current: &CurrentTcConfig,
    loss: f32,
    delay_ms: Option<f64>,
    // ... 12+ parameters
) -> bool {
    let will_remove_loss = current.has_loss && loss <= 0.0;
    let will_remove_delay = current.has_delay && delay_ms.is_none_or(|d| d <= 0.0);
    // ... more parameter checks for each TC feature
    
    will_remove_loss || will_remove_delay || /* ... 6 more conditions */
}
```

This is because the kernel's `tc qdisc replace` preserves old parameters - there's no way to "unset" a parameter without deleting the qdisc entirely.

**Proposed nlink API:**

Option A - Reset method:
```rust
impl Connection {
    /// Applies netem config, automatically using delete+add if needed to remove parameters
    pub async fn apply_netem_with_reset(
        &self,
        ifindex: u32,
        config: NetemConfig,
    ) -> Result<()>;
}
```

Option B - Builder pattern:
```rust
impl NetemConfigBuilder {
    /// Mark that unset parameters should be explicitly removed (triggers delete+add internally)
    pub fn reset_unset_parameters(self) -> Self;
}
```

Option C - Expose decision helper:
```rust
impl Connection {
    /// Returns true if applying `new_config` over `current` requires delete+add
    pub fn netem_update_requires_recreation(
        current: &NetemOptions,
        new_config: &NetemConfig,
    ) -> bool;
}
```

---

## Medium Priority

### 4. Convenience TC Query Methods

**Current Problem:**

Finding the netem config for an interface requires manual filtering:

```rust
pub async fn get_netem_options(&self, interface: &str) -> Result<Option<NetemOptions>> {
    let qdiscs = conn.get_qdiscs_for(interface).await?;
    for qdisc in qdiscs {
        if qdisc.parent() == 0xFFFFFFFF && qdisc.kind() == Some("netem") {
            return Ok(qdisc.netem_options());
        }
    }
    Ok(None)
}
```

**Proposed nlink API:**

```rust
impl Connection {
    /// Get the root qdisc for an interface (parent == 0xFFFFFFFF)
    pub async fn get_root_qdisc_for(&self, interface: &str) -> Result<Option<TcMessage>>;
    
    /// Convenience: get netem options if a netem qdisc is the root
    pub async fn get_netem_config_for(&self, interface: &str) -> Result<Option<NetemOptions>>;
}
```

---

### 5. Granular Error Types

**Current Problem:**

nlink errors are wrapped generically, losing important context:

```rust
Connection::new(Protocol::Route).map_err(|e| TcguiError::NetworkError {
    message: format!("Failed to create nlink connection: {}", e),
})
```

The frontend can't distinguish between "permission denied" and "interface not found".

**Proposed nlink API:**

```rust
#[derive(Error, Debug)]
pub enum Error {
    #[error("Permission denied: {context}")]
    PermissionDenied { context: String },
    
    #[error("Namespace not found: {namespace}")]
    NamespaceNotFound { namespace: String },
    
    #[error("Interface not found: {interface}")]
    InterfaceNotFound { interface: String },
    
    #[error("Qdisc not found for interface {interface}")]
    QdiscNotFound { interface: String },
    
    #[error("Socket error: {0}")]
    Socket(#[from] std::io::Error),
    
    #[error("Netlink protocol error: {0}")]
    Protocol(String),
}

impl Error {
    pub fn is_permission_denied(&self) -> bool;
    pub fn is_not_found(&self) -> bool;
}
```

---

### 6. Multi-Namespace Event Stream

**Current Problem:**

tcgui manually manages event streams for multiple namespaces:

```rust
// netlink_events.rs - NamespaceEventManager
pub fn add_namespace(&mut self, target: NamespaceTarget) -> Result<(), String> {
    let mut builder = EventStream::builder().links(true).tc(true);
    builder = match &target {
        NamespaceTarget::Default => builder,
        NamespaceTarget::Named(name) => builder.namespace(name),
        NamespaceTarget::Path { path, .. } => builder.namespace_path(path),
    };
    let stream = builder.build()?;
    
    // Manual task spawning and channel management
    let handle = tokio::spawn(async move { ... });
    self.active_namespaces.insert(namespace_name, handle);
}
```

**Proposed nlink API:**

```rust
pub struct MultiNamespaceEventStream {
    // Internal management
}

impl MultiNamespaceEventStream {
    pub fn new() -> Self;
    pub fn add_namespace(&mut self, spec: NamespaceSpec<'_>) -> Result<()>;
    pub fn remove_namespace(&mut self, name: &str);
    
    /// Returns (namespace_name, event)
    pub async fn next(&mut self) -> Option<(String, NetworkEvent)>;
}
```

---

## Low Priority

### 7. Rate Unit Conversion Helpers

**Current Problem:**

Manual unit conversion is error-prone:

```rust
// tcgui converts kbps to bytes/sec
let bytes_per_sec = (config.rate_limit.rate_kbps as u64) * 1000 / 8;
netem = netem.rate(bytes_per_sec);
```

**Proposed nlink API:**

```rust
pub mod units {
    pub fn kbps_to_bytes_per_sec(kbps: u32) -> u64 {
        (kbps as u64) * 1000 / 8
    }
    
    pub fn mbps_to_bytes_per_sec(mbps: u32) -> u64 {
        (mbps as u64) * 1_000_000 / 8
    }
    
    pub fn bytes_per_sec_to_kbps(bps: u64) -> u32 {
        (bps * 8 / 1000) as u32
    }
}

// Or in NetemConfigBuilder:
impl NetemConfigBuilder {
    pub fn rate_kbps(self, kbps: u32) -> Self;
    pub fn rate_mbps(self, mbps: u32) -> Self;
    pub fn rate_bytes_per_sec(self, bps: u64) -> Self;
}
```

---

### 8. StatsTracker Rate Caching

**Current Problem:**

tcgui manually caches rates from StatsTracker:

```rust
struct NamespaceStatsTracker {
    tracker: StatsTracker,
    last_rates: HashMap<u32, (f64, f64)>,  // Manual cache
}

// On each update:
let rates_snapshot = tracker.tracker.update(snapshot);
if let Some(ref rates) = rates_snapshot {
    if let Some(link_rates) = rates.links.get(&ifindex) {
        tracker.last_rates.insert(ifindex, (rx, tx));  // Cache manually
        (rx, tx)
    } else {
        tracker.last_rates.get(&ifindex).copied().unwrap_or((0.0, 0.0))
    }
}
```

**Proposed nlink API:**

```rust
impl StatsTracker {
    /// Get cached rate for an interface (from last update)
    pub fn get_rate(&self, ifindex: u32) -> Option<LinkRates>;
    
    /// Get all cached rates
    pub fn get_all_rates(&self) -> &HashMap<u32, LinkRates>;
}
```

---

### 9. Batch/Transaction Operations

**Current Problem:**

Applying TC to multiple interfaces is sequential:

```rust
for (namespace, interface) in interfaces {
    tc_manager.apply_tc_config(&namespace, &interface, config).await?;
}
```

**Proposed nlink API:**

```rust
pub struct NetlinkBatch {
    operations: Vec<NetlinkOperation>,
}

impl NetlinkBatch {
    pub fn new() -> Self;
    pub fn add_qdisc(&mut self, ifindex: u32, config: NetemConfig);
    pub fn del_qdisc(&mut self, ifindex: u32);
    
    /// Execute all operations, returning results for each
    pub async fn execute(self, conn: &Connection) -> Vec<Result<()>>;
}
```

---

## Additional Notes

### Interface Index Keying

tcgui uses a workaround for multi-namespace interface tracking:

```rust
// Composite key to avoid collisions across namespaces
let composite_key = index + (namespace_id * 1000000);
```

This is a tcgui architecture issue rather than an nlink limitation. The proper solution is using `(namespace: String, ifindex: u32)` tuple keys in tcgui's HashMap.

### Container Namespace Watching

Currently `NamespaceWatcher` only monitors `/var/run/netns`. Container namespaces (via `/proc/PID/ns/net`) aren't detected. This could be a future enhancement but has significant complexity due to container runtime integration.

---

## Contact

For questions about these feature requests or to discuss implementation details, please open an issue on the nlink repository.
