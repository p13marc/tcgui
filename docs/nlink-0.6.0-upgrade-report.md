# nlink 0.6.0 Upgrade Report for tcgui

**Date**: 2026-01-04  
**Current Version**: nlink 0.5.0  
**Target Version**: nlink 0.6.0

## Executive Summary

nlink 0.6.0 introduces significant new features that could enhance tcgui's capabilities. This report analyzes each relevant feature, assesses implementation effort, and provides recommendations.

---

## Current nlink Usage in tcgui

tcgui currently uses nlink for:

| Component | File | Usage |
|-----------|------|-------|
| Interface discovery | `network.rs` | `Connection<Route>`, `get_links()`, namespace operations |
| TC operations | `tc_commands.rs` | `NetemConfig`, `get_qdiscs_for()`, `add_qdisc_by_index()` |
| Namespace handling | `network.rs`, `tc_commands.rs` | `NamespaceSpec`, `connection_for()` |
| Event monitoring | `netlink_events.rs` | `RtnetlinkGroup`, event subscriptions |
| Namespace watching | `namespace_watcher.rs` | `NamespaceWatcher` |

---

## Relevant New Features in 0.6.0

### 1. Rate Limiting DSL (Plan 013)

**What it provides:**
```rust
// New human-readable rate parsing
let rate = RateLimit::parse("100mbit")?;

// Directional rate limiting
RateLimiter::new("eth0")
    .ingress(rate)      // NEW: ingress limiting
    .egress(rate)
    .with_netem()       // Combine with netem
    .apply()?;

// Per-host limiting with HTB + flower filters
PerHostLimiter::new("eth0")
    .per_source()
    .rate("10mbit")
    .apply()?;
```

**Current tcgui approach** (`tc_commands.rs:294-297`):
```rust
if config.rate_limit.enabled && config.rate_limit.rate_kbps > 0 {
    netem = netem.rate(rate::kbps_to_bytes(config.rate_limit.rate_kbps.into()));
}
```

**Benefits for tcgui:**
- Human-readable rate strings in presets/scenarios (e.g., `"rate": "10mbit"` instead of `"rate_kbps": 10000`)
- Ingress rate limiting (currently only egress is supported)
- Cleaner configuration validation

**Implementation effort:** Medium  
**Impact:** Medium-High

---

### 2. Declarative Network Configuration (Plan 012)

**What it provides:**
```rust
// Capture full network state
let config = NetworkConfig::capture()?;

// Compare configurations
let diff = old_config.diff(&new_config);

// Apply changes (with dry-run support)
config.apply()?;
```

**Current tcgui approach** (`tc_commands.rs:622-656`):
```rust
pub struct CapturedTcState {
    pub namespace: String,
    pub interface: String,
    pub qdisc_info: String,  // Just a string!
    pub had_netem: bool,
}
```

**Benefits for tcgui:**
- Full state capture for scenario rollback (not just netem presence)
- Diff-based changes for more precise operations
- Dry-run support for previewing changes
- Could restore exact previous TC configuration, not just "remove netem"

**Implementation effort:** Medium-High  
**Impact:** High (improves scenario reliability)

---

### 3. Network Diagnostics Module (Plan 014)

**What it provides:**
```rust
// Check connectivity through multiple methods
let result = ConnectivityChecker::new()
    .destination("10.0.0.1")
    .method(Method::TcpConnect(80))
    .check()?;

// Detect bottlenecks
let report = BottleneckDetector::new()
    .path("10.0.0.1")
    .detect()?;
// Returns: congestion, packet loss, high latency, MTU problems

// Scan network
let hosts = NetworkScanner::new()
    .subnet("192.168.1.0/24")
    .scan()?;
```

**Benefits for tcgui:**
- "Test Impact" button in UI to verify TC rules are working
- Show actual vs configured latency/loss
- Detect if network degradation is from TC rules or real issues

**Implementation effort:** High (new UI components needed)  
**Impact:** High (significantly improves UX)

---

### 4. FDB Event Monitoring

**What it provides:**
```rust
// New events in NetworkEvent enum
NetworkEvent::NewFdb(FdbEntry)  // Bridge FDB entry added
NetworkEvent::DelFdb(FdbEntry)  // Bridge FDB entry removed
```

**Benefits for tcgui:**
- Better detection of container network changes
- React to Docker/Podman bridge port additions in real-time

**Implementation effort:** Low  
**Impact:** Low-Medium

---

### 5. Bug Fixes (0.5.1)

**Fixed:**
> Race condition in `NamespaceWatcher` when the first namespace is created on a system where `/var/run/netns` doesn't exist.

