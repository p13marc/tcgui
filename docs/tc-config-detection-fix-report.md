# TC Configuration Detection Fix Report

## Issue Summary

When restarting the tcgui backend and frontend with an existing TC configuration on an interface, the frontend checkbox state did not reflect the actual TC state. For example, if `loss 1%` was applied to an interface, the LSS checkbox would remain unchecked after restart.

## Root Cause

The backend's `detect_current_tc_config` function was using nlink's `check_existing_qdisc()` method which only returned a string like `"qdisc netem root"` without the actual parameters. The subsequent string parsing with `parse_tc_parameters()` would find no values and return a default configuration with `loss: 0.0`.

## Solution Implemented

### Changes to nlink Crate (by user)

The nlink crate was enhanced with comprehensive netem options parsing:

#### Phase 1: Basic Netem Parsing
1. **Added `NetemOptions` struct** with core netem parameters
2. **Added `netem_options()` method to `TcMessage`** that parses TCA_OPTIONS and nested attributes
3. **Proper probability conversion** using `prob_to_percent()`

#### Phase 2: Complete Feature Parity (Latest Update)
The nlink crate now includes all recommended improvements:

1. **64-bit delay/jitter support**:
   - `delay_ns: u64` and `jitter_ns: u64` fields (nanosecond precision)
   - Parses TCA_NETEM_LATENCY64 and TCA_NETEM_JITTER64 for large delays (>4.29s)
   - Convenience methods: `delay_ms()`, `jitter_ms()`, `delay_us()`, `jitter_us()`

2. **Rate overhead parameters**:
   - `packet_overhead: i32`
   - `cell_size: u32`
   - `cell_overhead: i32`

3. **ECN support**: `ecn: bool` field

4. **Slot-based transmission**: `slot: Option<NetemSlotOptions>` with:
   - `min_delay_ns`, `max_delay_ns`
   - `max_packets`, `max_bytes`

5. **Statistics reading** via `TcMessage` fields:
   - `stats_basic: Option<TcStatsBasic>` (bytes, packets)
   - `stats_queue: Option<TcStatsQueue>` (qlen, backlog, drops, requeues, overlimits)
   - `stats_rate_est: Option<TcStatsRateEst>` (bps, pps)
   - Convenience methods: `bytes()`, `packets()`, `drops()`, `overlimits()`, etc.

### Changes to tcgui-backend

#### 1. `tcgui-backend/src/tc_commands.rs`

Added new methods using the nlink convenience API:

```rust
use nlink::netlink::tc_options::NetemOptions;

impl TcCommandManager {
    pub async fn get_netem_options(...) -> Result<Option<NetemOptions>> {
        // ...
        if let Some(netem_opts) = qdisc.netem_options() {
            info!(
                "Found netem qdisc on {}:{} with loss={}%, delay={:.2}ms",
                namespace, interface, netem_opts.loss_percent, netem_opts.delay_ms()
            );
            return Ok(Some(netem_opts));
        }
    }
}
```

#### 2. `tcgui-backend/src/main.rs`

Updated `detect_current_tc_config` to use new nlink convenience methods:

```rust
async fn detect_current_tc_config(&self, namespace: &str, interface: &str) -> Option<TcConfiguration> {
    match self.tc_manager.get_netem_options(namespace, interface).await {
        Ok(Some(netem_opts)) => {
            info!(
                "Detected existing netem qdisc on {}:{}: loss={}%, delay={:.2}ms, ecn={}",
                namespace, interface,
                netem_opts.loss_percent,
                netem_opts.delay_ms(),  // Uses convenience method
                netem_opts.ecn,
            );

            // Use convenience methods for conversion
            let delay_ms = if netem_opts.delay_ns > 0 {
                Some(netem_opts.delay_ms() as f32)  // 64-bit precision
            } else { None };

            let jitter_ms = if netem_opts.jitter_ns > 0 {
                Some(netem_opts.jitter_ms() as f32)  // 64-bit precision
            } else { None };

            // ... rest of conversion
        }
        // ...
    }
}
```

#### 3. `tcgui-backend/src/tc_config.rs`

Removed the unused `parse_tc_parameters()` function since we now use nlink's direct netlink parsing.

