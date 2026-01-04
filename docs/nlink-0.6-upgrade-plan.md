# nlink 0.6.0 Upgrade Implementation Plan

**Date**: 2026-01-04  
**Target Version**: tcgui 0.5.1  
**Scope**: Upgrade nlink and adopt available 0.6.0 features

---

## Overview

This plan covers the implementation of nlink 0.6.0 features identified in the response analysis. **Excluded** from this plan (per request):
- `RateLimiter` for ingress rate limiting
- `StreamMap` pattern for multi-namespace events

---

## Tasks

### Task 1: Bump nlink to 0.6.0

**File**: `Cargo.toml` (workspace root)

**Current**:
```toml
nlink = { version = "0.5.0", features = ["full"] }
```

**Change to**:
```toml
nlink = { version = "0.6.0", features = ["full"] }
```

**Verification**:
- Run `cargo build` to check API compatibility
- Run `just test` to verify all tests pass

**Risk**: Low - nlink team confirmed backward compatibility

---

### Task 2: Replace Shell-Based Gateway Detection

**File**: `tcgui-backend/src/diagnostics.rs`

**Current Implementation** (lines ~180-210):
```rust
async fn get_default_gateway(&self, namespace: &str) -> Option<String> {
    let output = if namespace == "default" {
        Command::new("ip")
            .args(["route", "show", "default"])
            // ...
    } else {
        Command::new("ip")
            .args(["netns", "exec", namespace, "ip", "route", "show", "default"])
            // ...
    };
    // Parse "default via X.X.X.X dev ethX"
}
```

**New Implementation**:
```rust
use nlink::netlink::{Connection, Route, namespace};

async fn get_default_gateway(&self, ns: &str) -> Option<String> {
    let conn = if ns == "default" {
        Connection::<Route>::new().ok()?
    } else {
        let ns_path = format!("/var/run/netns/{}", ns);
        Connection::<Route>::new_in_namespace_path(&ns_path).ok()?
    };

    let routes = conn.get_routes().await.ok()?;
    
    // Find IPv4 default route (dst_len == 0 means 0.0.0.0/0)
    routes.iter()
        .find(|r| r.dst_len() == 0 && r.is_ipv4())
        .and_then(|r| r.gateway())
        .map(|ip| ip.to_string())
}
```

**Benefits**:
- Eliminates shell command dependency
- More robust parsing (no string manipulation)
- Consistent with rest of nlink usage

**Testing**:
- Run diagnostics on interface with gateway
- Run diagnostics in network namespace
- Verify gateway is correctly detected

---

### Task 3: Add TC Stats to Diagnostics Output

**Files**: 
- `tcgui-backend/src/diagnostics.rs`
- `tcgui-shared/src/lib.rs` (if DiagnosticsReport needs update)

**Current State**: 
TC statistics infrastructure exists in `tc_commands.rs` (`get_tc_statistics()`) but is not integrated into diagnostics.

**Changes Required**:

#### 3a. Add TC stats to DiagnosticsReport

**File**: `tcgui-shared/src/lib.rs` (or wherever DiagnosticsReport is defined)

Add to existing `DiagnosticsReport` struct:
```rust
pub struct DiagnosticsReport {
    // ... existing fields ...
    
    /// TC qdisc statistics (if netem is configured)
    pub tc_stats: Option<TcDiagnosticStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcDiagnosticStats {
    /// Packets dropped by qdisc
    pub drops: u64,
    /// Queue overlimit events
    pub overlimits: u64,
    /// Current queue length (packets)
    pub qlen: u32,
    /// Current backlog (bytes)
    pub backlog: u32,
    /// Kernel-measured throughput (bps), if available
    pub bps: Option<u64>,
}
```

#### 3b. Collect TC stats in diagnostics

**File**: `tcgui-backend/src/diagnostics.rs`

In the diagnostics collection function, add:
```rust
// Get TC statistics if netem is configured
let tc_stats = match self.tc_manager.get_tc_statistics(&namespace, &interface).await {
    Ok(Some(stats)) => Some(TcDiagnosticStats {
        drops: stats.queue.drops,
        overlimits: stats.queue.overlimits,
        qlen: stats.queue.qlen,
        backlog: stats.queue.backlog,
        bps: stats.rate_est.map(|r| r.bps),
    }),
    Ok(None) => None,  // No netem configured
    Err(e) => {
        warn!("Failed to get TC stats: {}", e);
        None
    }
};
```

