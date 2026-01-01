# TC Statistics Implementation Report

## Summary

This report documents the implementation of TC qdisc statistics display in tcgui, leveraging the improved nlink crate API. The implementation adds real-time monitoring of packet drops, overlimits, and other qdisc statistics.

## Implementation Overview

### Components Modified

#### Backend (tcgui-backend)

1. **tc_commands.rs** - Added statistics collection methods:
   - `get_tc_statistics(namespace, interface)` - Retrieves TC stats from netem qdisc
   - `get_tc_statistics_with_path(namespace, namespace_path, interface)` - Container-aware variant
   - Returns `Option<(TcStatsBasic, TcStatsQueue)>` tuple

2. **main.rs** - Added statistics publishing:
   - `monitor_and_send_tc_stats()` - Runs alongside bandwidth monitoring
   - `get_tc_stats_publisher()` - Creates/caches Zenoh publishers per interface
   - Publishes to `tcgui/{backend}/tc/stats/{namespace}/{interface}`
   - Uses best-effort QoS (no history, drop on congestion) for high-frequency updates

#### Shared Types (tcgui-shared)

3. **lib.rs** - Added statistics types:
   ```rust
   pub struct TcStatsBasic {
       pub bytes: u64,
       pub packets: u64,
   }

   pub struct TcStatsQueue {
       pub qlen: u32,
       pub backlog: u32,
       pub drops: u32,
       pub requeues: u32,
       pub overlimits: u32,
   }

   pub struct TcStatisticsUpdate {
       pub namespace: String,
       pub interface: String,
       pub backend_name: String,
       pub timestamp: u64,
       pub stats_basic: Option<TcStatsBasic>,
       pub stats_queue: Option<TcStatsQueue>,
   }
   ```

#### Frontend (tcgui-frontend)

4. **zenoh_manager.rs** - Added subscription handler:
   - Subscribes to `tcgui/*/tc/stats/*/*`
   - `handle_tc_stats_update_sample()` - Deserializes and emits events

5. **messages.rs** - Added message variants:
   - `ZenohEvent::TcStatisticsUpdate(TcStatisticsUpdate)`
   - `TcGuiMessage::TcStatisticsUpdate(TcStatisticsUpdate)`

6. **app.rs** - Added event routing:
   - Maps `ZenohEvent::TcStatisticsUpdate` to `TcGuiMessage::TcStatisticsUpdate`
   - Calls handler function

7. **message_handlers.rs** - Added handler:
   - `handle_tc_statistics_update()` - Routes updates to correct TcInterface

8. **interface/state.rs** - Added state fields:
   - `tc_stats_basic: Option<TcStatsBasic>`
   - `tc_stats_queue: Option<TcStatsQueue>`
   - `update_tc_statistics()` method

9. **interface/base.rs** - Added UI rendering:
   - `update_tc_statistics()` public API method
   - `render_tc_stats_display()` - Shows drops/overlimits when TC active
   - Drops shown in error color when > 0
   - Compact formatting with K/M suffixes

### Data Flow

```
Backend                          Frontend
   │                                │
   ├─ get_tc_statistics()           │
   │   └─ nlink: get_qdiscs_for()   │
   │       └─ TcMessage.drops()     │
   │           .overlimits()        │
   │           .packets()           │
   │           .bytes()             │
   │                                │
   ├─ TcStatisticsUpdate ──────────►│
   │   (Zenoh pub/sub)              │
   │                                ├─ ZenohEvent::TcStatisticsUpdate
   │                                │
   │                                ├─ TcGuiMessage::TcStatisticsUpdate
   │                                │
   │                                ├─ handle_tc_statistics_update()
   │                                │
   │                                └─ TcInterface.update_tc_statistics()
   │                                     └─ render_tc_stats_display()
```

### UI Display

When TC is configured on an interface, the UI shows:
- **Drops** (XCircle icon) - Red if > 0, muted otherwise
- **Overlimits** (AlertTriangle icon) - Rate limiting triggered events

Format: Compact with K/M suffixes (e.g., "12K" for 12,000)

---

