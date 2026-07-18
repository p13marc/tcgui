# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed
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