**Impact for tcgui:** The `namespace_watcher.rs` component would benefit from this fix. Currently it might miss the first namespace creation on fresh systems.

---

## Features NOT Relevant to tcgui

The following 0.6.0 features are not applicable:

| Feature | Reason |
|---------|--------|
| MPTCP Path Manager | tcgui doesn't manage MPTCP |
| MACsec Configuration | Not in scope |
| SRv6 Segment Routing | Not in scope |
| MPLS Routes | Not in scope |
| Nexthop Objects | Not in scope |
| Bridge VLAN Filtering | tcgui focuses on TC, not VLANs |
| Bridge FDB Management | Could be useful but low priority |
| TC Filter Chains | tcgui uses simple root qdiscs |
| HTB/HFSC/DRR/QFQ Classes | tcgui uses netem, not classful qdiscs |

---

## Recommendations

### Tier 1: Immediate (Version Bump)

**Action:** Update `Cargo.toml` from `nlink = "0.5.0"` to `nlink = "0.6.0"`

**Changes required:** None (API compatible)

**Benefits:**
- NamespaceWatcher race condition fix
- All bug fixes from 0.5.1
- Access to new features when needed

```toml
# Cargo.toml change
nlink = { version = "0.6.0", features = ["full"] }
```

---

### Tier 2: Short-term (Rate Limiting DSL)

**Action:** Adopt `RateLimit::parse()` for human-readable rates

**Changes required:**

1. Update `TcNetemConfig.rate_limit` to accept string rates:
```rust
// tcgui-shared/src/lib.rs
pub struct RateLimitConfig {
    pub enabled: bool,
    pub rate: String,  // "10mbit", "1gbit", etc.
}
```

2. Update preset/scenario JSON format:
```json5
// Before
{ "rate_kbps": 10000 }

// After  
{ "rate": "10mbit" }
```

3. Update `tc_commands.rs` to use new DSL:
```rust
if config.rate_limit.enabled {
    let rate = RateLimit::parse(&config.rate_limit.rate)?;
    netem = netem.rate(rate.bytes_per_sec());
}
```

**Effort:** ~2-3 hours  
**Backward compatibility:** Would need migration for existing presets

---

### Tier 3: Medium-term (NetworkConfig for Scenarios)

**Action:** Use `NetworkConfig::capture()` for scenario state management

**Changes required:**

1. Replace `CapturedTcState` with `NetworkConfig`:
```rust
// tc_commands.rs
pub async fn capture_tc_state(...) -> Result<NetworkConfig> {
    NetworkConfig::capture_for_interface(namespace, interface)
}

pub async fn restore_tc_state(config: &NetworkConfig) -> Result<()> {
    config.apply()
}
```

2. Update scenario execution to store full configs:
```rust
// scenario/execution.rs
struct ExecutionState {
    original_config: NetworkConfig,  // Full state, not just "had_netem"
    // ...
}
```

**Effort:** ~4-6 hours  
**Impact:** Much more reliable scenario rollback

---

### Tier 4: Long-term (Diagnostics)

**Action:** Add "Test Network" feature to UI

**New UI components:**
- "Diagnose" button per interface
- Results panel showing:
  - Configured vs actual latency
  - Packet loss verification
  - Connectivity status

**Effort:** ~1-2 days (backend + frontend)  
**Impact:** Significant UX improvement

---

## Migration Checklist

- [ ] Update `Cargo.toml` to nlink 0.6.0
- [ ] Run `cargo build` to verify compatibility
- [ ] Run `cargo test` to verify no regressions
- [ ] (Optional) Adopt Rate Limiting DSL
- [ ] (Optional) Migrate to NetworkConfig for state capture
- [ ] (Optional) Add diagnostics UI

---

## Risk Assessment

| Change | Risk Level | Mitigation |
|--------|------------|------------|
| Version bump only | Very Low | API is backward compatible |
| Rate DSL adoption | Low | Can keep old format as fallback |
| NetworkConfig adoption | Medium | Thorough testing of rollback scenarios |
| Diagnostics feature | Low | Additive feature, no existing code changes |

---

## Conclusion

**Recommended approach:**

1. **Immediate**: Bump to 0.6.0 for bug fixes (no code changes)
2. **Next release**: Adopt Rate Limiting DSL for cleaner preset format
3. **Future**: Consider NetworkConfig and Diagnostics based on user feedback

The version bump is safe and provides immediate benefits. The Rate Limiting DSL would improve the preset/scenario authoring experience. NetworkConfig would make scenario rollback more robust.
