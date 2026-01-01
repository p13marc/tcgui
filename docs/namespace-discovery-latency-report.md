# Namespace Discovery Latency Report

## Problem Statement

When network namespaces are created with `ip netns add`, there can be significant delay (up to 60 seconds) before tcgui-backend discovers them. This is caused by a race condition related to the `/var/run/netns/` directory lifecycle.

## Root Cause Analysis

### The `/var/run/netns/` Directory Lifecycle

1. **On fresh boot**: The `/var/run/netns/` directory typically **does not exist**
2. **First namespace creation**: When `ip netns add foo` is run:
   - The kernel creates `/var/run/netns/` directory (if missing)
   - Creates `/var/run/netns/foo` bind mount for the namespace
3. **Last namespace deletion**: When the last namespace is deleted with `ip netns del`:
   - The directory may be left empty (or removed on some systems)

### Current tcgui-backend Behavior

In `tcgui-backend/src/main.rs:349-358`:

```rust
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
```

In `tcgui-backend/src/namespace_watcher.rs:32-36`:

```rust
let netns_path = Path::new("/var/run/netns");

// Check if the directory exists
if !netns_path.exists() {
    warn!("/var/run/netns does not exist, namespace watching disabled");
    return None;
}
```

### The Problem

1. Backend starts before any namespaces exist
2. `/var/run/netns/` doesn't exist yet
3. `NamespaceWatcher::new()` returns `None`
4. Backend falls back to **60-second polling interval**
5. User runs `ip netns add foo`
6. Namespace is created but not detected for up to 60 seconds

## Proposed Solutions

### Solution 1: Watch Parent Directory `/var/run/` (Recommended - tcgui fix)

Watch `/var/run/` for creation of the `netns` subdirectory, then dynamically start watching `/var/run/netns/`.

**Pros:**
- Works on fresh boots
- No nlink changes required
- Immediate detection of first namespace

**Cons:**
- More complex watch logic
- `/var/run/` is busy directory (many events to filter)

**Implementation in tcgui-backend:**

```rust
pub struct NamespaceWatcher {
    _parent_watcher: Option<RecommendedWatcher>,
    _netns_watcher: Option<RecommendedWatcher>,
}

impl NamespaceWatcher {
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<NamespaceEvent>) {
        let (tx, rx) = mpsc::channel(buffer_size);
        
        let netns_path = Path::new("/var/run/netns");
        
        if netns_path.exists() {
            // Directory exists, watch it directly
            let watcher = Self::create_netns_watcher(tx.clone());
            return (Self { _parent_watcher: None, _netns_watcher: watcher }, rx);
        }
        
        // Directory doesn't exist - watch /var/run/ for its creation
        let tx_clone = tx.clone();
        let parent_watcher = Self::create_parent_watcher(move |event| {
            if Self::is_netns_creation(event) {
                // /var/run/netns was created, now watch it
                // (would need interior mutability or message passing to switch)
            }
        });
        
        (Self { _parent_watcher: parent_watcher, _netns_watcher: None }, rx)
    }
}
```

### Solution 2: Create Directory at Backend Startup (Simple - tcgui fix)

Pre-create `/var/run/netns/` at backend startup if it doesn't exist.

**Pros:**
- Simple one-line fix
- Works immediately

**Cons:**
- Requires CAP_DAC_OVERRIDE or root to create in `/var/run/`
- May conflict with system namespace management
- Not a clean solution

**Implementation:**

```rust
// At backend startup
std::fs::create_dir_all("/var/run/netns").ok();
```

### Solution 3: Reduce Polling Interval (Workaround - tcgui fix)

Reduce the fallback polling interval from 60s to something more reasonable (e.g., 5s).

**Pros:**
- No architectural changes
- Simple configuration change

**Cons:**
- Still has latency (up to 5s)
- More CPU usage from polling
- Doesn't fix the root cause

**Implementation in `main.rs:362`:**

```rust
// Current: 60 seconds
let mut namespace_monitor_interval = interval(Duration::from_secs(60));

// Proposed: 5 seconds when inotify unavailable
let poll_interval = if namespace_events.is_some() { 60 } else { 5 };
let mut namespace_monitor_interval = interval(Duration::from_secs(poll_interval));
```

### Solution 4: Add Namespace Watcher to nlink (Best Long-term - nlink enhancement)

Add a `NamespaceWatcher` to nlink that handles all the complexity internally.

