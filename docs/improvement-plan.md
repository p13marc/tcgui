# TC GUI Improvement Plan

This document outlines the implementation plan for improving the TC GUI codebase.

## Overview

| Task | Priority | Effort | Status |
|------|----------|--------|--------|
| 1. Replace debug println with tracing | High | 1-2 hours | Pending |
| 2. Implement publisher cleanup | High | 3-4 hours | Pending |
| 3. Split main.rs into modules | High | 1-2 days | Pending |
| 4. Migrate to structured TC config | Medium | 4-6 hours | Pending |
| 5. Replace polling with netlink events | Medium | 2-3 days | Pending |
| 6. Remove hot reload feature | Low | 1-2 hours | Pending |

---

## 1. Replace Debug println with Tracing

### Problem
10 debug `println!` statements left in production frontend code, cluttering console output.

### Location
`tcgui-frontend/src/interface/base.rs`

### Statements to Replace

| Line | Statement |
|------|-----------|
| 61 | `println!("DEBUG: LossChanged slider moved to: {}", v);` |
| 74 | `println!("DEBUG: CorrelationChanged slider moved to: {}", v);` |
| 94 | `println!("DEBUG: DelayChanged slider moved to: {}", v);` |
| 124 | `println!("DEBUG: DuplicatePercentageChanged slider moved to: {}", v);` |
| 143 | `println!("DEBUG: DuplicateCorrelationChanged slider moved to: {}", v);` |
| 157 | `println!("DEBUG: ReorderPercentageChanged slider moved to: {}", v);` |
| 167 | `println!("DEBUG: ReorderCorrelationChanged slider moved to: {}", v);` |
| 175 | `println!("DEBUG: ReorderGapChanged slider moved to: {}", v);` |
| 189 | `println!("DEBUG: CorruptPercentageChanged slider moved to: {}", v);` |
| 201 | `println!("DEBUG: CorruptCorrelationChanged slider moved to: {}", v);` |
| 214 | `println!("DEBUG: RateLimitChanged slider moved to: {}", v);` |

### Implementation

Replace `println!` with `tracing::debug!()` for optional verbose logging:

```rust
// Ensure tracing is in Cargo.toml (should already be present)
tracing = "0.1"

// Replace each println! with tracing::debug!
// Example:
// Before:
println!("DEBUG: LossChanged slider moved to: {}", v);
// After:
tracing::debug!("LossChanged slider moved to: {}", v);
```

### Verification
```bash
# After changes, verify no println! remain
grep -n "println!" tcgui-frontend/src/interface/base.rs
# Should return empty or only intentional prints

just dev-frontend  # Ensure builds cleanly
```

---

## 2. Implement Publisher Cleanup

### Problem
`tc_config_publishers` HashMap in `TcBackend` grows indefinitely. Publishers are created but never removed when interfaces disappear, causing memory leaks in long-running backends.

### Location
`tcgui-backend/src/main.rs`

### Current Code

**Struct field (line 46):**
```rust
tc_config_publishers: HashMap<String, AdvancedPublisher<'static>>,
```

**Get/create method (lines 920-952):**
```rust
async fn get_tc_config_publisher(
    &mut self,
    namespace: &str,
    interface: &str,
) -> Result<&AdvancedPublisher<'static>> {
    let key = format!("{}/{}", namespace, interface);
    if !self.tc_config_publishers.contains_key(&key) {
        // Creates new publisher, never removes old ones
    }
    Ok(self.tc_config_publishers.get(&key).unwrap())
}
```

### Implementation Steps

#### Step 2.1: Add cleanup method
```rust
/// Remove publishers for interfaces that no longer exist
fn cleanup_stale_publishers(&mut self, current_interfaces: &[NetworkInterface]) {
    // Build set of valid keys from current interfaces
    let valid_keys: HashSet<String> = current_interfaces
        .iter()
        .map(|iface| format!("{}/{}", iface.namespace, iface.name))
        .collect();

    // Find and remove stale publishers
    let stale_keys: Vec<String> = self
        .tc_config_publishers
        .keys()
        .filter(|key| !valid_keys.contains(*key))
        .cloned()
        .collect();

    for key in stale_keys {
        info!("Removing stale TC config publisher for: {}", key);
        self.tc_config_publishers.remove(&key);
    }
}
```

