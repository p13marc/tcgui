# nlink 0.4.0 Migration Report

**Date:** 2026-01-03  
**Project:** tcgui  
**Previous Version:** nlink 0.3.0  
**New Version:** nlink 0.4.0  

## Migration Status: Success

All changes have been applied successfully:
- Build: Passing
- Tests: 323+ tests passing
- Clippy: No warnings

## Summary of Breaking Changes

### 1. Message Struct Fields Now Private

**Change:** All message struct fields (`LinkMessage`, `TcMessage`, etc.) are now `pub(crate)` with public accessor methods.

**Impact on tcgui:**
- `msg.name` → `msg.name()` (returns `Option<&str>`)
- `msg.kind` → `msg.kind()` (returns `Option<&str>`)
- `link.name.clone()` → `link.name().map(|s| s.to_string())`

**Assessment:** This is a good change. It enables future internal changes without breaking the public API and provides a cleaner interface with consistent accessor patterns.

### 2. Connection Now Requires Generic Parameter

**Change:** `Connection` is now `Connection<P>` where `P` is a protocol type like `Route`.

**Impact on tcgui:**
- `Connection::new(Protocol::Route)` → `Connection::<Route>::new()`
- `Connection::new_in_namespace_path(Protocol::Route, path)` → `Connection::<Route>::new_in_namespace_path(path)`
- Struct fields and function parameters needed type updates

**Assessment:** This is an excellent change. Type-safe connections prevent runtime errors from protocol mismatches and make the API more self-documenting.

### 3. Qdisc Options API Simplified

**Change:** 
- Removed `netem_options()` convenience method
- New `options()` method returns parsed `QdiscOptions` enum

**Before:**
```rust
if let Some(netem) = qdisc.netem_options() {
    println!("delay: {:?}", netem.delay());
}
```

**After:**
```rust
use nlink::netlink::tc_options::QdiscOptions;
if let Some(QdiscOptions::Netem(netem)) = qdisc.options() {
    println!("delay: {:?}", netem.delay());
}
```

**Assessment:** Good change. The enum-based approach is more extensible for supporting other qdisc types (HTB, TBF, etc.) and makes it explicit what type of options you're working with.

### 4. NetemOptions Methods Return Option<T>

**Change:** Methods now return `Option<Duration>` or `Option<f64>` instead of raw values.

**Before:**
```rust
if netem.delay().as_micros() > 0 {
    println!("delay: {:?}", netem.delay());
}
```

**After:**
```rust
if let Some(delay) = netem.delay() {
    println!("delay: {:?}", delay);
}
```

**New accessor methods:**
- `delay()` → `Option<Duration>`
- `jitter()` → `Option<Duration>`
- `loss()` → `Option<f64>`
- `duplicate()` → `Option<f64>`
- `reorder()` → `Option<f64>`
- `corrupt()` → `Option<f64>`
- `rate_bps()` → `Option<u64>`

**Assessment:** Excellent change. This is more idiomatic Rust - using `Option` to represent "not set" rather than checking for zero values. It prevents bugs where zero might be a valid value.

### 5. EventStream API Removed

**Change:** `EventStream` and `EventStreamBuilder` removed. Use `Connection<Route>::subscribe()` + `events()` instead.

**Before:**
```rust
let mut stream = EventStream::builder()
    .links(true)
    .tc(true)
    .namespace("myns")
    .build()?;

while let Some(event) = stream.try_next().await? {
    // handle event
}
```

**After:**
```rust
let mut conn = Connection::<Route>::new_in_namespace_path("/var/run/netns/myns")?;
conn.subscribe(&[RtnetlinkGroup::Link, RtnetlinkGroup::Tc])?;
let mut events = conn.events();

while let Some(result) = events.next().await {
    let event = result?;
    // handle event
}
```

**Assessment:** Mixed feelings. The new API is more consistent with the typed connection pattern and gives more control, but the builder pattern was more ergonomic for simple use cases. The subscription via enum is cleaner though.

### 6. RouteGroup Renamed to RtnetlinkGroup

**Change:** Renamed to better reflect that it covers all rtnetlink groups.

**Assessment:** Good naming improvement - more accurate and self-documenting.

## What Works Well in nlink 0.4.0