**Pros:**
- Reusable by other nlink users
- Consistent with nlink's "batteries included" philosophy
- Can implement optimal strategy (parent watch + netns watch)
- Could also monitor `/proc/*/ns/net` for container namespaces

**Cons:**
- Adds dependency on `notify` crate to nlink
- More scope creep for nlink

**Proposed nlink API:**

```rust
// In nlink::netlink::namespace

/// Events from namespace watcher
pub enum NamespaceEvent {
    /// A named namespace was created (/var/run/netns/*)
    Created { name: String },
    /// A named namespace was deleted
    Deleted { name: String },
    /// The netns directory was created (useful for knowing when to start detailed watching)
    DirectoryCreated,
}

/// Watch for namespace changes
pub struct NamespaceWatcher { ... }

impl NamespaceWatcher {
    /// Create a new watcher that handles /var/run/netns lifecycle
    pub fn new() -> Result<(Self, mpsc::Receiver<NamespaceEvent>)>;
    
    /// List current namespaces and start watching for changes
    pub fn list_and_watch() -> Result<(Vec<String>, Self, mpsc::Receiver<NamespaceEvent>)>;
}
```

### Solution 5: Use Netlink NETNS Events (Best - nlink enhancement)

Linux kernel can send netlink events for namespace operations via `NETLINK_KOBJECT_UEVENT` or `RTM_NEWNSID`/`RTM_DELNSID` messages.

**Pros:**
- No filesystem watching needed
- Kernel-native solution
- Works regardless of `/var/run/netns/` existence

**Cons:**
- `RTM_*NSID` requires namespace ID tracking
- More complex netlink message handling
- May require newer kernel versions

**Proposed nlink API:**

```rust
// Listen for namespace netlink events
let conn = Connection::new(Protocol::Route)?;
conn.subscribe_namespace_events().await?;

while let Some(event) = conn.recv_namespace_event().await {
    match event {
        NamespaceNetlinkEvent::NewNsId { nsid, pid } => { ... }
        NamespaceNetlinkEvent::DelNsId { nsid } => { ... }
    }
}
```

## Recommendation

### Short-term (tcgui fix)

Implement **Solution 3** (reduce polling interval) as an immediate fix:

```rust
// In main.rs, use shorter polling when inotify is unavailable
let poll_interval = if namespace_events.is_some() { 60 } else { 5 };
let mut namespace_monitor_interval = interval(Duration::from_secs(poll_interval));
```

This provides acceptable latency (5s max) with minimal code changes.

### Medium-term (tcgui improvement)

Implement **Solution 1** (watch parent directory) for immediate namespace detection even on fresh boots:

- Watch `/var/run/` for `netns` directory creation
- Dynamically start watching `/var/run/netns/` when it appears
- Handle directory deletion gracefully

### Long-term (nlink enhancement)

Implement **Solution 4** (nlink NamespaceWatcher) or **Solution 5** (netlink events):

Add a proper `NamespaceWatcher` to nlink that:
1. Handles the `/var/run/netns/` lifecycle transparently
2. Provides a simple API for consumers
3. Could optionally use netlink events when available

This would benefit all nlink users and provide the cleanest solution.

## Files Affected

### For Solution 3 (short-term fix)
- `tcgui-backend/src/main.rs` - Adjust polling interval

### For Solution 1 (medium-term fix)
- `tcgui-backend/src/namespace_watcher.rs` - Rewrite to watch parent directory

### For Solution 4/5 (nlink enhancement)
- `nlink/src/netlink/namespace.rs` - Add `NamespaceWatcher`
- `nlink/src/netlink/mod.rs` - Export new types
- `nlink/Cargo.toml` - Add `notify` dependency (for Solution 4)

## Testing Strategy

1. **Fresh boot test**: Start backend before any namespaces exist
2. **Create first namespace**: `ip netns add test1` - should be detected quickly
3. **Create second namespace**: `ip netns add test2` - should be detected immediately
4. **Delete namespaces**: `ip netns del test1` - should be detected
5. **Delete last namespace**: `ip netns del test2` - verify no crash/error
6. **Recreate after deletion**: `ip netns add test3` - should work

## References

- Linux namespaces: `man 7 namespaces`
- iproute2 netns: `man 8 ip-netns`
- Netlink RTM_*NSID: Linux kernel `include/uapi/linux/rtnetlink.h`
- notify crate: https://docs.rs/notify
