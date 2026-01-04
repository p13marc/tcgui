# nlink Integration Report for tcgui

**Date**: 2026-01-04  
**nlink Version**: 0.6.0  
**tcgui Version**: 0.5.0

## Executive Summary

This report documents tcgui's current integration with the nlink library, analyzes what nlink features are being used, identifies areas where nlink could be improved to better serve tcgui's needs, and proposes enhancements for both immediate and future development.

---

## Current nlink Usage in tcgui

### 1. Interface Discovery (`network.rs`)

**Usage:**
```rust
use nlink::netlink::{Connection, Route, namespace};

// Create connection for default namespace
let connection = Connection::<Route>::new()?;

// Get links
let links = connection.get_links().await?;

// For containers/named namespaces
let conn = Connection::<Route>::new_in_namespace_path(ns_path)?;
```

**Features Used:**
- `Connection::<Route>::new()` - Default namespace connection
- `Connection::<Route>::new_in_namespace_path()` - Namespace-aware connections
- `get_links()` - Interface enumeration
- `LinkMessage` accessors: `name_or()`, `ifindex()`, `is_up()`, `has_carrier()`, `mtu()`, `mac_address()`, `link_info()`

**Pain Points:**
- No built-in way to get all interfaces across all namespaces in one call
- Must manually iterate namespaces and create connections for each

---

### 2. Traffic Control (`tc_commands.rs`)

**Usage:**
```rust
use nlink::netlink::tc::NetemConfig;
use nlink::netlink::tc_options::{NetemOptions, QdiscOptions};
use nlink::util::rate;

// Build netem configuration
let netem = NetemConfig::default()
    .delay_ms(100)
    .delay_jitter_ms(10)
    .loss_percent(5.0)
    .rate(rate::kbps_to_bytes(1000));

// Apply to interface
connection.apply_netem(interface_name, netem).await?;

// Query existing qdiscs
let qdiscs = connection.get_qdiscs_for(interface_name).await?;
for qdisc in qdiscs {
    if let Some(QdiscOptions::Netem(opts)) = qdisc.options() {
        // Read current config
    }
}
```

**Features Used:**
- `NetemConfig` builder for delay, loss, duplicate, reorder, corrupt, rate
- `apply_netem()` / `apply_netem_by_index()` - Apply netem qdisc
- `get_qdiscs_for()` - Query existing qdiscs
- `QdiscOptions::Netem` - Parse netem options
- `NetemOptions` accessors: `delay()`, `jitter()`, `loss()`, `duplicate()`, `reorder()`, `corrupt()`, `rate_bps()`
- `rate::kbps_to_bytes()` - Rate unit conversion

**Pain Points:**
- Rate limiting uses raw bytes/sec internally; would prefer human-readable strings like "10mbit"
- No ingress rate limiting support (netem only does egress)
- `apply_netem()` only does replace, not add - have to handle "qdisc not found" error manually
- Cannot capture full TC state for rollback (only detect if netem exists)

---

### 3. Bandwidth Monitoring (`bandwidth.rs`)

**Usage:**
```rust
use nlink::netlink::stats::{LinkStats as NlinkLinkStats, StatsSnapshot, StatsTracker};
use nlink::netlink::{Connection, Route};

// Get link with stats
let links = connection.get_links().await?;
for link in links {
    if let Some(stats) = link.stats() {
        let rx_bytes = stats.rx_bytes();
        let tx_bytes = stats.tx_bytes();
    }
}

// Use StatsTracker for rate calculation
let mut tracker = StatsTracker::new();
tracker.update(snapshot);
if let Some(rates) = tracker.rates() {
    let rx_bps = rates.rx_bytes_per_sec;
    let tx_bps = rates.tx_bytes_per_sec;
}
```

**Features Used:**
- `LinkStats` accessors for rx/tx bytes, packets, errors, dropped
- `StatsTracker` for rate-of-change calculation
- `StatsSnapshot` for point-in-time stats

**Working Well:** This integration is solid. No major pain points.

---

### 4. Event Monitoring (`netlink_events.rs`)