1. **Type-safe connections** - `Connection<Route>` prevents protocol mismatches at compile time
2. **Consistent accessor pattern** - All message types now use methods instead of public fields
3. **Option-based returns** - Idiomatic Rust for optional values
4. **Extensible qdisc options** - Enum-based approach supports multiple qdisc types
5. **Cleaner event subscription** - Using an enum for groups is more type-safe

## Areas for Improvement

### 1. Correlation Accessors Missing

The new API provides accessors for primary values (`loss()`, `delay()`, etc.) but correlation values are still accessed as raw fields:
- `netem_opts.delay_corr`
- `netem_opts.loss_corr`
- `netem_opts.duplicate_corr`
- `netem_opts.reorder_corr`
- `netem_opts.corrupt_corr`

**Recommendation for nlink 0.5.0:** Add Option-returning accessors for correlations:
```rust
fn delay_correlation(&self) -> Option<f64>
fn loss_correlation(&self) -> Option<f64>
// etc.
```

### 2. ECN and Gap Fields Still Public

The `ecn` and `gap` fields are still accessed directly:
- `netem_opts.ecn`
- `netem_opts.gap`

**Recommendation for nlink 0.5.0:** Add accessor methods:
```rust
fn ecn(&self) -> bool
fn gap(&self) -> Option<u32>
```

### 3. Named Namespace Connection

The `new_in_namespace()` method takes a raw file descriptor, not a namespace name. For named namespaces, you must construct the path manually:

```rust
// Current workaround
let path = PathBuf::from(format!("/var/run/netns/{}", name));
Connection::<Route>::new_in_namespace_path(&path)
```

**Recommendation for nlink 0.5.0:** Add a convenience method:
```rust
fn new_in_named_namespace(name: &str) -> Result<Self>
```

### 4. Event Stream Ownership

The `events()` method borrows from the connection. For async tasks, `into_event_stream()` exists which consumes the connection and returns an owned stream:

```rust
tokio::spawn(async move {
    let mut events = conn.into_event_stream();  // consumes conn
    while let Some(result) = events.next().await { ... }
});
```

**Note:** tcgui now uses `into_event_stream()` for cleaner ownership semantics in async tasks.

### 5. NamespaceSpec Integration

tcgui uses `NamespaceSpec` for namespace handling, which still works via `spec.connection()`. It would be nice if this returned `Connection<Route>` explicitly in the type signature.

## Recommendations for nlink 0.5.0

### High Priority

1. **Complete NetemOptions accessor coverage**
   - Add `delay_correlation()`, `loss_correlation()`, etc.
   - Add `ecn()` and `gap()` accessors
   - Make all internal fields private

2. **Named namespace convenience**
   - Add `Connection::<Route>::new_in_named_namespace("myns")`

### Medium Priority

3. **NamespaceSpec typed returns**
   - `spec.connection()` should return `Connection<Route>` not just `Connection`

### Low Priority

4. **Builder pattern for event subscription** (optional)
   - Some users might prefer the old builder ergonomics
   - Could add `ConnectionBuilder` that configures subscriptions before building

5. **Documentation improvements**
   - Add migration guide in CHANGELOG
   - Show common patterns for async usage

## Conclusion

The nlink 0.4.0 migration was successful. The new API is more type-safe, more idiomatic Rust, and better positioned for future evolution. The main gaps are:

1. Incomplete accessor coverage for NetemOptions (correlations, ecn, gap)
2. Minor ergonomic issues with named namespaces and event stream ownership

These are minor issues that don't block usage. Overall, nlink 0.4.0 is a significant improvement over 0.3.0.

## Files Modified

| File | Changes |
|------|---------|
| `Cargo.toml` | Version bump 0.3.0 → 0.4.0 |
| `tcgui-backend/src/tc_commands.rs` | QdiscOptions enum, Connection<Route>, accessor methods |
| `tcgui-backend/src/netlink_events.rs` | EventStream → Connection subscribe API |
| `tcgui-backend/src/network.rs` | Connection<Route> typing |
| `tcgui-backend/src/bandwidth.rs` | Connection<Route> typing |
| `tcgui-backend/src/container.rs` | Connection<Route>, name accessor |
| `tcgui-backend/src/main.rs` | NetemOptions accessor methods |
| `tcgui-backend/examples/test_netns.rs` | Connection<Route>, name accessor |

## Test Results

```
test result: ok. 323 passed; 0 failed; 0 ignored
```

All existing tests continue to pass with no modifications needed to test code.
