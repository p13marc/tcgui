# nlink Migration Plan

This document outlines the plan to migrate tcgui-backend from `rtnetlink` crate and process spawning (`Command::new`) to the `nlink` library for all network operations.

## Executive Summary

**Goal**: Replace all process spawning (`tc`, `ip` commands) and `rtnetlink` crate usage with `nlink` library calls for a pure Rust, zero-fork network management solution.

**Risk Level**: High - This touches core networking functionality across the entire backend.

**Branch**: `feature/nlink-migration`

**Status**: **READY TO START MIGRATION** - All nlink enhancements are complete including typed TC API.

## Current State Analysis

### Process Spawning Locations (23 total)

| File | Count | Commands Spawned |
|------|-------|------------------|
| `tc_commands.rs` | 10 | `tc`, `ip netns exec`, `nsenter` |
| `network.rs` | 2 | `tc qdisc show` |
| `container.rs` | 2 | `nsenter`, `docker/podman exec` |
| `commands/builder.rs` | 3 | `tc`, `ip`, `sudo` |

### rtnetlink Usage Locations

| File | Purpose |
|------|---------|
| `network.rs` | Interface discovery, state management (up/down) |
| `netlink_events.rs` | Real-time interface change monitoring |
| `main.rs` | Session initialization |

## nlink Capabilities Assessment (Final)

### All Required Features Now Available

| Feature | nlink API | Status |
|---------|-----------|--------|
| Interface listing | `conn.get_links()` | **Ready** |
| Interface by name/index | `conn.get_link_by_name()`, `get_link_by_index()` | **Ready** |
| Interface state (up/down) | `conn.set_link_up()`, `set_link_down()`, `set_link_state()` | **Ready** |
| State by index | `conn.set_link_up_by_index()`, `set_link_down_by_index()` | **Ready** |
| Link MTU | `conn.set_link_mtu()`, `set_link_mtu_by_index()` | **Ready** |
| Link deletion | `conn.del_link()`, `del_link_by_index()` | **Ready** |
| Link statistics | `LinkMessage.stats` (rx/tx bytes, packets, errors) | **Ready** |
| **Namespace-aware connection** | `Connection::new_in_namespace_path(path)` | **Ready** |
| **Namespace-aware EventStream** | `.namespace()`, `.namespace_path()`, `.namespace_pid()` | **Ready** |
| Event monitoring | `EventStream` with `NewLink`, `DelLink`, TC events | **Ready** |
| Qdisc listing | `conn.get_qdiscs()`, `get_qdiscs_for()` | **Ready** |
| **Typed qdisc operations** | `conn.add_qdisc()`, `replace_qdisc()`, `del_qdisc()` | **Ready** |
| **Typed qdisc by index** | `conn.add_qdisc_by_index()`, `replace_qdisc_by_index()` | **Ready** |
| **Typed NetemConfig** | `NetemConfig::new().delay().loss().build()` | **Ready** |
| TC classes/filters | `conn.get_classes()`, `get_filters()` | **Ready** |
| Statistics tracking | `StatsSnapshot`, `StatsTracker`, `LinkRates` | **Ready** |
| Thread safety | `Connection` is `Send + Sync` | **Ready** |

### No Remaining Gaps

All previously identified gaps have been addressed:
- ✅ Namespace-aware Connection
- ✅ Namespace-aware EventStream
- ✅ Typed TC API with `NetemConfig`
- ✅ TC operations by interface index (`*_by_index` methods)

## Migration Phases

### Phase 0: nlink Enhancements - COMPLETE

All enhancements have been implemented.

#### Namespace-Aware Connection

```rust
// By path
let conn = Connection::new_in_namespace_path(Protocol::Route, "/var/run/netns/myns")?;
let conn = Connection::new_in_namespace_path(Protocol::Route, "/proc/1234/ns/net")?;
```

#### Namespace-Aware EventStream

```rust
let mut stream = EventStream::builder()
    .namespace("myns")                      // Named namespace
    .namespace_path("/proc/1234/ns/net")    // By path
    .namespace_pid(container_pid)           // By PID
    .links(true)
    .tc(true)
    .build()?;
```