## nlink Crate Analysis

### Current Capabilities

The nlink crate provides comprehensive TC support:

#### NetemOptions (Reading)
- **64-bit delay/jitter**: `delay_ns`, `jitter_ns` with convenience methods:
  - `delay_us()`, `jitter_us()` - Returns u32 microseconds
  - `delay_ms()`, `jitter_ms()` - Returns f64 milliseconds
- **Loss/Duplicate/Reorder/Corrupt**: Percentage and correlation values
- **Rate limiting**: `rate` (bytes/sec), `packet_overhead`, `cell_size`, `cell_overhead`
- **ECN support**: `ecn: bool`
- **Slot configuration**: `NetemSlotOptions` for slot-based transmission
- **Gap**: Reorder gap parameter

#### TcMessage Statistics
- **TcStatsBasic**: `bytes`, `packets` (u64)
- **TcStatsQueue**: `qlen`, `backlog`, `drops`, `requeues`, `overlimits` (u32)
- **TcStatsRateEst**: `bps`, `pps` (bytes/packets per second)
- Convenience methods: `drops()`, `overlimits()`, `requeues()`, `qlen()`, `backlog()`, `bytes()`, `packets()`

#### Connection Methods
- `get_qdiscs()` - All qdiscs
- `get_qdiscs_for(interface)` - Qdiscs for specific interface
- `add_qdisc_by_index()` - Add new qdisc
- `replace_qdisc_by_index()` - Replace existing qdisc
- `del_qdisc_by_index()` - Delete qdisc
- Namespace support via `connection_for()` and `new_in_namespace_path()`

---

## Potential nlink Improvements

### 1. Rate Estimator Access (Priority: Medium)

**Current State**: `TcStatsRateEst` is parsed but not exposed via convenience methods on `TcMessage`.

**Suggested Improvement**:
```rust
impl TcMessage {
    /// Get bytes per second from rate estimator.
    pub fn bps(&self) -> u32 {
        self.stats_rate_est.map(|s| s.bps).unwrap_or(0)
    }

    /// Get packets per second from rate estimator.
    pub fn pps(&self) -> u32 {
        self.stats_rate_est.map(|s| s.pps).unwrap_or(0)
    }
}
```

**Benefit**: Enables real-time throughput monitoring without manual calculation.

### 2. Statistics Delta Calculation (Priority: Low)

**Current State**: Only absolute counters are provided.

**Suggested Improvement**: Add helper for calculating deltas between samples:
```rust
impl TcStatsBasic {
    /// Calculate delta from previous sample.
    pub fn delta(&self, previous: &Self) -> TcStatsBasicDelta {
        TcStatsBasicDelta {
            bytes: self.bytes.saturating_sub(previous.bytes),
            packets: self.packets.saturating_sub(previous.packets),
        }
    }
}
```

**Benefit**: Simplifies rate calculation in monitoring applications.

### 3. Netem Distribution Support (Priority: Low)

**Current State**: Only slot distribution delay/jitter is parsed.

**Missing Features**:
- `TCA_NETEM_DELAY_DIST` - Delay distribution table
- `TCA_NETEM_LOSS` - Loss model (Gilbert-Elliott, etc.)

**Suggested Improvement**: Parse and expose distribution parameters:
```rust
pub struct NetemOptions {
    // ... existing fields ...
    
    /// Delay distribution type (normal, pareto, paretonormal, etc.)
    pub delay_distribution: Option<NetemDistribution>,
    
    /// Loss model (random, state, gemodel)
    pub loss_model: Option<NetemLossModel>,
}
```

**Benefit**: Full feature parity with `tc` command-line tool.

### 4. Netem Config Builder Improvements (Priority: Medium)

**Current State**: `NetemConfig` builder works well but could be more ergonomic.

**Suggested Improvements**:

a) **Duration convenience methods**:
```rust
impl NetemConfig {
    /// Set delay using Duration.
    pub fn delay_duration(self, delay: Duration) -> Self {
        self.delay(delay)  // Already exists
    }

    /// Set delay in milliseconds (convenience for common case).
    pub fn delay_ms(self, ms: u32) -> Self {
        self.delay(Duration::from_millis(ms as u64))
    }
}
```