**Usage:**
```rust
use nlink::RtnetlinkGroup;
use nlink::netlink::events::NetworkEvent;
use nlink::netlink::{Connection, Route};

// Subscribe to events
let mut conn = Connection::<Route>::new()?;
conn.subscribe(&[RtnetlinkGroup::Link, RtnetlinkGroup::Tc])?;

// Stream events
let mut events = conn.events();
while let Some(result) = events.next().await {
    match result? {
        NetworkEvent::NewLink(link) => { /* handle */ }
        NetworkEvent::DelLink(link) => { /* handle */ }
        NetworkEvent::NewQdisc(qdisc) => { /* handle */ }
        // ...
    }
}
```

**Features Used:**
- `RtnetlinkGroup` for subscription filtering
- `NetworkEvent` enum for typed events
- `events()` stream for async event consumption

**Pain Points:**
- No way to subscribe to events across multiple namespaces with a single stream
- Must spawn separate tasks per namespace and merge streams manually

---

### 5. Namespace Operations (`namespace_watcher.rs`, `diagnostics.rs`)

**Usage:**
```rust
use nlink::netlink::namespace_watcher::{NamespaceEvent, NamespaceWatcher};
use nlink::netlink::namespace;

// Watch for namespace changes
let mut watcher = NamespaceWatcher::new()?;
while let Some(event) = watcher.next().await {
    match event {
        NamespaceEvent::Added(name) => { /* handle */ }
        NamespaceEvent::Removed(name) => { /* handle */ }
    }
}

// Create connection in namespace
let conn = namespace::connection_for("myns")?;
```

**Features Used:**
- `NamespaceWatcher` for detecting namespace add/remove
- `namespace::connection_for()` - Named namespace connections
- `Connection::new_in_namespace_path()` - Path-based namespace connections

**Working Well:** Namespace watching is solid after 0.5.1 race condition fix.

---

### 6. Diagnostics (`diagnostics.rs`) - New in 0.5.0

**Usage:**
```rust
use nlink::Connection;
use nlink::netlink::Route;

// Check link status
let conn = Connection::<Route>::new()?;
let links = conn.get_links().await?;
for link in &links {
    let is_up = link.is_up();
    let has_carrier = link.has_carrier();
    let mtu = link.mtu().unwrap_or(1500);
}
```

**Note:** The new diagnostics feature currently uses nlink only for link status. Gateway detection and ping tests use shell commands (`ip route`, `ping`) because nlink doesn't yet provide:
- Route table queries for gateway detection
- ICMP ping functionality

---

## nlink Features NOT Used by tcgui

| Feature | Reason Not Used |
|---------|-----------------|
| HTB/HFSC/DRR/QFQ classes | tcgui focuses on simple netem, not complex QoS |
| TC filters | Not needed for per-interface netem |
| MPLS/SRv6/MPTCP | Out of scope for TC GUI |
| MACsec | Out of scope |
| Bridge VLAN/FDB | tcgui manages TC, not switching |
| Nexthop objects | Not needed |
| Routing rules | Not needed |
| WireGuard | Out of scope |
| Conntrack/IPsec | Out of scope |

---

## Recommendations for nlink Improvements

### High Priority (Would Significantly Benefit tcgui)

#### 1. Route Table Query API

**Current Gap:** tcgui's diagnostics use `ip route get` shell command to find the default gateway.

**Proposed nlink API:**
```rust
// Get default gateway
let gateway = conn.get_default_gateway().await?;

// Or more general route lookup
let route = conn.lookup_route(IpAddr::from_str("8.8.8.8")?).await?;
let gateway = route.gateway();
let dev = route.oif();
```

**Benefit:** Eliminate shell command dependency in diagnostics.

#### 2. Human-Readable Rate Parsing

**Current Gap:** tcgui uses raw kbps values; users must mentally convert.

**Proposed nlink API:**
```rust
// Already in 0.6.0
use nlink::rate_limit::RateLimit;

let rate = RateLimit::parse("10mbit")?;
netem.rate(rate.bytes_per_sec());
```

**Benefit:** Cleaner preset/scenario JSON: `"rate": "10mbit"` instead of `"rate_kbps": 10000`.

**Status:** Available in nlink 0.6.0, just needs tcgui integration.

#### 3. Full TC State Capture/Restore

**Current Gap:** tcgui only knows "had netem or not" for rollback.

**Proposed nlink API:**
```rust
// Capture full TC state
let state = conn.capture_tc_state("eth0").await?;

// Later restore it exactly
conn.restore_tc_state(&state).await?;
```