## Updated Unit Conversion Reference

| nlink NetemOptions | TcConfiguration | Conversion |
|-------------------|-----------------|------------|
| `delay_ns` (u64) | `delay_ms` (f32) | `netem_opts.delay_ms() as f32` |
| `jitter_ns` (u64) | `delay_jitter_ms` (f32) | `netem_opts.jitter_ms() as f32` |
| `delay_corr` (f64) | `delay_correlation` (f32) | cast to f32 |
| `loss_percent` (f64) | `loss` (f32) | cast to f32 |
| `loss_corr` (f64) | `correlation` (f32) | cast to f32 |
| `duplicate_percent` (f64) | `duplicate_percent` (f32) | cast to f32 |
| `duplicate_corr` (f64) | `duplicate_correlation` (f32) | cast to f32 |
| `reorder_percent` (f64) | `reorder_percent` (f32) | cast to f32 |
| `reorder_corr` (f64) | `reorder_correlation` (f32) | cast to f32 |
| `gap` (u32) | `reorder_gap` (u32) | direct copy |
| `corrupt_percent` (f64) | `corrupt_percent` (f32) | cast to f32 |
| `corrupt_corr` (f64) | `corrupt_correlation` (f32) | cast to f32 |
| `rate` (u64, bytes/sec) | `rate_limit_kbps` (u32) | `rate * 8 / 1000` |

## Test Results

- Build: Success with no warnings
- All tests pass

## Files Modified

1. `tcgui-backend/src/tc_commands.rs` - Updated to use `delay_ms()` convenience method
2. `tcgui-backend/src/main.rs` - Updated `detect_current_tc_config` to use 64-bit delay/jitter and ECN
3. `tcgui-backend/src/tc_config.rs` - Removed dead code (`parse_tc_parameters`)

---

## nlink Crate: Current Capabilities

The nlink crate now provides **complete netem options parsing** with:

### NetemOptions Struct

```rust
pub struct NetemOptions {
    // 64-bit precision timing
    pub delay_ns: u64,
    pub jitter_ns: u64,
    pub delay_corr: f64,
    
    // Probability parameters (0-100%)
    pub loss_percent: f64,
    pub loss_corr: f64,
    pub duplicate_percent: f64,
    pub duplicate_corr: f64,
    pub reorder_percent: f64,
    pub reorder_corr: f64,
    pub corrupt_percent: f64,
    pub corrupt_corr: f64,
    
    // Rate limiting
    pub rate: u64,
    pub packet_overhead: i32,
    pub cell_size: u32,
    pub cell_overhead: i32,
    
    // Queue settings
    pub limit: u32,
    pub gap: u32,
    pub ecn: bool,
    
    // Slot-based transmission
    pub slot: Option<NetemSlotOptions>,
}

impl NetemOptions {
    pub fn delay_us(&self) -> u32;
    pub fn jitter_us(&self) -> u32;
    pub fn delay_ms(&self) -> f64;
    pub fn jitter_ms(&self) -> f64;
}
```

### TcMessage Statistics

```rust
pub struct TcMessage {
    // ... other fields
    pub stats_basic: Option<TcStatsBasic>,   // bytes, packets
    pub stats_queue: Option<TcStatsQueue>,   // qlen, backlog, drops, requeues, overlimits
    pub stats_rate_est: Option<TcStatsRateEst>, // bps, pps
}

impl TcMessage {
    pub fn bytes(&self) -> u64;
    pub fn packets(&self) -> u64;
    pub fn drops(&self) -> u32;
    pub fn overlimits(&self) -> u32;
    pub fn requeues(&self) -> u32;
    pub fn qlen(&self) -> u32;
    pub fn backlog(&self) -> u32;
}
```

## Future Opportunities

With the enhanced nlink crate, tcgui could add:

1. **TC Statistics Display** - Show real-time stats (drops, packets, bytes) in the UI using `TcMessage` statistics fields
2. **ECN Support** - Add ECN checkbox to the UI (nlink already supports `ecn: bool`)
3. **Slot-based Transmission** - Advanced traffic shaping using slot configuration

The nlink crate is now **feature-complete** for tcgui's netem use case.
