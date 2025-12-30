# rust-netlink Fork Analysis for TC/netem Support

## Executive Summary

To eliminate process spawning for TC operations in tcgui-backend, we need to add netem and TBF qdisc support to the rust-netlink ecosystem. This requires forking **one primary crate** with changes potentially needed in a second.

## rust-netlink Organization Overview

The [rust-netlink](https://github.com/rust-netlink) organization provides Rust crates for Linux netlink protocol communication:

| Crate | Stars | Purpose | Fork Needed? |
|-------|-------|---------|--------------|
| [rtnetlink](https://github.com/rust-netlink/rtnetlink) | 150 | High-level API for network manipulation | Maybe |
| [netlink-packet-route](https://github.com/rust-netlink/netlink-packet-route) | 50 | Low-level packet types for route protocol | **Yes** |
| [netlink-sys](https://github.com/rust-netlink/netlink-sys) | 17 | Netlink socket abstraction | No |
| [netlink-packet-core](https://github.com/rust-netlink/netlink-packet-core) | 7 | Core NetlinkMessage type | No |
| [netlink-packet-generic](https://github.com/rust-netlink/netlink-packet-generic) | 6 | Generic netlink protocol | No |

## Crate to Fork: netlink-packet-route

**Repository:** https://github.com/rust-netlink/netlink-packet-route  
**Version:** v0.27.0 (December 24, 2025)  
**License:** MIT

### Current TC Support

The `src/tc/` module structure:
```
src/tc/
├── actions/          # TC actions (mirror, nat, tunnel_key)
├── filters/          # TC filters (flower, u32, matchall)
├── qdiscs/           # Qdisc implementations
│   ├── fq_codel.rs   # Fair Queuing CoDel - IMPLEMENTED
│   ├── ingress.rs    # Ingress qdisc - IMPLEMENTED
│   └── mod.rs
├── stats/            # TC statistics
├── tests/
├── attribute.rs
├── header.rs
├── message.rs
├── mod.rs
└── options.rs
```

### What's Missing

| Qdisc | Status | tcgui Usage |
|-------|--------|-------------|
| **netem** | Not implemented | Core - delay, loss, jitter, corruption, reorder |
| **TBF** (Token Bucket Filter) | Not implemented | Rate limiting |
| **HTB** (Hierarchical Token Bucket) | Not implemented | Alternative rate limiting |
| pfifo_fast | Not implemented | Default qdisc |
| noqueue | Not implemented | - |

## Implementation Plan

### Phase 1: Add netem qdisc support

Create `src/tc/qdiscs/netem.rs` following the fq_codel.rs pattern:

```rust
// Marker struct
pub struct TcQdiscNetem {}
impl TcQdiscNetem {
    pub const KIND: &'static str = "netem";
}

// Options enum for netem parameters
pub enum TcQdiscNetemOption {
    // Core parameters
    Latency(u32),           // Delay in microseconds
    Limit(u32),             // Queue limit
    Loss(u32),              // Loss percentage (0-100% scaled)
    Gap(u32),               // Reorder gap
    Duplicate(u32),         // Duplicate percentage
    Jitter(u32),            // Jitter in microseconds
    
    // Extended parameters  
    Corrupt(TcNetemCorrupt),
    Reorder(TcNetemReorder),
    Rate(TcNetemRate),
    
    Other(DefaultNla),
}

// Nested structs for complex parameters
pub struct TcNetemCorrupt {
    pub probability: u32,
    pub correlation: u32,
}

pub struct TcNetemReorder {
    pub probability: u32,
    pub correlation: u32,
}

pub struct TcNetemRate {
    pub rate: u64,          // Bytes per second
    pub packet_overhead: i32,
    pub cell_size: u32,
    pub cell_overhead: i32,
}
```

Required trait implementations:
- `Nla` - For attribute serialization
- `Parseable<NlaBuffer>` - For deserialization
- `Emitable` - For buffer writing

### Phase 2: Add TBF qdisc support

Create `src/tc/qdiscs/tbf.rs`:

```rust
pub struct TcQdiscTbf {}
impl TcQdiscTbf {
    pub const KIND: &'static str = "tbf";
}

pub enum TcQdiscTbfOption {
    Rate(u64),              // Rate in bytes/sec
    Burst(u32),             // Burst size
    Limit(u32),             // Queue limit
    Mtu(u32),               // MTU
    Peakrate(u64),          // Peak rate
    Minburst(u32),          // Minimum burst
    Other(DefaultNla),
}
```

### Phase 3: rtnetlink integration (if needed)

The [rtnetlink](https://github.com/rust-netlink/rtnetlink) crate provides `QDiscHandle` with:
- `get()` - List qdiscs
- `add()` - Create qdisc
- `change()` - Modify qdisc
- `replace()` - Create or replace
- `del()` - Delete qdisc

Currently `QDiscNewRequest` only has:
- `ingress()` - Create ingress qdisc
- `handle()`, `parent()`, `root()` - Generic options

May need to add:
- `netem(options)` - Create netem qdisc with options
- `tbf(options)` - Create TBF qdisc with options

## Reference: Linux Kernel TC Attributes

From `linux/pkt_sched.h`:

```c
// netem attributes
enum {
    TCA_NETEM_UNSPEC,
    TCA_NETEM_CORR,         // Correlation
    TCA_NETEM_DELAY_DIST,   // Delay distribution
    TCA_NETEM_REORDER,      // Reordering
    TCA_NETEM_CORRUPT,      // Corruption
    TCA_NETEM_LOSS,         // Loss model
    TCA_NETEM_RATE,         // Rate limiting
    TCA_NETEM_ECN,          // ECN marking
    TCA_NETEM_RATE64,       // 64-bit rate
    TCA_NETEM_PAD,
    TCA_NETEM_LATENCY64,    // 64-bit latency
    TCA_NETEM_JITTER64,     // 64-bit jitter
    TCA_NETEM_SLOT,         // Slot parameters
    TCA_NETEM_SLOT_DIST,    // Slot distribution
};

// TBF attributes  
enum {
    TCA_TBF_UNSPEC,
    TCA_TBF_PARMS,          // Main parameters
    TCA_TBF_RTAB,           // Rate table
    TCA_TBF_PTAB,           // Peak rate table
    TCA_TBF_RATE64,         // 64-bit rate
    TCA_TBF_PRATE64,        // 64-bit peak rate
    TCA_TBF_BURST,          // Burst size
    TCA_TBF_PBURST,         // Peak burst
    TCA_TBF_PAD,
};
```

## Existing Go Implementation Reference

The [vishvananda/netlink](https://github.com/vishvananda/netlink) Go library has complete netem and TBF support that can serve as reference:

**Netem struct:**
```go
type Netem struct {
    QdiscAttrs
    Latency       uint32  // in us
    DelayCorr     float32 // in %
    Limit         uint32
    Loss          float32 // in %
    LossCorr      float32 // in %
    Gap           uint32
    Duplicate     float32 // in %
    DuplicateCorr float32 // in %
    Jitter        uint32  // in us
    ReorderProb   float32 // in %
    ReorderCorr   float32 // in %
    CorruptProb   float32 // in %
    CorruptCorr   float32 // in %
}
```

**TBF struct:**
```go
type Tbf struct {
    QdiscAttrs
    Rate      uint64
    Limit     uint32
    Buffer    uint32
    Peakrate  uint64
    Minburst  uint32
}
```

## Effort Estimate

| Task | Complexity | Files |
|------|------------|-------|
| netem qdisc implementation | Medium-High | 1-2 new files |
| TBF qdisc implementation | Medium | 1-2 new files |
| Unit tests | Medium | Test files |
| rtnetlink integration | Low-Medium | Modify existing |
| Documentation | Low | README updates |

## Recommendation

1. **Fork netlink-packet-route** first
2. Implement netem following fq_codel.rs pattern
3. Add TBF support
4. Test with tcgui-backend
5. Consider upstreaming changes via PR

## Alternative: mmynk/rust-tc

[rust-tc](https://github.com/mmynk/rust-tc) (8 stars, v0.0.4) is an alternative but:
- Read-only (no write/update/delete)
- Less active development
- Smaller community

The rust-netlink ecosystem is more mature and actively maintained.