#### Step 2.2: Call cleanup after interface discovery
In the `run()` method, after `discover_all_interfaces()` succeeds:
```rust
_ = interface_monitor_interval.tick() => {
    match self.network_manager.discover_all_interfaces().await {
        Ok(discovered_interfaces) => {
            let filtered = Self::filter_interfaces(&discovered_interfaces, &self.config);
            
            // NEW: Clean up publishers for removed interfaces
            self.cleanup_stale_publishers(&filtered);
            
            // Existing logic...
            self.interfaces = filtered;
        }
    }
}
```

#### Step 2.3: Add graceful shutdown (optional)
Implement `Drop` or explicit shutdown method:
```rust
impl TcBackend {
    async fn shutdown(&mut self) {
        info!("Shutting down TcBackend, cleaning up {} publishers", 
              self.tc_config_publishers.len());
        self.tc_config_publishers.clear();
        // Publishers are dropped, Zenoh handles cleanup
    }
}
```

### Verification
```bash
just test-backend  # Run backend tests

# Manual verification:
# 1. Start backend
# 2. Create a veth pair, observe publisher creation in logs
# 3. Delete veth pair
# 4. Wait for next interface refresh (5s)
# 5. Verify "Removing stale TC config publisher" log message
```

---

## 3. Split main.rs into Modules

### Problem
`tcgui-backend/src/main.rs` is 1150+ lines with mixed concerns: query handlers, TC parsing, publisher management, and event loop all in one file.

### Current Structure

| Section | Lines | Description |
|---------|-------|-------------|
| Imports & TcBackend struct | 1-65 | Setup |
| TcBackend::new() | 75-145 | Initialization |
| TcBackend::run() | 162-310 | Main event loop |
| handle_tc_query() | 364-813 | TC query handler (450 lines) |
| handle_interface_query() | 814-883 | Interface query handler |
| send_backend_status() | 885-910 | Health publishing |
| get_tc_config_publisher() | 920-952 | Publisher management |
| parse_tc_parameters() | 980-1100 | TC config parsing |
| detect_current_tc_config() | 1102-1140 | TC detection |
| publish_tc_config() | 1150-1180 | Publishing |
| main() | 1190-1210 | Entry point |

### Proposed Module Structure

```
tcgui-backend/src/
├── main.rs                    # Entry point, TcBackend struct, run() loop
├── handlers/
│   ├── mod.rs
│   ├── tc_query.rs           # handle_tc_query() + TC config building
│   └── interface_query.rs    # handle_interface_query()
├── publishers/
│   ├── mod.rs
│   ├── tc_config.rs          # TC config publisher management + cleanup
│   └── status.rs             # send_backend_status()
└── tc_config/
    ├── mod.rs
    ├── parser.rs             # parse_tc_parameters()
    ├── detection.rs          # detect_current_tc_config()
    └── publishing.rs         # publish_tc_config()
```

### Implementation Steps

#### Step 3.1: Create handlers module
Create `tcgui-backend/src/handlers/mod.rs`:
```rust
mod tc_query;
mod interface_query;

pub use tc_query::handle_tc_query;
pub use interface_query::handle_interface_query;
```

#### Step 3.2: Extract TC query handler
Move lines 364-813 to `tcgui-backend/src/handlers/tc_query.rs`:
```rust
use crate::TcBackend;
use tcgui_shared::{TcRequest, TcResponse, TcguiError};
use zenoh::query::Query;

impl TcBackend {
    pub async fn handle_tc_query(&mut self, query: Query) -> Result<(), TcguiError> {
        // Existing implementation...
    }
}
```

#### Step 3.3: Extract interface query handler
Move lines 814-883 to `tcgui-backend/src/handlers/interface_query.rs`:
```rust
use crate::TcBackend;
use tcgui_shared::{InterfaceRequest, InterfaceResponse, TcguiError};
use zenoh::query::Query;

impl TcBackend {
    pub async fn handle_interface_query(&mut self, query: Query) -> Result<(), TcguiError> {
        // Existing implementation...
    }
}
```