#### Typed TC API with NetemConfig

```rust
use nlink::netlink::tc::NetemConfig;
use std::time::Duration;

// Build typed netem configuration
let netem = NetemConfig::new()
    .delay(Duration::from_millis(100))
    .jitter(Duration::from_millis(10))
    .delay_correlation(25.0)
    .loss(5.0)
    .loss_correlation(10.0)
    .duplicate(1.0)
    .corrupt(0.5)
    .reorder(2.0)
    .rate_bps(1_000_000)  // 1 Mbps
    .build();

// Add qdisc by interface name
conn.add_qdisc("eth0", netem.clone()).await?;

// Or by interface index (for namespace-aware operations)
let link = conn.get_link_by_name("eth0").await?.unwrap();
conn.add_qdisc_by_index(link.ifindex(), netem).await?;

// Replace existing qdisc
conn.replace_qdisc("eth0", netem).await?;

// Delete qdisc
conn.del_qdisc("eth0", "root").await?;
```

### Phase 1: Interface Discovery Migration

**Files affected**: `network.rs`

```rust
// OLD (rtnetlink)
let (connection, handle, _) = rtnetlink::new_connection()?;
tokio::spawn(connection);
let mut links = handle.link().get().execute();

// NEW (nlink)
let conn = Connection::new(Protocol::Route)?;
let links = conn.get_links().await?;
for link in links {
    let index = link.ifindex();
    let name = link.name.as_deref();
    let is_up = link.is_up();
    let kind = link.kind();
}
```

**For namespaced discovery**:

```rust
// NEW - namespace-aware, no manual setns
let conn = Connection::new_in_namespace_path(Protocol::Route, &ns_path)?;
let links = conn.get_links().await?;
```

### Phase 2: Interface State Management Migration

**Files affected**: `network.rs`

```rust
// OLD (rtnetlink)
let message = LinkMessageBuilder::<LinkUnspec>::new()
    .index(index)
    .up()
    .build();
handle.link().set(message).execute().await?;

// NEW (nlink) - one-liner!
conn.set_link_up_by_index(index).await?;
conn.set_link_down_by_index(index).await?;
```

### Phase 3: Event Monitoring Migration

**Files affected**: `netlink_events.rs`

```rust
// OLD (manual netlink)
let (mut connection, _handle, mut messages) = new_connection()?;
let addr = SocketAddr::new(0, RTMGRP_LINK);
connection.socket_mut().socket_mut().bind(&addr)?;

// NEW (nlink EventStream)
let mut stream = EventStream::builder()
    .links(true)
    .tc(true)
    .build()?;

while let Some(event) = stream.next().await? {
    match event {
        NetworkEvent::NewLink(link) => { /* added/changed */ }
        NetworkEvent::DelLink(link) => { /* removed */ }
        NetworkEvent::NewQdisc(tc) => { /* qdisc added */ }
        _ => {}
    }
}
```

**For namespace monitoring**:

```rust
let mut stream = EventStream::builder()
    .namespace_path("/proc/1234/ns/net")
    .links(true)
    .tc(true)
    .build()?;
```

### Phase 4: TC Command Migration (Core) - SIMPLIFIED WITH TYPED API

**Files affected**: `tc_commands.rs`, `commands/builder.rs`

