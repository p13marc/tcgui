# Multi-Namespace Event Stream: Architecture Recommendations

This report analyzes the `multi-namespace-event-stream-comparison.md` findings and proposes improvements to both tcgui's architecture and potential nlink abstractions.

## Executive Summary

The comparison document correctly recommends keeping the master implementation over the feature branch. However, there are opportunities to improve both sides:

| Area | Current State | Recommendation |
|------|---------------|----------------|
| tcgui architecture | Good, some complexity | Minor refactoring possible |
| nlink abstractions | Adequate | Several improvements would help |
| MultiNamespaceEventStream | Not worth adopting as-is | Needs API redesign in nlink |

## Analysis of the Core Problem

### Why the Feature Branch Failed

The feature branch attempted to use nlink's `MultiNamespaceEventStream` but introduced:

1. **Busy-loop pattern** with 100ms sleep when no namespaces are monitored
2. **Race conditions** in `is_monitoring()` using `try_lock()`
3. **Lost functionality** (`remove_namespace()` was removed)
4. **Increased complexity** (Arc<Mutex<>> + command channel)

The root cause: `MultiNamespaceEventStream` was designed as a standalone stream, not as a manageable component that integrates well with existing event loops.

### What the Master Implementation Does Right

```
┌─────────────────────────────────────────────┐
│           NamespaceEventManager             │
├─────────────────────────────────────────────┤
│ - event_tx: mpsc::Sender                    │
│ - active_namespaces: HashMap<String, JoinHandle> │
└─────────────────────────────────────────────┘
         │
         ├──► Task 1: process_namespace_events(stream1, "ns1")
         ├──► Task 2: process_namespace_events(stream2, "ns2")
         └──► Task N: process_namespace_events(streamN, "nsN")
```

Strengths:
- **Simple state management**: HashMap with JoinHandles
- **Non-blocking queries**: `is_monitoring()` is O(1)
- **Full lifecycle control**: add/remove/drop all work correctly
- **Isolated failures**: One namespace's stream error doesn't affect others

---

## Recommendations for tcgui

### Option 1: Keep Current Architecture (Recommended)

The current `NamespaceEventManager` is 96 lines of well-structured code that:
- Works correctly
- Integrates cleanly with the main event loop
- Supports all required operations

**No changes needed** unless specific pain points emerge.

### Option 2: Minor Refactoring - Unify Event Handling

Currently, the main event loop has separate branches for default and container namespaces:

```rust
// main.rs - current
tokio::select! {
    Some(event) = netlink_events.recv() => {
        // Default namespace events via NetlinkEventListener
    }
    Some(ns_event) = namespace_netlink_events.recv() => {
        // Container namespace events via NamespaceEventManager
    }
}
```

This could be unified by having `NamespaceEventManager` also handle the default namespace:

```rust
// Proposed change
impl NamespaceEventManager {
    pub fn new_with_default(buffer_size: usize) -> Result<(Self, mpsc::Receiver<NamespacedEvent>), String> {
        let (mut manager, rx) = Self::new(buffer_size);
        manager.add_namespace(NamespaceTarget::Default)?;
        Ok((manager, rx))
    }
}

// main.rs - simplified
tokio::select! {
    Some(ns_event) = namespace_netlink_events.recv() => {
        // All namespace events (including default) via NamespaceEventManager
    }
}
```

**Impact**: Reduces one branch in `tokio::select!`, eliminates `NetlinkEventListener` struct.

### Option 3: Direct Stream Consumption (Not Recommended)

Using `MultiNamespaceEventStream` directly in `main.rs` without a wrapper:

```rust
let mut multi_stream = MultiNamespaceEventStream::new();

loop {
    tokio::select! {
        Some((namespace, event)) = multi_stream.next() => {
            // Handle directly
        }
        // Need separate channel/mechanism to add namespaces dynamically
    }
}
```

**Problems**:
- Cannot add namespaces from within the event loop (needs separate command channel anyway)
- Loses the clean abstraction boundary
- More coupling to nlink internals

---

## Recommendations for nlink

### 1. Redesign MultiNamespaceEventStream API

The current API requires external management of the stream. A better design:

```rust
pub struct MultiNamespaceEventStream {
    streams: StreamMap<String, EventStream>,
    // No internal task - caller owns the polling
}

impl MultiNamespaceEventStream {
    /// Create empty stream manager
    pub fn new() -> Self;
    
    /// Add a namespace (can be called between polls)
    pub fn add(&mut self, name: impl Into<String>, spec: NamespaceSpec<'_>) -> Result<()>;
    
    /// Remove a namespace
    pub fn remove(&mut self, name: &str) -> bool;
    
    /// Check if a namespace is being monitored
    pub fn contains(&self, name: &str) -> bool;
    
    /// Get list of monitored namespaces
    pub fn namespaces(&self) -> impl Iterator<Item = &str>;
    
    /// Check if any namespaces are being monitored
    pub fn is_empty(&self) -> bool;
}

impl Stream for MultiNamespaceEventStream {
    type Item = (String, Result<NetworkEvent, Error>);
    
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
}
```

**Key differences from current design**:
- No internal spawned task
- Caller integrates into their own event loop
- Mutable access for add/remove between polls
- Implements `Stream` trait for standard integration

### 2. Add NamespacedEventStream Builder

For cases where users want a managed, channel-based solution:

```rust
pub struct NamespacedEventStreamBuilder {
    buffer_size: usize,
    initial_namespaces: Vec<(String, NamespaceSpec<'static>)>,
}

impl NamespacedEventStreamBuilder {
    pub fn new() -> Self;
    pub fn buffer_size(self, size: usize) -> Self;
    pub fn add_namespace(self, name: impl Into<String>, spec: NamespaceSpec<'static>) -> Self;
    
    /// Build and return (handle, receiver)
    /// Handle allows adding/removing namespaces
    /// Receiver yields (namespace, event) tuples
    pub fn build(self) -> (NamespaceStreamHandle, mpsc::Receiver<(String, NetworkEvent)>);
}

pub struct NamespaceStreamHandle {
    command_tx: mpsc::Sender<Command>,
}

impl NamespaceStreamHandle {
    pub async fn add(&self, name: impl Into<String>, spec: NamespaceSpec<'_>) -> Result<()>;
    pub async fn remove(&self, name: &str) -> Result<bool>;
    pub fn is_monitoring(&self, name: &str) -> bool;  // Needs internal state sync
}
```

This would essentially be nlink providing what tcgui currently implements manually.

### 3. Provide EventStream Builder Improvements

Currently, tcgui has to handle namespace specification separately:

```rust
// Current
let mut builder = EventStream::builder().links(true).tc(true);
builder = match &target {
    NamespaceTarget::Default => builder,
    NamespaceTarget::Named(name) => builder.namespace(name),
    NamespaceTarget::Path { path, .. } => builder.namespace_path(path),
};
```

Better:

```rust
// Proposed
let stream = EventStream::builder()
    .links(true)
    .tc(true)
    .namespace_spec(spec)  // Accept NamespaceSpec directly
    .build()?;
```

### 4. Add Convenience for Event Tagging

tcgui wraps events with namespace information:

```rust
pub struct NamespacedEvent {
    pub namespace: String,
    pub event: NetlinkEvent,
}
```

nlink could provide this:

```rust
impl EventStream {
    /// Create a stream that yields (namespace_name, event) tuples
    pub fn with_namespace_tag(self, name: impl Into<String>) -> TaggedEventStream;
}
```

---

## Implementation Priority

### For tcgui (effort vs. impact)

| Change | Effort | Impact | Priority |
|--------|--------|--------|----------|
| Keep as-is | None | Stable | Default |
| Unify default + container streams | Low | Minor simplification | Optional |
| Use future nlink abstractions | Medium | Depends on nlink | Wait |

### For nlink (suggestions for library improvement)

| Feature | Effort | Impact | Priority |
|---------|--------|--------|----------|
| `EventStreamBuilder::namespace_spec()` | Low | Quality of life | High |
| Redesigned `MultiNamespaceEventStream` | Medium | Better integration | Medium |
| `NamespacedEventStreamBuilder` | Medium | Eliminates boilerplate | Medium |
| `EventStream::with_namespace_tag()` | Low | Minor convenience | Low |

---

## Conclusion

1. **tcgui should keep the master implementation** - it works well and is maintainable.

2. **The feature branch correctly identified the goal** (single-task event processing) but **implemented it incorrectly** (busy loops, race conditions).

3. **nlink's current `MultiNamespaceEventStream`** isn't designed for the integration pattern tcgui needs. If nlink redesigns it as suggested above, tcgui could adopt it in the future.

4. **The best immediate improvement** for tcgui would be unifying default and container namespace handling into a single `NamespaceEventManager`, eliminating the separate `NetlinkEventListener`.

5. **For nlink**, the highest-value improvement would be accepting `NamespaceSpec` in the `EventStreamBuilder`, followed by a redesigned multi-namespace stream that implements the standard `Stream` trait without spawning internal tasks.

---

## Appendix: Current vs. Ideal Code Comparison

### Current tcgui Pattern

```rust
// netlink_events.rs (96 lines of management code)
pub struct NamespaceEventManager {
    event_tx: mpsc::Sender<NamespacedEvent>,
    active_namespaces: HashMap<String, JoinHandle<()>>,
}

impl NamespaceEventManager {
    pub fn add_namespace(&mut self, target: NamespaceTarget) -> Result<(), String> {
        // Build EventStream with namespace handling
        let mut builder = EventStream::builder().links(true).tc(true);
        builder = match &target {
            NamespaceTarget::Default => builder,
            NamespaceTarget::Named(name) => builder.namespace(name),
            NamespaceTarget::Path { path, .. } => builder.namespace_path(path),
        };
        let stream = builder.build()?;
        
        // Spawn task and track handle
        let handle = tokio::spawn(Self::process_namespace_events(stream, name, tx));
        self.active_namespaces.insert(name, handle);
        Ok(())
    }
}
```

### Ideal with Improved nlink

```rust
// With proposed nlink improvements
pub struct NamespaceEventManager {
    stream: MultiNamespaceEventStream,
}

impl NamespaceEventManager {
    pub fn add_namespace(&mut self, name: &str, spec: NamespaceSpec<'_>) -> Result<()> {
        self.stream.add(name, spec)
    }
    
    pub fn remove_namespace(&mut self, name: &str) -> bool {
        self.stream.remove(name)
    }
    
    pub async fn next(&mut self) -> Option<(String, NetworkEvent)> {
        self.stream.next().await
    }
}

// In main.rs event loop
tokio::select! {
    Some((namespace, event)) = namespace_manager.next() => {
        handle_event(namespace, event);
    }
    // Can call namespace_manager.add() or remove() from other branches
}
```

The key insight is that nlink should provide **composable building blocks** rather than **complete solutions**, allowing users like tcgui to integrate them into their specific architecture patterns.