#### Step 3.4: Create publishers module
Create `tcgui-backend/src/publishers/mod.rs`:
```rust
mod tc_config;
mod status;

pub use tc_config::TcConfigPublisherManager;
pub use status::send_backend_status;
```

Extract publisher management to `tcgui-backend/src/publishers/tc_config.rs`:
```rust
use std::collections::{HashMap, HashSet};
use zenoh_ext::AdvancedPublisher;
use tcgui_shared::NetworkInterface;

pub struct TcConfigPublisherManager {
    publishers: HashMap<String, AdvancedPublisher<'static>>,
    session: Arc<Session>,
    backend_name: String,
}

impl TcConfigPublisherManager {
    pub fn new(session: Arc<Session>, backend_name: String) -> Self { ... }
    pub async fn get_or_create(&mut self, namespace: &str, interface: &str) -> Result<&AdvancedPublisher<'static>> { ... }
    pub fn cleanup_stale(&mut self, current_interfaces: &[NetworkInterface]) { ... }
}
```

#### Step 3.5: Create tc_config module
Create `tcgui-backend/src/tc_config/mod.rs`:
```rust
mod parser;
mod detection;
mod publishing;

pub use parser::parse_tc_parameters;
pub use detection::detect_current_tc_config;
pub use publishing::publish_tc_config;
```

#### Step 3.6: Update main.rs
After extraction, main.rs should contain:
- `TcBackend` struct definition
- `TcBackend::new()` initialization
- `TcBackend::run()` event loop (with handler calls delegated)
- `main()` entry point

```rust
mod handlers;
mod publishers;
mod tc_config;

// ... remaining core logic
```

### Verification
```bash
just check       # Compile check
just clippy      # Lint check
just test-backend  # All tests pass
```

---

## 4. Migrate to Structured TC Config

### Problem
Legacy `apply_tc_config_in_namespace()` function has 15 parameters, making it error-prone and hard to maintain. A structured `TcNetemConfig` approach exists but isn't fully utilized.

### Location
`tcgui-backend/src/tc_commands.rs`

### Current State

**Legacy function signature:**
```rust
pub async fn apply_tc_config_in_namespace(
    &self,
    namespace: &str,
    interface: &str,
    loss: f32,
    correlation: Option<f32>,
    delay_ms: Option<f32>,
    delay_jitter_ms: Option<f32>,
    delay_correlation: Option<f32>,
    duplicate_percent: Option<f32>,
    duplicate_correlation: Option<f32>,
    reorder_percent: Option<f32>,
    reorder_correlation: Option<f32>,
    reorder_gap: Option<u32>,
    corrupt_percent: Option<f32>,
    corrupt_correlation: Option<f32>,
    rate_limit_kbps: Option<u32>,
) -> Result<String>
```

**Structured approach (already exists):**
```rust
pub async fn apply_tc_config_structured(
    &self,
    namespace: &str,
    interface: &str,
    config: &TcNetemConfig,
) -> Result<String>
```

### Implementation Steps

#### Step 4.1: Mark legacy function as deprecated
```rust
#[deprecated(since = "0.2.0", note = "Use apply_tc_config_structured() instead")]
pub async fn apply_tc_config_in_namespace(...) -> Result<String> {
    // Existing implementation
}
```

#### Step 4.2: Update call sites in main.rs
Find all calls to `apply_tc_config_in_namespace` in `handle_tc_query()` and replace with structured approach:

**Before:**
```rust
self.tc_manager.apply_tc_config_in_namespace(
    namespace,
    interface,
    loss,
    correlation,
    delay_ms,
    // ... 12 more parameters
).await?
```

**After:**
```rust
let config = TcNetemConfig {
    loss: Some(LossConfig { percent: loss, correlation }),
    delay: delay_ms.map(|ms| DelayConfig { 
        delay_ms: ms, 
        jitter_ms: delay_jitter_ms, 
        correlation: delay_correlation 
    }),
    // ... structured fields
};
self.tc_manager.apply_tc_config_structured(namespace, interface, &config).await?
```