```rust
use nlink::netlink::tc::NetemConfig;
use std::time::Duration;

/// Convert tcgui's TcNetemConfig to nlink's NetemConfig
fn to_nlink_netem(config: &TcNetemConfig) -> NetemConfig {
    let mut netem = NetemConfig::new();
    
    if let Some(delay) = config.delay_ms {
        netem = netem.delay(Duration::from_millis(delay as u64));
    }
    if let Some(jitter) = config.jitter_ms {
        netem = netem.jitter(Duration::from_millis(jitter as u64));
    }
    if let Some(corr) = config.delay_correlation {
        netem = netem.delay_correlation(corr);
    }
    if let Some(loss) = config.loss {
        netem = netem.loss(loss);
    }
    if let Some(corr) = config.loss_correlation {
        netem = netem.loss_correlation(corr);
    }
    if let Some(dup) = config.duplicate {
        netem = netem.duplicate(dup);
    }
    if let Some(corr) = config.duplicate_correlation {
        netem = netem.duplicate_correlation(corr);
    }
    if let Some(corrupt) = config.corrupt {
        netem = netem.corrupt(corrupt);
    }
    if let Some(corr) = config.corrupt_correlation {
        netem = netem.corrupt_correlation(corr);
    }
    if let Some(reorder) = config.reorder {
        netem = netem.reorder(reorder);
    }
    if let Some(corr) = config.reorder_correlation {
        netem = netem.reorder_correlation(corr);
    }
    if let Some(rate) = config.rate_kbit {
        netem = netem.rate_bps((rate as u64) * 1000);
    }
    
    netem.build()
}

/// Apply TC configuration (namespace-aware)
async fn apply_tc_config(
    namespace: &str,
    namespace_path: Option<&Path>,
    interface: &str,
    config: &TcNetemConfig,
) -> Result<()> {
    let conn = get_connection_for_namespace(namespace, namespace_path)?;
    let netem = to_nlink_netem(config);
    
    // Use by_index for namespace safety
    let link = conn.get_link_by_name(interface).await?
        .ok_or_else(|| Error::NotFound(interface.to_string()))?;
    
    conn.replace_qdisc_by_index(link.ifindex(), netem).await?;
    Ok(())
}

/// Remove TC configuration
async fn remove_tc_config(
    namespace: &str,
    namespace_path: Option<&Path>,
    interface: &str,
) -> Result<()> {
    let conn = get_connection_for_namespace(namespace, namespace_path)?;
    
    let link = conn.get_link_by_name(interface).await?
        .ok_or_else(|| Error::NotFound(interface.to_string()))?;
    
    conn.del_qdisc_by_index(link.ifindex(), "root").await?;
    Ok(())
}
```

### Phase 5: TC Query Migration

**Files affected**: `network.rs`

```rust
// OLD (process spawning)
let output = Command::new("tc")
    .args(["qdisc", "show", "dev", interface])
    .output().await?;
let has_netem = String::from_utf8_lossy(&output.stdout).contains("netem");

// NEW (nlink)
let conn = get_connection_for_namespace(namespace, namespace_path)?;
let qdiscs = conn.get_qdiscs_for(interface).await?;
let has_netem = qdiscs.iter().any(|q| q.kind() == Some("netem"));
```

### Phase 6: Container Namespace Handling

**Files affected**: `container.rs`

```rust
// OLD
let mut cmd = Command::new("nsenter");
cmd.arg(format!("--net={}", ns_path.display()));
cmd.arg("tc").args(["qdisc", "show", "dev", interface]);

// NEW
let conn = Connection::new_in_namespace_path(Protocol::Route, &ns_path)?;
let link = conn.get_link_by_name(interface).await?;
conn.replace_qdisc_by_index(link.ifindex(), netem).await?;
```

**Note**: Keep `docker exec` / `podman exec` as fallback only for containers where `/proc/<pid>/ns/net` isn't accessible.

### Phase 7: Bandwidth Monitoring Migration

**Files affected**: `bandwidth.rs`

```rust
use nlink::netlink::stats::{StatsSnapshot, StatsTracker};

let conn = get_connection_for_namespace(namespace, namespace_path)?;
let mut tracker = StatsTracker::new();

loop {
    let links = conn.get_links().await?;
    let snapshot = StatsSnapshot::from_links(&links);
    
    if let Some(rates) = tracker.update(snapshot) {
        for (ifindex, link_rates) in &rates.links {
            let rx_mbps = link_rates.rx_bps() / 1_000_000.0;
            let tx_mbps = link_rates.tx_bps() / 1_000_000.0;
        }
    }
    
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

## Dependency Changes

### Remove from Cargo.toml

```toml
# tcgui-backend/Cargo.toml - Remove these
rtnetlink = "0.14"
netlink-packet-route = "0.19"
futures-util = "0.3"  # if only used for rtnetlink
```

### Add to Cargo.toml

```toml
# tcgui-backend/Cargo.toml - Add
nlink = { git = "https://github.com/p13marc/nlink", features = ["full"] }
```

## Helper Function: Get Connection for Namespace

```rust
use nlink::netlink::{Connection, Protocol, Result};
use std::path::Path;

