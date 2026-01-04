# nlink Response to tcgui Integration Report

**Date**: 2026-01-04  
**nlink Version**: 0.6.0  
**Responding to**: tcgui Integration Report (tcgui 0.5.0)

## Summary

The tcgui integration report is well-researched and provides valuable feedback on nlink's API design. This response addresses each point, clarifies what nlink 0.6.0 already provides, and acknowledges areas where improvements could be made.

---

## What nlink 0.6.0 Already Provides

Several features the report marks as "gaps" or "proposed" are already implemented in nlink 0.6.0:

### 1. Route Table Queries (Already Available)

The report mentions tcgui shells out to `ip route get` for gateway detection. However, nlink 0.6.0 provides:

```rust
use nlink::netlink::{Connection, Route};

let conn = Connection::<Route>::new()?;

// Get all routes
let routes = conn.get_routes().await?;

// Find default gateway
let default_route = routes.iter()
    .find(|r| r.dst_len() == 0 && r.is_ipv4());
if let Some(route) = default_route {
    println!("Default gateway: {:?}", route.gateway);
    println!("Via interface: {:?}", route.oif);
}

// RouteMessage helpers
for route in &routes {
    if route.is_static() {
        println!("{}", route.destination_str());
    }
}
```

Additionally, `FibLookup` provides direct route lookups:

```rust
use nlink::netlink::{Connection, FibLookup};
use std::net::Ipv4Addr;

let conn = Connection::<FibLookup>::new()?;
let result = conn.lookup(Ipv4Addr::new(8, 8, 8, 8).into()).await?;
println!("Route type: {:?}, table: {}", result.route_type, result.table);
```

**Status**: Fully available. tcgui should use `get_routes()` and filter for default gateway.

### 2. Human-Readable Rate Parsing (Already Available)

The report correctly identifies this as available in 0.6.0:

```rust
use nlink::netlink::ratelimit::RateLimit;
use nlink::util::parse::get_rate;

// Parse rate strings
let rate = RateLimit::parse("100mbit")?;
println!("Rate in bps: {}", rate.rate);

// Or use the utility directly
let bytes_per_sec = get_rate("1gbit")?;
```

**Status**: Fully available. tcgui should integrate this for cleaner preset formats.

### 3. Connectivity Checker / Diagnostics (Already Available)

The report mentions tcgui shells out to `ping`. nlink 0.6.0's `Diagnostics` module provides:

```rust
use nlink::netlink::{Connection, Route};
use nlink::netlink::diagnostics::Diagnostics;

let conn = Connection::<Route>::new()?;
let diag = Diagnostics::new(conn);

// Check connectivity (uses route lookup + neighbor cache, not ICMP)
let report = diag.check_connectivity("8.8.8.8".parse()?).await?;
println!("Route: {:?}", report.route);
println!("Gateway reachable: {}", report.gateway_reachable);
for issue in &report.issues {
    println!("[{:?}] {}", issue.severity, issue.message);
}
```

**Clarification**: The connectivity check uses route lookup and neighbor cache inspection rather than ICMP ping. For actual latency testing, shell commands are still needed since raw sockets for ICMP require additional privileges and complexity.

**Status**: Partially available. Route/gateway checking is implemented; actual ICMP ping is out of scope.

### 4. Ingress Rate Limiting (Already Available)

```rust
use nlink::netlink::{Connection, Route};
use nlink::netlink::ratelimit::RateLimiter;
use std::time::Duration;

let conn = Connection::<Route>::new()?;

RateLimiter::new("eth0")
    .egress("100mbit")?
    .ingress("1gbit")?
    .burst_to("150mbit")?
    .latency(Duration::from_millis(20))
    .apply(&conn)
    .await?;
```

**Status**: Fully available. Uses IFB (Intermediate Functional Block) device internally.

### 5. TC Stats Query (Already Available)

TC statistics are available via `TcMessage` accessors:

```rust
let qdiscs = conn.get_qdiscs_for("eth0").await?;
for qdisc in &qdiscs {
    println!("Kind: {}", qdisc.kind().unwrap_or("?"));
    println!("Drops: {}", qdisc.drops());
    println!("Overlimits: {}", qdisc.overlimits());
    println!("Backlog: {} bytes, {} packets", qdisc.backlog(), qdisc.qlen());
    println!("Rate: {} bps, {} pps", qdisc.bps(), qdisc.pps());
    println!("Bytes: {}, Packets: {}", qdisc.bytes(), qdisc.packets());
}
```

**Status**: Fully available.

---

## Valid Gaps Identified

The report correctly identifies some areas that could be improved:

### 1. TC State Capture/Restore

**Current State**: The `NetworkConfig` module provides declarative configuration with diff/apply, but does not yet support capturing the *current* TC state into a config object.

**What exists**:
- `NetworkConfig::diff()` compares desired vs current state
- `NetworkConfig::apply()` applies changes idempotently

**What's missing**:
- `NetworkConfig::capture()` to snapshot current state for later restore

**Recommendation**: For now, tcgui can:
1. Query qdiscs before applying changes: `conn.get_qdiscs_for("eth0").await?`
2. Store the netem options if present
3. Restore by reapplying the captured options or deleting the qdisc

Example workaround:
```rust
// Before applying scenario
let qdiscs = conn.get_qdiscs_for("eth0").await?;
let had_netem = qdiscs.iter().any(|q| q.is_netem());
let prev_options = qdiscs.iter()
    .find(|q| q.is_netem() && q.is_root())
    .and_then(|q| q.netem_options());

// After scenario ends
if let Some(opts) = prev_options {
    // Recreate similar netem config
} else if had_netem {
    // Remove netem
    conn.remove_netem("eth0").await?;
}
```

**Future Enhancement**: A `capture_tc_state()` method that returns a serializable state object would be valuable.

### 2. Multi-Namespace Event Stream

**Current State**: Each namespace requires a separate connection and event stream. Users must use `tokio_stream::StreamMap` or similar to combine them.

**Example with current API**:
```rust
use nlink::netlink::{Connection, Route, RtnetlinkGroup, namespace};
use tokio_stream::{StreamExt, StreamMap};

let mut streams = StreamMap::new();

// Default namespace
let mut conn = Connection::<Route>::new()?;
conn.subscribe_all()?;
streams.insert("default", conn.into_events());

// Named namespace
let mut conn_ns = namespace::connection_for("myns")?;
conn_ns.subscribe_all()?;
streams.insert("myns", conn_ns.into_events());

// Combined stream
while let Some((ns, result)) = streams.next().await {
    println!("[{}] {:?}", ns, result?);
}
```

**Future Enhancement**: A dedicated `MultiNamespaceEventStream` builder could simplify this pattern.

### 3. All Interfaces Across All Namespaces

**Current State**: Must iterate namespaces manually.

```rust
use nlink::netlink::{namespace, Connection, Route};

let mut all_links = Vec::new();

// Default namespace
let conn = Connection::<Route>::new()?;
all_links.extend(conn.get_links().await?);

// Named namespaces
for ns in namespace::list()? {
    let conn = namespace::connection_for(&ns)?;
    all_links.extend(conn.get_links().await?);
}
```

**Future Enhancement**: A `get_links_all_namespaces()` helper could be added.

---

## Clarifications on Pain Points

### `apply_netem()` Only Does Replace

**Clarification**: This is intentional. The method uses `RTM_NEWQDISC` with replace semantics to be idempotent. If no qdisc exists, it creates one; if one exists, it replaces it.

To explicitly add (and fail if exists):
```rust
// Use add_qdisc() which fails on conflict
conn.add_qdisc("eth0", netem_config).await?;
```

To handle the "no qdisc to delete" case gracefully:
```rust
match conn.del_qdisc("eth0", "root").await {
    Ok(()) => println!("Deleted"),
    Err(e) if e.is_not_found() => println!("Nothing to delete"),
    Err(e) => return Err(e),
}
```