#### Step 4.3: Simplify TcNetemConfig construction
Add builder pattern or `From` implementation for cleaner construction:

```rust
impl TcNetemConfig {
    pub fn builder() -> TcNetemConfigBuilder {
        TcNetemConfigBuilder::default()
    }
}

pub struct TcNetemConfigBuilder {
    config: TcNetemConfig,
}

impl TcNetemConfigBuilder {
    pub fn loss(mut self, percent: f32, correlation: Option<f32>) -> Self {
        self.config.loss = Some(LossConfig { percent, correlation });
        self
    }
    
    pub fn delay(mut self, delay_ms: f32, jitter_ms: Option<f32>, correlation: Option<f32>) -> Self {
        self.config.delay = Some(DelayConfig { delay_ms, jitter_ms, correlation });
        self
    }
    
    // ... other methods
    
    pub fn build(self) -> TcNetemConfig {
        self.config
    }
}
```

#### Step 4.4: Remove legacy function (future)
Once all call sites are migrated and a release cycle has passed, remove the deprecated function entirely.

### Verification
```bash
just check  # Should show deprecation warnings at call sites
just test   # All tests pass
```

---

## 5. Replace Polling with Netlink Events

### Problem
Interface discovery uses 5-second polling interval, causing:
- Delayed detection of interface changes
- Unnecessary CPU cycles for unchanged interfaces
- Poor responsiveness in frontend

### Location
- `tcgui-backend/src/network.rs` - Discovery logic
- `tcgui-backend/src/main.rs` - Polling loop

### Current Code
```rust
// In run() method
let mut interface_monitor_interval = interval(Duration::from_secs(5));

_ = interface_monitor_interval.tick() => {
    match self.network_manager.discover_all_interfaces().await {
        // Full rediscovery every 5 seconds
    }
}
```

### Implementation Steps

#### Step 5.1: Add rtnetlink event monitoring
Create `tcgui-backend/src/network/events.rs`:

```rust
use futures::StreamExt;
use rtnetlink::new_connection;
use netlink_packet_route::link::LinkMessage;
use tokio::sync::mpsc;

pub enum InterfaceEvent {
    Added(NetworkInterface),
    Removed { name: String, namespace: String },
    StateChanged { name: String, namespace: String, up: bool },
}

pub struct InterfaceEventMonitor {
    event_tx: mpsc::Sender<InterfaceEvent>,
}

impl InterfaceEventMonitor {
    pub async fn start(event_tx: mpsc::Sender<InterfaceEvent>) -> Result<Self> {
        let (connection, handle, mut messages) = new_connection()?;
        
        // Spawn connection handler
        tokio::spawn(connection);
        
        // Subscribe to link events
        // RTMGRP_LINK = 1
        handle.socket_mut().socket_mut().add_membership(1)?;
        
        // Spawn event processing task
        tokio::spawn(async move {
            while let Some((message, _)) = messages.next().await {
                if let NetlinkPayload::InnerMessage(RtnlMessage::NewLink(link)) = message.payload {
                    // Parse and send InterfaceEvent::Added
                }
                if let NetlinkPayload::InnerMessage(RtnlMessage::DelLink(link)) = message.payload {
                    // Parse and send InterfaceEvent::Removed
                }
            }
        });
        
        Ok(Self { event_tx })
    }
}
```

#### Step 5.2: Update NetworkManager
Add event-based monitoring alongside existing discovery:

```rust
impl NetworkManager {
    pub async fn start_event_monitoring(&self) -> mpsc::Receiver<InterfaceEvent> {
        let (tx, rx) = mpsc::channel(100);
        InterfaceEventMonitor::start(tx).await?;
        rx
    }
}
```