**Benefits**:
- Shows effectiveness of TC rules (drops, overlimits)
- Provides kernel-measured throughput
- Helps debug when traffic shaping isn't working as expected

---

### Task 4: Adopt Human-Readable Rate Parsing

**Files**:
- `tcgui-shared/src/preset_json.rs`
- `tcgui-shared/src/scenario_json.rs`
- `tcgui-shared/Cargo.toml` (add nlink dependency if not present)

**Current Format**:
```json5
{
    rate_limit: { rate_kbps: 1000 }
}
```

**New Format** (backward compatible):
```json5
{
    rate_limit: { rate: "1mbit" }
    // OR legacy format still works:
    rate_limit: { rate_kbps: 1000 }
}
```

#### 4a. Update RateLimitConfigJson

**File**: `tcgui-shared/src/preset_json.rs`

```rust
use nlink::netlink::ratelimit::RateLimit;

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfigJson {
    /// Human-readable rate string (e.g., "10mbit", "1gbit", "500kbit")
    pub rate: Option<String>,
    
    /// Legacy: rate in kbps (deprecated, use `rate` instead)
    #[serde(default)]
    pub rate_kbps: Option<u32>,
}

impl RateLimitConfigJson {
    /// Convert to rate in kbps, preferring human-readable format
    pub fn to_rate_kbps(&self) -> Result<u32, String> {
        if let Some(ref rate_str) = self.rate {
            // Parse human-readable rate using nlink
            let rate = RateLimit::parse(rate_str)
                .map_err(|e| format!("Invalid rate '{}': {}", rate_str, e))?;
            // Convert bytes/sec to kbps
            let kbps = (rate.rate * 8) / 1000;
            Ok(kbps as u32)
        } else if let Some(kbps) = self.rate_kbps {
            Ok(kbps)
        } else {
            Ok(1000) // Default 1000 kbps
        }
    }
}
```

#### 4b. Update PresetFile conversion

**File**: `tcgui-shared/src/preset_json.rs`

```rust
impl PresetFile {
    pub fn to_custom_preset(self) -> Result<CustomPreset, String> {
        let config = TcNetemConfig {
            // ... other fields ...
            rate_limit: match self.rate_limit {
                Some(rate) => TcRateLimitConfig {
                    enabled: true,
                    rate_kbps: rate.to_rate_kbps()?,
                },
                None => TcRateLimitConfig {
                    enabled: false,
                    rate_kbps: 1000,
                },
            },
        };
        Ok(CustomPreset { /* ... */ })
    }
}
```

#### 4c. Same changes for scenario_json.rs

Apply identical pattern to `tcgui-shared/src/scenario_json.rs`.

**Supported Rate Formats** (from nlink):
- `"10mbit"` - 10 megabits/sec
- `"1gbit"` - 1 gigabit/sec
- `"500kbit"` - 500 kilobits/sec
- `"100mbps"` - 100 megabytes/sec (note: bytes not bits)
- `"1000"` - 1000 bytes/sec (raw number)

**Testing**:
- Create preset with `rate: "10mbit"`
- Create preset with legacy `rate_kbps: 10000`
- Verify both parse correctly
- Test invalid rate strings produce clear errors

---

### Task 5: Use nlink Diagnostics Module for Route/Gateway Checks

**File**: `tcgui-backend/src/diagnostics.rs`

**Current**: Manual route parsing after shell command.

**New**: Use nlink's `Diagnostics` module where applicable.

```rust
use nlink::netlink::diagnostics::Diagnostics;
use nlink::netlink::{Connection, Route};

async fn check_route_to_target(&self, namespace: &str, target: &str) -> RouteCheckResult {
    let conn = self.get_connection_for_namespace(namespace)?;
    let diag = Diagnostics::new(conn);
    
    let target_ip: IpAddr = target.parse()?;
    let report = diag.check_connectivity(target_ip).await?;
    
    RouteCheckResult {
        has_route: report.route.is_some(),
        gateway: report.route.and_then(|r| r.gateway()).map(|ip| ip.to_string()),
        gateway_reachable: report.gateway_reachable,
        issues: report.issues.iter().map(|i| i.message.clone()).collect(),
    }
}
```

