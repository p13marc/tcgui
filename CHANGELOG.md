# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed
- Upgraded `nlink` 0.25 → 0.26. No source change on the netlink paths: the
  interface dump, the netem apply/replace/remove spine, the ethtool and
  nl80211 enrichment probes, `StatsTracker`, the resync event streams and
  `util::parse::get_rate` all keep their signatures and semantics. All six
  root-gated live tests still pass against a real kernel.

  Two things did move. **`schemars` 0.8 is gone from the tree** — nlink 0.26
  moves to schemars 1.0, which is what this workspace already used, so the
  duplicate collapses.

  And `tcgui-backend` now sets `#![recursion_limit = "256"]`. nlink 0.26
  `Box::pin`s `Connection::send_dump` to close the *layout*-depth recursion
  class (nlink #315), which trades depth in one solver for depth in another:
  proving `Send` for a future that awaits down the netlink request chain now
  recurses past rustc's default limit of 128, and `tokio::spawn` needs
  exactly that proof. The failure surfaces on an unrelated-looking
  `tokio::spawn` in `scenario/zenoh_handlers.rs`. The limit belongs to the
  calling crate — which is the point nlink's own changelog makes about who
  can fix this class — and it is compile-time only, with no runtime cost.
- **Adopted the `zenkey` convention crates (crates.io)**: the hand-rolled
  keyspace-v2 layer is now built on `zenkey` 0.2 + `zenkey-build`. The
  subject/procedure vocabulary lives in `tcgui-shared/registry/tc.toml`
  (linted per RFC 08 §5 at build time); `tcgui_shared::topics` keeps its
  public API but builds and parses every key through the generated typed
  registry, and the backend serves the same file on `@rpc/tc/introspect` —
  one source of truth for keys and slice. Origin minting now uses zenkey's
  reference derivation with the existing `tcgui-host-id-v1` salt, so
  machine-id-derived host origins are unchanged; hosts on the persisted
  random fallback re-key once.
- Added `zblob` (crates.io): the backend serves the `@blob/artifact` plane
  (empty for now — the diagnostics/support-bundle download will publish
  through it).
- Upgraded `zenkey` / `zenkey-build` 0.6 → 0.7. Machine-id-derived host
  origins are **unchanged** (0.7's `HostId::digest` is byte-identical). But
  0.7 rewrote the chunk slugger's boundary sentinel from `e` to `x`, so the
  key for any interface or namespace name that *starts or ends with a
  non-`[a-z0-9]` character* changes — `ETH0` was `e_x45__x54__x48_0` and is
  now `x_x45__x54__x48_0`; `_myns` was `e_myns` and is now `x_x5f_myns`.
  Clean names (`eth0`, `eth0.100`, `default`) are byte-identical, so a normal
  deployment sees no wire change at all.
- Upgraded `zenkey` / `zenkey-build` 0.7 → 0.8, which **re-keys escaped names
  again** — and this time because of a bug this repo filed (tcgui#39). The 0.7
  slugger was not injective: the escaped form of `_myns` was `x_x5f_myns`,
  which is itself a legal value, so a namespace actually named `x_x5f_myns`
  shared a key with one named `_myns`. Found here, slugging Linux device
  names, where both spellings are legal.

  zenkey 0.8 (RFC 03 §2 v1.31) reserves the prefix `x-` on both sides of the
  boundary: a value passes through only if it is charset-legal *and* does not
  start with `x-`; otherwise it is `x-` followed by a body in which every byte
  outside `[a-z0-9]` becomes `_xHH`, with no closing underscore. So `ETH0` is
  now `x-_x45_x54_x480` (`x_x45__x54__x48_0` under 0.7) and `_myns` is now
  `x-_x5fmyns` (`x_x5f_myns` under 0.7), while `x_x5f_myns` passes through
  untouched — the collision is gone.

  As with 0.6 → 0.7: **clean names are byte-identical**, so `eth0`,
  `eth0.100`, `default` and every lowercase ULID leaf keep their key, and a
  normal deployment sees no wire change at all. Machine-id-derived host
  origins are unchanged (12 hex digits is a clean value).

  `zenkey_slug_outputs_are_pinned` is updated to the 0.8 table, and a new
  `zenkey_slug_is_injective_and_reversible` asserts the property the pin table
  could not express — that no two names reach the same chunk, and that
  `chunk_unslug` (0.8's left inverse, shipped for exactly this) recovers each
  one. A pinned output table would have passed happily through the 0.7 bug.
- The `@rpc` procedures whose path carries `{ns}/{iface}` now declare
  `cardinality = 1024`, matching the `state` subjects that address the same
  interface population — `zenkey-build` 0.7 extends the key-population budget
  lint (RFC 08 §2) from subjects to procedures.
- **BREAKING (wire):** `TcResponse`, `InterfaceControlResponse` and
  `DiagnosticsResponse` lost their `success: bool` and `error_code:
  Option<i32>` fields. A value reply now always means success and a failure
  always rides Zenoh's reply-error channel with a namespaced `error/...` name
  (RFC keyspace-v2 05 §3), so both fields were dead — `success` was `true` and
  `error_code` `None` on every reply a consumer could ever observe. There are
  no `#[serde(default)]`s on them, so a 0.8 peer cannot deserialize a 0.9
  reply.
- Every request-decode failure now answers on the reply-error channel.
  Missing, oversize, non-UTF-8 and malformed-JSON payloads previously
  propagated out of their handler into a caller that only logged, so the
  querier received **no reply at all** and timed out. The interface and
  diagnostics handlers also gained the payload size guard they never had.
- **BREAKING (wire):** migrated to the keyspace-v2 `tcgui/v1/<origin>/<class>/tc/<subject…>`
  grammar (this records the cutover in 125e46c, which never got an entry).
  The Zenoh session namespace is `tcgui`, so app code never spells the base.
  Keys are now built from a **host origin** (`h-<12 hex>`, derived from the
  machine id) instead of the operator-chosen backend name, which becomes a
  display label in the health document and is never a key discriminator.
  Liveliness moved to its own `state/tc/alive` leaf, split from the health
  document. Per-interface `state/tc/interface/{ns}/{if}` records with Delete
  tombstones replace the `interfaces/list` + `interfaces/events` pair, and a
  cleared TC config is a `SampleKind::Delete` rather than a `None` payload.
  `@rpc/tc/introspect` serves the registry and `@rpc/tc/describe` the schema
  set. **0.8.x and 0.9.0 peers do not interoperate.**
- Three subjects the registry declared since 1.0 but nothing ever published are
  now live: `state/tc/sensor` (producer registration — version and the raw
  namespace list, which is the one place a non-chunk-clean namespace name
  survives losslessly), `events/tc/applied/{ulid}` (an audit record for
  operator-driven applies only — a scenario step is excluded, or a scenario
  stepping every 500ms would blow the events class rate budget), and
  `state/tc/scenario/{id}` (the scenario library, file templates included).
- Removing an interface now retracts its `state/tc/config/{ns}/{if}` key with a
  Delete. Previously the publisher was dropped without a tombstone, so the last
  config written for a vanished NIC stood on the state plane forever and a
  late-joining GUI showed shaping for an interface that no longer existed.
- Removed the dead pre-cutover types `InterfaceListUpdate`,
  `InterfaceStateEvent` and `InterfaceEventType`, and repointed the stale
  `Topic: tcgui/{backend_name}/…` doc comments at `registry/tc.toml`.

## [0.8.0] - 2026-05-05

### Changed
- Upgraded nlink from 0.8.0 to 0.15.1 (typed-units rollout: `TcHandle`, `Percent`, `Rate` at TC API boundaries)
- Upgraded zenoh / zenoh-ext from 1.5.1 to 1.9
- Upgraded bollard from 0.18 to 0.21 (`query_parameters` module + builder pattern for list/inspect options)
- Upgraded iced_aw from 0.13 to 0.14
- Upgraded dirs from 5 to 6
- Upgraded nix from 0.30 to 0.31
- Unified `thiserror` on workspace 2.0 (backend was on 1.0)
- `qdisc.parent()` / `qdisc.handle()` now use `TcHandle` instead of raw `u32` (root check via `is_root()`)
- Netem rate construction uses `Rate::kbit(...)` instead of `rate::kbps_to_bytes`

### Fixed
- Pre-existing clippy patterns flagged by newer toolchain (`collapsible_match`, `collapsible_if`, `unnecessary_sort_by`)

## [0.7.0] - 2026-01-18

### Changed

- **deps**: Update nlink from 0.6.0 to 0.8.0
  - Fixes namespace interface resolution bug where TC operations in network namespaces
    would fail because sysfs-based name resolution read from the host namespace
  - nlink 0.8.0 uses netlink-based resolution, making `namespace::connection_for()` 
    work correctly for all TC operations
  - API change: `get_qdiscs_for()` renamed to `get_qdiscs_by_name()`

## [0.6.0] - 2026-01-17

### Added

- **frontend**: Dual-control inputs with chips, slider, NumberInput for TC features
- **frontend**: Grid layout for feature cards using iced Grid widget
- **frontend**: Research-based presets for network condition simulation

### Changed

- **frontend**: Replaced dropdowns with expanded chips + slider for correlation values
- **frontend**: Improved text sizing and NumberInput proportions

### Fixed

- **frontend**: Feature cards now properly fill grid cell width
- **frontend**: Cards align to top with `align_y(Start)`
- **frontend**: Reduced card height with compact spacing