b) **Preset configurations**:
```rust
impl NetemConfig {
    /// Create a satellite link preset (high latency, some loss).
    pub fn satellite() -> Self {
        Self::new()
            .delay(Duration::from_millis(600))
            .jitter(Duration::from_millis(50))
            .loss(0.1)
    }

    /// Create a mobile network preset.
    pub fn mobile_3g() -> Self {
        Self::new()
            .delay(Duration::from_millis(100))
            .jitter(Duration::from_millis(30))
            .loss(1.0)
            .rate(384_000 / 8)  // 384 kbps
    }
}
```

### 5. Streaming Statistics (Priority: High)

**Current State**: Statistics must be polled via `get_qdiscs_for()`.

**Suggested Improvement**: Add netlink notification support for TC changes:
```rust
impl Connection {
    /// Subscribe to TC statistics updates.
    pub async fn subscribe_tc_stats(
        &self,
        ifindex: i32,
    ) -> Result<impl Stream<Item = TcStatsUpdate>> {
        // Use RTM_NEWQDISC/RTM_DELQDISC notifications
        // with RTNLGRP_TC multicast group
    }
}
```

**Benefit**: Eliminates polling overhead, enables event-driven statistics updates.

### 6. HTB/Class Statistics (Priority: Medium)

**Current State**: HTB class options are parsed, but class-specific statistics access is limited.

**Suggested Improvement**: Add methods to query class statistics:
```rust
impl Connection {
    /// Get all classes for a qdisc.
    pub async fn get_classes(&self, ifindex: i32) -> Result<Vec<TcMessage>>;

    /// Get class statistics for a specific class.
    pub async fn get_class_stats(
        &self,
        ifindex: i32,
        classid: u32,
    ) -> Result<Option<TcMessage>>;
}
```

**Benefit**: Enables monitoring of hierarchical QoS configurations.

### 7. Filter Support (Priority: Low)

**Current State**: Basic filter parsing exists but filter configuration is limited.

**Suggested Improvement**: Add filter builders similar to `NetemConfig`:
```rust
pub struct U32FilterConfig { /* ... */ }
pub struct FlowerFilterConfig { /* ... */ }
pub struct BpfFilterConfig { /* ... */ }

impl Connection {
    pub async fn add_filter(
        &self,
        ifindex: i32,
        parent: u32,
        filter: impl FilterConfig,
    ) -> Result<()>;
}
```

**Benefit**: Enables programmatic traffic classification.

### 8. Error Context Enhancement (Priority: Medium)

**Current State**: Errors are descriptive but could include more context.

**Suggested Improvement**:
```rust
#[derive(Debug, thiserror::Error)]
pub enum TcError {
    #[error("Failed to add qdisc '{kind}' on interface {ifindex}: {source}")]
    AddQdiscFailed {
        kind: String,
        ifindex: i32,
        source: std::io::Error,
    },

    #[error("Qdisc operation requires CAP_NET_ADMIN capability")]
    PermissionDenied,

    #[error("Interface index {ifindex} not found")]
    InterfaceNotFound { ifindex: i32 },
}
```

**Benefit**: Easier debugging and better error messages for users.

---

## Summary of Recommendations

| Priority | Improvement | Effort | Impact |
|----------|-------------|--------|--------|
| High | Streaming Statistics | Medium | Eliminates polling |
| Medium | Rate Estimator Access | Low | Better monitoring |
| Medium | Error Context Enhancement | Low | Better debugging |
| Medium | HTB/Class Statistics | Medium | QoS monitoring |
| Medium | Config Builder Improvements | Low | Better ergonomics |
| Low | Statistics Delta Calculation | Low | Convenience |
| Low | Netem Distribution Support | High | Feature parity |
| Low | Filter Support | High | Advanced use cases |

The nlink crate is already well-designed and provides comprehensive TC functionality. The suggested improvements are incremental enhancements that would benefit specific use cases rather than fundamental changes to the API.