**Note**: Keep shell `ping` for latency testing - nlink confirmed ICMP ping is out of scope.

---

### Task 6: Implement TC State Snapshot with Netem Options

**File**: `tcgui-backend/src/tc_commands.rs`

**Current**: `CapturedTcState` only tracks `had_netem: bool`.

**Enhancement**: Store the actual netem options for proper restoration.

```rust
use nlink::netlink::tc_options::NetemOptions;

#[derive(Debug, Clone)]
pub struct CapturedTcState {
    pub namespace: String,
    pub interface: String,
    pub had_netem: bool,
    /// The actual netem options if configured
    pub netem_options: Option<NetemOptions>,
}

pub async fn capture_tc_state(&self, namespace: &str, interface: &str) -> Result<CapturedTcState> {
    let conn = self.get_connection_for_namespace(namespace)?;
    let qdiscs = conn.get_qdiscs_for(interface).await?;
    
    let root_netem = qdiscs.iter()
        .find(|q| q.is_netem() && q.parent() == 0xFFFFFFFF);
    
    Ok(CapturedTcState {
        namespace: namespace.to_string(),
        interface: interface.to_string(),
        had_netem: root_netem.is_some(),
        netem_options: root_netem.and_then(|q| q.netem_options()),
    })
}

pub async fn restore_tc_state(&self, state: &CapturedTcState) -> Result<String> {
    // First remove current config
    self.remove_tc_config_in_namespace(&state.namespace, &state.interface).await.ok();
    
    if let Some(ref opts) = state.netem_options {
        // Rebuild NetemConfig from captured options
        let mut netem = NetemConfig::default();
        
        if let Some(delay) = opts.delay() {
            netem = netem.delay_us(delay.as_micros() as u32);
        }
        if let Some(jitter) = opts.jitter() {
            netem = netem.delay_jitter_us(jitter.as_micros() as u32);
        }
        if let Some(loss) = opts.loss() {
            netem = netem.loss_percent(loss);
        }
        if let Some(dup) = opts.duplicate() {
            netem = netem.duplicate_percent(dup);
        }
        if let Some(corrupt) = opts.corrupt() {
            netem = netem.corrupt_percent(corrupt);
        }
        if let Some(rate) = opts.rate_bps() {
            netem = netem.rate(rate);
        }
        
        // Reapply the original config
        let conn = self.get_connection_for_namespace(&state.namespace)?;
        conn.apply_netem(&state.interface, netem).await?;
        
        Ok("TC state restored with original configuration".to_string())
    } else {
        Ok("TC state restored (no previous configuration)".to_string())
    }
}
```

**Benefits**:
- Scenario rollback restores exact previous TC config
- Not just "had netem or not" but actual delay/loss/rate values

---

## Implementation Order

1. **Task 1**: Bump nlink version (prerequisite for all others)
2. **Task 2**: Replace gateway detection (isolated change, easy to test)
3. **Task 3**: Add TC stats to diagnostics (extends existing infrastructure)
4. **Task 4**: Human-readable rate parsing (shared crate change)
5. **Task 5**: Use nlink Diagnostics module (depends on Task 2)
6. **Task 6**: TC state snapshot enhancement (improves scenario reliability)

---

## Testing Strategy

### Unit Tests
- Rate parsing: valid rates, invalid rates, legacy format
- TC stats conversion

### Integration Tests
- Gateway detection in default namespace
- Gateway detection in network namespace
- TC stats retrieval with netem configured
- TC state capture and restore round-trip

### Manual Testing
- Run `just dev` to verify no warnings/errors
- Start backend, run diagnostics on real interface
- Create preset with human-readable rate
- Run scenario, stop mid-execution, verify cleanup

---

## Rollback Plan

If issues arise after deployment:
1. Revert nlink to 0.5.0 in `Cargo.toml`
2. Revert gateway detection to shell-based
3. Rate parsing: legacy `rate_kbps` still works

---

## Success Criteria

- [ ] nlink 0.6.0 compiles and tests pass
- [ ] Gateway detection works without shell commands
- [ ] TC stats appear in diagnostics output
- [ ] Presets accept `rate: "10mbit"` format
- [ ] Scenarios accept `rate: "10mbit"` format
- [ ] TC state restore properly reapplies original config
- [ ] `just dev` passes with zero warnings