/// Get a netlink connection for the specified namespace.
pub fn get_connection_for_namespace(
    namespace: &str,
    namespace_path: Option<&Path>,
) -> Result<Connection> {
    if namespace == "default" {
        Connection::new(Protocol::Route)
    } else if let Some(ns_path) = namespace_path {
        Connection::new_in_namespace_path(Protocol::Route, ns_path)
    } else {
        let ns_path = format!("/var/run/netns/{}", namespace);
        Connection::new_in_namespace_path(Protocol::Route, &ns_path)
    }
}
```

## Recommended Pattern for Namespace-Aware TC

```rust
// Safe, no threads, namespace-correct
let conn = get_connection_for_namespace("container_ns", Some(ns_path))?;
let link = conn.get_link_by_name("eth0").await?.unwrap();
conn.add_qdisc_by_index(link.ifindex(), netem).await?;
```

**Why this works**:
1. The namespace connection uses a brief `setns()` only during socket creation
2. The resulting socket is safely bound to the target namespace
3. Using `*_by_index` avoids any `/sys/class/net/` lookups
4. No threading concerns - it's all single-threaded

## Testing Strategy

### Unit Tests

1. **TcNetemConfig conversion**: Verify tcgui config -> nlink NetemConfig conversion
2. **Namespace path handling**: Test traditional vs container namespace detection
3. **Error mapping**: Verify nlink errors map correctly to `TcguiError`

### Integration Tests

1. **Default namespace TC operations**: Apply/remove netem in default namespace
2. **Named namespace TC operations**: Create test namespace, apply TC, verify, cleanup
3. **Interface discovery**: Compare output with `ip link show`
4. **Qdisc detection**: Compare with `tc qdisc show`
5. **Interface state**: Toggle up/down, verify state changes
6. **Event monitoring**: Verify events received for interface changes

### Manual Testing Checklist

- [ ] Frontend shows all interfaces correctly
- [ ] TC configuration applies successfully (all parameters)
- [ ] TC removal works correctly
- [ ] Namespace interfaces discovered
- [ ] Container interfaces discovered (if Docker/Podman available)
- [ ] Interface up/down toggle works
- [ ] Real-time bandwidth stats work
- [ ] Interface events trigger UI updates
- [ ] Scenarios work correctly (multi-step TC changes)

## Rollback Plan

If critical issues are discovered:

1. Keep the `feature/nlink-migration` branch for reference
2. Return to `master` branch
3. Document specific failures for nlink improvements

## Updated Timeline Estimate

| Phase | Effort | Status |
|-------|--------|--------|
| Phase 0 (nlink enhancements) | - | **COMPLETE** |
| Phase 1 (Interface discovery) | 0.5 day | Ready to start |
| Phase 2 (Interface state) | 0.5 day | Ready |
| Phase 3 (Event monitoring) | 0.5 day | Ready |
| Phase 4 (TC commands) | 1 day | Ready (simplified with typed API) |
| Phase 5 (TC queries) | 0.5 day | Ready |
| Phase 6 (Containers) | 0.5 day | Ready |
| Phase 7 (Bandwidth) | 0.5 day | Ready |
| Testing & fixes | 1.5 days | - |

**Total remaining**: ~5.5 days of development (reduced from 7 days thanks to typed API)

## Success Criteria

1. Zero `Command::new` calls for network operations (except container exec fallback)
2. Zero `rtnetlink` crate usage
3. All existing functionality preserved
4. All tests passing
5. No performance regression in interface discovery or TC operations