**Benefit:** Scenario rollback would restore exact previous configuration, not just "remove netem".

**Status:** `NetworkConfig::capture()` in 0.6.0 might help, needs investigation.

---

### Medium Priority (Nice to Have)

#### 4. Ping/Connectivity Testing

**Current Gap:** Diagnostics shell out to `ping` command.

**Proposed nlink API:**
```rust
// 0.6.0 has this
use nlink::diagnostics::ConnectivityChecker;

let result = ConnectivityChecker::new()
    .destination("10.0.0.1")
    .method(Method::Icmp)
    .count(3)
    .check()
    .await?;

println!("Latency: {:?}", result.latency);
println!("Loss: {}%", result.packet_loss_percent);
```

**Benefit:** No shell commands for diagnostics.

**Status:** Available in nlink 0.6.0, could replace current ping-based diagnostics.

#### 5. Multi-Namespace Event Stream

**Current Gap:** Must spawn separate tasks per namespace.

**Proposed nlink API:**
```rust
let mut stream = MultiNamespaceEventStream::new()
    .namespace("ns1")
    .namespace("ns2")
    .default_namespace()
    .build()?;

while let Some((ns_name, event)) = stream.next().await {
    println!("[{}] {:?}", ns_name, event);
}
```

**Benefit:** Simpler event handling code, fewer spawned tasks.

---

### Low Priority (Future Consideration)

#### 6. Ingress Rate Limiting

**Current Gap:** netem only shapes egress traffic.

**Proposed nlink API:**
```rust
// 0.6.0 has this
use nlink::rate_limit::RateLimiter;

RateLimiter::new("eth0")
    .ingress("10mbit")
    .egress("100mbit")
    .apply()
    .await?;
```

**Benefit:** Could offer bidirectional rate limiting in UI.

**Status:** Available in nlink 0.6.0, would require UI changes to expose.

#### 7. TC Stats Query API

**Current Gap:** tcgui queries `/proc/net/dev` for interface stats but cannot easily get qdisc-specific stats (drops, overlimits, etc.).

**Proposed nlink API:**
```rust
let stats = conn.get_qdisc_stats("eth0").await?;
println!("Drops: {}", stats.drops);
println!("Overlimits: {}", stats.overlimits);
println!("Requeues: {}", stats.requeues);
```

**Benefit:** Show how many packets were affected by TC rules.

---

## Summary: What nlink Could Improve

| Priority | Feature | Benefit for tcgui | Effort |
|----------|---------|-------------------|--------|
| High | Route table queries | Remove shell commands from diagnostics | Medium |
| High | Rate string parsing | Cleaner preset format | Low (already in 0.6.0) |
| High | TC state capture/restore | Reliable scenario rollback | Medium |
| Medium | Connectivity checker | Pure-Rust diagnostics | Low (already in 0.6.0) |
| Medium | Multi-namespace events | Simpler event handling | Medium |
| Low | Ingress rate limiting | Bidirectional shaping | Low (already in 0.6.0) |
| Low | TC stats API | Show rule effectiveness | Low |

---

## Action Items for tcgui

### Immediate (0.5.1 Release)
1. Bump nlink to 0.6.0 in `Cargo.toml`
2. Verify all tests pass
3. No code changes required (API compatible)

### Short-term (0.6.0 Release)
1. Adopt `RateLimit::parse()` for human-readable rates in presets/scenarios
2. Consider using `ConnectivityChecker` from nlink instead of shelling out to `ping`
3. Update preset/scenario JSON format to use string rates

### Medium-term
1. Investigate `NetworkConfig` for better TC state management
2. Consider exposing ingress rate limiting in UI
3. Add qdisc stats display (drops, overlimits)

---

## Conclusion

tcgui makes good use of nlink's core functionality: interface discovery, TC netem configuration, event monitoring, and namespace handling. The nlink 0.6.0 release brings several features that could benefit tcgui:

1. **Rate Limiting DSL** - Ready to use, would improve preset authoring
2. **Connectivity Checker** - Could replace shell-based diagnostics
3. **NetworkConfig** - Could improve scenario rollback reliability

The main gaps are:
- Route table queries (for gateway detection without shell commands)
- Multi-namespace event aggregation (simplify event handling code)
- TC qdisc stats (show rule effectiveness)

These represent potential future enhancements to nlink that would make tcgui's code cleaner and more robust.