### Namespace Iteration

The report notes there's no single-call way to get interfaces across namespaces. This is by design:

1. **Performance**: Each namespace requires a separate netlink socket
2. **Isolation**: Namespace operations should be explicit
3. **Error handling**: Failures in one namespace shouldn't affect others

The `namespace::list()` + loop pattern is the recommended approach.

---

## API Usage Recommendations

### Preferred Patterns for tcgui

1. **Interface discovery**:
   ```rust
   let links = conn.get_links().await?;
   for link in &links {
       let name = link.name_or("?");
       let is_up = link.is_up();
       let has_carrier = link.has_carrier();
   }
   ```

2. **TC netem with human rates**:
   ```rust
   use nlink::netlink::tc::NetemConfig;
   use std::time::Duration;
   
   let netem = NetemConfig::new()
       .delay(Duration::from_millis(100))
       .jitter(Duration::from_millis(10))
       .loss(1.0)  // percent
       .rate_str("10mbit")?  // if you add this method
       .build();
   
   conn.apply_netem("eth0", netem).await?;
   ```

3. **Rate limiting (new in 0.6.0)**:
   ```rust
   use nlink::netlink::ratelimit::RateLimiter;
   
   RateLimiter::new("eth0")
       .egress("100mbit")?
       .ingress("1gbit")?
       .apply(&conn)
       .await?;
   ```

4. **Gateway detection** (no shell commands needed):
   ```rust
   let routes = conn.get_routes().await?;
   let gw = routes.iter()
       .find(|r| r.dst_len() == 0 && r.is_ipv4())
       .and_then(|r| r.gateway);
   ```

5. **Event monitoring with namespace support**:
   ```rust
   use tokio_stream::StreamMap;
   
   let mut streams = StreamMap::new();
   // ... add streams for each namespace
   
   while let Some((ns, event)) = streams.next().await {
       match event? {
           NetworkEvent::NewLink(link) => { /* ... */ }
           NetworkEvent::NewQdisc(qdisc) => { /* ... */ }
           _ => {}
       }
   }
   ```

---

## Recommended tcgui Action Items

### Immediate (0.5.1)
1. Update to nlink 0.6.0 - API is backward compatible
2. Replace `ip route get` shell command with `conn.get_routes()` for gateway detection
3. Verify existing integration works with new version

### Short-term (0.6.0)
1. Use `RateLimit::parse()` for human-readable rate strings in presets
2. Use `RateLimiter` for combined egress/ingress limiting if needed
3. Consider using `Diagnostics` for link/route checks instead of shell commands
4. Display TC stats (drops, overlimits) in the UI using `TcMessage` accessors

### Medium-term
1. Implement proper state capture before applying scenarios (store netem options)
2. Use `StreamMap` pattern for multi-namespace event monitoring
3. Display qdisc effectiveness metrics (drops/packets ratio)

---

## Future nlink Enhancements (Backlog)

Based on this integration report, the following enhancements are being considered:

| Priority | Feature | Description |
|----------|---------|-------------|
| Medium | `NetworkConfig::capture()` | Snapshot current network state for rollback |
| Medium | `MultiNamespaceEventStream` | Simplified multi-namespace event aggregation |
| Low | `rate_str()` on NetemConfig | Direct string rate in netem builder |
| Low | `get_default_gateway()` | Convenience method on Connection |

---

## Conclusion

tcgui is using nlink effectively for its core use cases. The integration report highlighted some valid usability improvements that could be made, but many of the "gaps" identified are actually already available in nlink 0.6.0:

**Already Available (tcgui should adopt)**:
- Route queries for gateway detection
- Human-readable rate parsing
- Ingress rate limiting via `RateLimiter`
- TC statistics via `TcMessage`
- Network diagnostics via `Diagnostics`

**Valid Enhancement Requests**:
- TC state capture/restore (workaround available)
- Multi-namespace event stream builder (pattern documented)

The nlink team appreciates this detailed feedback and will consider the enhancement requests for future releases.
