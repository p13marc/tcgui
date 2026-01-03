# nlink 0.5.0 Migration Report

**Date:** 2026-01-03  
**Previous Version:** 0.4.0  
**New Version:** 0.5.0

## Summary

Successfully migrated tcgui-backend from nlink 0.4.0 to 0.5.0. The update required handling two breaking changes and allowed adoption of new convenience accessor methods.

## Breaking Changes Addressed

### 1. `into_event_stream()` Renamed to `into_events()`

For consistency with Rust naming conventions (`iter()`/`into_iter()` pattern), the method was renamed.

**Files affected:** `tcgui-backend/src/netlink_events.rs`

```rust
// Before
let mut events = conn.into_event_stream();

// After
let mut events = conn.into_events();
```

**Locations:**
- `NetlinkEventListener::start()` (line 168)
- `NamespaceEventManager::process_namespace_events()` (line 389)

### 2. `NetemOptions` Fields Now Private

All `NetemOptions` fields are now `pub(crate)` with public accessor methods. This required updating code that directly accessed fields.

**Files affected:** `tcgui-backend/src/main.rs`

#### ECN Field

```rust
// Before
let ecn_val = netem_opts.ecn;

// After
let ecn_val = netem_opts.ecn();
```

#### Correlation Fields

```rust
// Before
let delay_correlation = if netem_opts.delay_corr > 0.0 {
    Some(netem_opts.delay_corr as f32)
} else {
    None
};

// After (cleaner with Option::filter)
let delay_correlation = netem_opts
    .delay_correlation()
    .filter(|&c| c > 0.0)
    .map(|c| c as f32);
```

All correlation accessors updated:
| Old Field | New Accessor |
|-----------|--------------|
| `delay_corr` | `delay_correlation()` |
| `loss_corr` | `loss_correlation()` |
| `duplicate_corr` | `duplicate_correlation()` |
| `reorder_corr` | `reorder_correlation()` |
| `corrupt_corr` | `corrupt_correlation()` |

#### Gap Field

```rust
// Before
let reorder_gap = if netem_opts.gap > 0 {
    Some(netem_opts.gap)
} else {
    None
};

// After
let reorder_gap = netem_opts.gap().filter(|&g| g > 0);
```

## New Features Available (Not Yet Adopted)

The following nlink 0.5.0 features are now available for future use:

### LinkMessage Statistics Helpers

Convenience methods that delegate to `stats()`:
- `total_bytes()`, `total_packets()`, `total_errors()`, `total_dropped()`
- `rx_bytes()`, `tx_bytes()`, `rx_packets()`, `tx_packets()`
- `rx_errors()`, `tx_errors()`, `rx_dropped()`, `tx_dropped()`

Currently using `NlinkLinkStats::from_link_message()` which works fine.

### TcMessage Convenience Methods

- `is_class()` - Check if this is a TC class
- `is_filter()` - Check if this is a TC filter
- `handle_str()` - Get handle as human-readable string (e.g., "1:0")
- `parent_str()` - Get parent as human-readable string (e.g., "root")

Useful for debugging and logging.

### Additional NetemOptions Accessors

- `delay_ns()`, `jitter_ns()` - Raw nanosecond values
- `loss_percent()`, `duplicate_percent()`, `reorder_percent()`, `corrupt_percent()` - Raw percentages
- `packet_overhead()`, `cell_size()`, `cell_overhead()` - Rate limiting overhead values
- `limit()` - Queue limit in packets
- `slot()` - Slot-based transmission configuration
- `loss_model()` - Loss model configuration

### Additional Error Checks

- `is_address_in_use()` - EADDRINUSE
- `is_name_too_long()` - ENAMETOOLONG
- `is_try_again()` - EAGAIN
- `is_no_buffer_space()` - ENOBUFS
- `is_connection_refused()` - ECONNREFUSED
- `is_host_unreachable()` - EHOSTUNREACH
- `is_message_too_long()` - EMSGSIZE
- `is_too_many_open_files()` - EMFILE
- `is_read_only()` - EROFS

## Files Modified

| File | Changes |
|------|---------|
| `Cargo.toml` | Updated nlink version 0.4.0 → 0.5.0 |
| `tcgui-backend/src/netlink_events.rs` | Renamed `into_event_stream()` → `into_events()` (2 locations) |
| `tcgui-backend/src/main.rs` | Updated `NetemOptions` field access to use accessor methods |

## Verification

### Build Status
```
✓ cargo build -p tcgui-backend
✓ cargo build (full workspace)
```

### Test Status
```
✓ 127 tests passed
✓ 0 tests failed
```

## Code Quality Improvements

The migration resulted in cleaner code patterns:

1. **Option chaining**: Using `.filter()` and `.map()` instead of manual if-else blocks
2. **Encapsulation**: Better API stability as internal field representation can now change without breaking user code
3. **Consistency**: Accessor pattern matches Rust conventions

## Rollback Instructions

If issues are discovered, revert by:

1. Change `Cargo.toml`: `nlink = { version = "0.4.0", features = ["full"] }`
2. Revert `netlink_events.rs`: `into_events()` → `into_event_stream()`
3. Revert `main.rs`: Use direct field access instead of accessor methods
