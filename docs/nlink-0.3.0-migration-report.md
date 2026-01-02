# nlink 0.3.0 Migration Report

This document summarizes the changes made to tcgui to take advantage of nlink 0.3.0 features, based on the feature requests in `docs/nlink-feature-requests.md`.

## Overview

| Feature Request | Priority | Status | Impact |
|-----------------|----------|--------|--------|
| Netem parameter detection API | High | ✅ Implemented | Eliminates fragile string parsing |
| Unified namespace connection API | High | ✅ Implemented | Reduces code duplication |
| Qdisc reset/atomic update | High | ✅ Implemented | Simplifies 60+ lines of workaround code |
| Rate unit conversion helpers | Low | ✅ Implemented | Cleaner, less error-prone code |
| Multi-namespace event stream | Medium | ⏸️ Deferred | Current implementation works well |
| Convenience TC query methods | Medium | N/A | Not yet available in nlink 0.3.0 |
| Granular error types | Medium | N/A | Not yet available in nlink 0.3.0 |
| StatsTracker rate caching | Low | N/A | Not yet available in nlink 0.3.0 |

## Changes Made

### 1. Dependency Update

**File:** `Cargo.toml`

```diff
- nlink = { version = "0.2.0", features = ["full"] }
+ nlink = { version = "0.3.0", features = ["full"] }
```

### 2. EventStream API Change

**File:** `tcgui-backend/src/netlink_events.rs`

nlink 0.3.0 changed `EventStream` to implement the standard `Stream` trait from `futures-core`. This required:

1. Adding the `StreamExt` import:
```rust
use futures_util::StreamExt;
```

2. Updating the event consumption pattern:
```rust
// Before (nlink 0.2.0)
loop {
    match stream.next().await {
        Ok(Some(event)) => { /* handle */ }
        Ok(None) => { break; }
        Err(e) => { /* error */ }
    }
}

// After (nlink 0.3.0)
while let Some(result) = stream.next().await {
    match result {
        Ok(event) => { /* handle */ }
        Err(e) => { /* error */ }
    }
}
```

### 3. Unified Namespace Connection API (NamespaceSpec)

**File:** `tcgui-backend/src/tc_commands.rs`

**Before:** Manual namespace type checking with separate connection creation paths:
```rust
fn create_connection(namespace: &str, namespace_path: Option<&Path>) -> Result<Connection, TcguiError> {
    if namespace == "default" {
        Connection::new(Protocol::Route).map_err(...)
    } else if Self::is_container_namespace(namespace) {
        if let Some(ns_path) = namespace_path {
            Connection::new_in_namespace_path(Protocol::Route, ns_path).map_err(...)
        } else {
            Err(...)
        }
    } else {
        namespace::connection_for(namespace).map_err(...)
    }
}
```

**After:** Using `NamespaceSpec` for unified handling:
```rust
fn namespace_spec<'a>(
    namespace: &'a str,
    namespace_path: Option<&'a Path>,
) -> Result<NamespaceSpec<'a>, TcguiError> {
    if namespace == "default" {
        Ok(NamespaceSpec::Default)
    } else if Self::is_container_namespace(namespace) {
        namespace_path
            .map(NamespaceSpec::Path)
            .ok_or_else(|| TcguiError::NetworkError { ... })
    } else {
        Ok(NamespaceSpec::Named(namespace))
    }
}

fn create_connection(namespace: &str, namespace_path: Option<&Path>) -> Result<Connection, TcguiError> {
    let spec = Self::namespace_spec(namespace, namespace_path)?;
    spec.connection().map_err(|e| TcguiError::NetworkError {
        message: format!("Failed to connect to namespace '{}': {}", namespace, e),
    })
}
```

### 4. Rate Conversion Utilities

**File:** `tcgui-backend/src/tc_commands.rs`

**Before:** Manual calculation prone to errors:
```rust
// Convert kbps to bytes per second
let bytes_per_sec = (config.rate_limit.rate_kbps as u64) * 1000 / 8;
netem = netem.rate(bytes_per_sec);
```

**After:** Using nlink's rate conversion:
```rust
use nlink::util::rate;

netem = netem.rate(rate::kbps_to_bytes(config.rate_limit.rate_kbps.into()));
```

### 5. NetemOptions Parameter Detection and Qdisc Recreation