#### Step 5.3: Modify run() to use events
```rust
// In TcBackend::run()
let mut interface_events = self.network_manager.start_event_monitoring().await?;

// Fallback polling for named namespaces (less frequent)
let mut namespace_poll_interval = interval(Duration::from_secs(30));

loop {
    select! {
        // Event-driven for default namespace (immediate)
        Some(event) = interface_events.recv() => {
            match event {
                InterfaceEvent::Added(iface) => {
                    self.interfaces.push(iface);
                    self.network_manager.send_interface_list(&self.interfaces).await?;
                }
                InterfaceEvent::Removed { name, namespace } => {
                    self.interfaces.retain(|i| !(i.name == name && i.namespace == namespace));
                    self.cleanup_stale_publishers(&self.interfaces);
                    self.network_manager.send_interface_list(&self.interfaces).await?;
                }
                InterfaceEvent::StateChanged { name, namespace, up } => {
                    // Update interface state
                }
            }
        }
        
        // Fallback polling for named namespaces
        _ = namespace_poll_interval.tick() => {
            // Only discover interfaces in named namespaces
            // Default namespace handled by events
        }
    }
}
```

#### Step 5.4: Handle named namespaces
Named network namespaces don't receive netlink events from the default namespace. Keep polling for these, but reduce frequency:

```rust
async fn discover_named_namespace_interfaces(&self) -> Result<Vec<NetworkInterface>> {
    // Only discover in /var/run/netns/* namespaces
    // Skip default namespace (handled by events)
}
```

### Verification
```bash
just test-backend

# Manual testing:
# Terminal 1: Run backend with RUST_LOG=debug
# Terminal 2: 
sudo ip link add veth0 type veth peer name veth1
# Observe immediate detection in backend logs (vs 5s delay)
sudo ip link del veth0
# Observe immediate removal
```

---

## 6. Remove Hot Reload Feature

### Problem
The hot reload feature in `tcgui-backend/src/config/hot_reload.rs` is extensive (~450 lines) but incomplete and unused. It adds complexity without providing value.

### Location
- `tcgui-backend/src/config/hot_reload.rs` - Main implementation
- `tcgui-backend/src/config/mod.rs` - Module declaration

### Files to Remove/Modify

#### Step 6.1: Remove hot_reload.rs
Delete the file:
```bash
rm tcgui-backend/src/config/hot_reload.rs
```

#### Step 6.2: Update config/mod.rs
Remove the hot_reload module declaration from `tcgui-backend/src/config/mod.rs`:

**Before:**
```rust
mod hot_reload;
pub use hot_reload::*;
```

**After:**
```rust
// Remove hot_reload references
```

#### Step 6.3: Remove any imports
Search for and remove any imports of hot_reload types in other files:
```bash
grep -r "hot_reload" tcgui-backend/src/
```

#### Step 6.4: Clean up unused dependencies
Check if any dependencies were only used by hot_reload and remove them from `Cargo.toml`:
- Check for `notify` (file watcher)
- Check for any HTTP client dependencies used only for config fetching

### Verification
```bash
just check       # Compile check
just clippy      # Lint check  
just test-backend  # All tests pass

# Verify no references remain
grep -r "hot_reload" tcgui-backend/
grep -r "HotReload" tcgui-backend/
```

---

## Appendix: Execution Order

### Recommended Implementation Order

1. **Replace debug println with tracing** (1-2 hours)
   - Quick win, immediate code quality improvement
   - No dependencies on other tasks

2. **Implement publisher cleanup** (3-4 hours)
   - Fixes memory leak
   - Can be done before or after main.rs refactoring

3. **Migrate to structured TC config** (4-6 hours)
   - Reduces complexity in query handler
   - Should be done before main.rs split for cleaner extraction

4. **Split main.rs into modules** (1-2 days)
   - Major refactoring
   - Easier after TC config migration
   - Enables better testing

5. **Replace polling with netlink events** (2-3 days)
   - Performance improvement
   - Requires clean module structure from step 4

6. **Remove hot reload feature** (1-2 hours)
   - Code cleanup
   - Can be done independently at any point

### Testing Strategy

After each task:
```bash
just dev          # Full development cycle
just pre-commit   # Pre-commit checks
```

After all tasks:
```bash
just coverage     # Verify test coverage maintained
```