**File:** `tcgui-backend/src/tc_commands.rs`

This was the most significant improvement. The old code used fragile string parsing and complex manual logic:

**Before (60+ lines of workaround code):**
```rust
// String-based parsing
fn parse_current_tc_config(&self, qdisc_info: &str) -> CurrentTcConfig {
    CurrentTcConfig {
        has_loss: qdisc_info.contains("loss"),
        has_delay: qdisc_info.contains("delay"),
        has_duplicate: qdisc_info.contains("duplicate"),
        // ... more string matching
    }
}

// Manual recreation decision with 12+ parameters
fn needs_qdisc_recreation(
    &self,
    current: &CurrentTcConfig,
    loss: f32,
    correlation: Option<f32>,
    delay_ms: Option<f32>,
    // ... 10 more parameters
) -> bool {
    let will_remove_loss = current.has_loss && loss <= 0.0;
    let will_remove_delay = current.has_delay && delay_ms.is_none_or(|d| d <= 0.0);
    // ... complex logic for each parameter
    will_remove_loss || will_remove_delay || /* 6 more conditions */
}

// Usage in apply_tc_config_structured_with_path:
let existing_qdisc = self.check_existing_qdisc_with_path(...).await.unwrap_or_default();
if existing_qdisc.contains("netem") {
    let (loss, correlation, delay_ms, /* 10 more */) = config.to_legacy_params();
    let current_config = self.parse_current_tc_config(&existing_qdisc);
    let needs_recreation = self.needs_qdisc_recreation(&current_config, loss, /* ... */);
    // ...
}
```

**After (using nlink's typed API):**
```rust
// Get typed NetemOptions directly from the kernel
let existing_netem = self
    .get_netem_options_with_path(namespace, namespace_path, interface)
    .await
    .ok()
    .flatten();

match existing_netem {
    Some(current_opts) => {
        // nlink handles all the recreation logic internally
        if current_opts.requires_recreation_for(&netem_config) {
            // Delete and add
            let _ = conn.del_qdisc_by_index(ifindex, "root").await;
            conn.add_qdisc_by_index(ifindex, netem_config).await?;
        } else {
            // Simple replace
            conn.replace_qdisc_by_index(ifindex, netem_config).await?;
        }
    }
    None => {
        // No existing netem - add new or replace other qdisc
    }
}
```

## Deferred: MultiNamespaceEventStream

**Reason:** The current `NamespaceEventManager` implementation is well-integrated with the main event loop architecture using `mpsc` channels. While nlink's `MultiNamespaceEventStream` provides similar functionality natively, replacing it would require:

1. Refactoring the main event loop's `tokio::select!` pattern
2. Changing from channel-based to direct stream consumption
3. Updating event parsing and dispatch logic

The current implementation works correctly and the EventStream fixes have already been applied. This could be revisited in a future refactoring effort if desired.

## Verification

All changes have been verified:

```bash
# Compilation
cargo check                    # ✅ Pass

# Tests  
cargo test --lib               # ✅ 56 tests passed

# Linting
cargo clippy --all-targets -- -D warnings  # ✅ No warnings
```

## Files Modified

| File | Changes |
|------|---------|
| `Cargo.toml` | Updated nlink version |
| `tcgui-backend/src/netlink_events.rs` | Added StreamExt import, updated event loop patterns |
| `tcgui-backend/src/tc_commands.rs` | Added NamespaceSpec, rate utilities, NetemOptions.requires_recreation_for() |

## Benefits Summary

1. **Type Safety:** Replaced string parsing with typed `NetemOptions` API
2. **Reduced Code:** Eliminated 60+ lines of manual parameter checking
3. **Maintainability:** nlink handles kernel-specific logic for qdisc recreation
4. **Correctness:** Rate conversions use tested library functions
5. **Unified API:** NamespaceSpec provides consistent namespace handling

## nlink Features Used

From nlink 0.3.0:

- `nlink::netlink::namespace::NamespaceSpec` - Unified namespace specification
- `NamespaceSpec::connection()` - Create connection for any namespace type
- `nlink::netlink::tc_options::NetemOptions::requires_recreation_for()` - Determine if qdisc needs delete+add
- `nlink::util::rate::kbps_to_bytes()` - Rate unit conversion
- `EventStream` implementing `Stream` trait - Standard async iteration
