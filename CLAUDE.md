# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TC GUI is a Linux network traffic control (tc netem) graphical interface with a security-focused split-architecture: an unprivileged Iced GUI frontend communicates with a privileged Rust backend via Zenoh pub/sub messaging.

## Build and Development Commands

```bash
# Build
just build                    # Build all (debug)
just build-release            # Build all (release)
just build-backend            # Build backend only
just build-frontend           # Build frontend only

# Run (requires two terminals or use run-backend in background)
just run-backend              # Run backend (debug, requires sudo)
just run-frontend             # Run frontend (debug)

# Development workflows
just dev                      # Format + check + clippy + tests
just dev-fast                 # Format + fast-clippy + fast-tests (60% faster)
just dev-minimal              # Format + fast-tests (~2 seconds)
just dev-backend              # Backend-only cycle
just dev-frontend             # Frontend-only cycle

# Quality checks
just fmt                      # Format code
just check                    # Compile check (zero warnings policy)
just clippy                   # Lint (strict, warnings as errors)
just test                     # Full test suite
just test-fast                # Fast tests (lib targets only)
just test-live                # Live tc tests against the kernel (needs root; skipped by `just ci`)
just coverage                 # Code coverage with tarpaulin

# Pre-commit
just pre-commit               # Essential quality gate before commits

# Component-specific
cargo test -p tcgui-backend --lib
cargo test -p tcgui-frontend --lib
cargo clippy -p tcgui-shared -- -D warnings
```

## Architecture

### Three-Crate Workspace

```
tcgui/
├── tcgui-shared/     # Common types: messages, NetworkInterface, TcConfiguration
├── tcgui-backend/    # Privileged service (CAP_NET_ADMIN): tc commands, nlink
└── tcgui-frontend/   # Iced GUI: displays interfaces, sends TC requests
```

### Communication Pattern (Zenoh)

Every key follows the keyspace-v2 grammar:

```
<base>/v1/<origin>/<class>/<producer>/<subject…>
  tcgui   v1  h-<12hex>  state|telemetry|events|@rpc   tc   …
```

`base = tcgui` is the Zenoh session **namespace**, so app code never spells it.
`origin` is the host id minted from the machine id — **not** the backend name,
which is a display label in the health document and never a key discriminator.

**Do not enumerate the keys here.** The vocabulary lives in exactly one place,
`tcgui-shared/registry/tc.toml`, which `zenkey-build` compiles into typed
builders and which the backend serves verbatim on `@rpc/tc/introspect`. Four
files each keeping their own copy of this table is why they all drifted after
the cutover. To see the live vocabulary:

```bash
zenctl topic list --base tcgui          # from the bus, via introspect
cat tcgui-shared/registry/tc.toml       # the source of truth
```

Classes, and what they mean for a writer:

- `state/` — last-writer-wins documents. **Removal is a `SampleKind::Delete`
  tombstone, never a `None` payload.**
- `telemetry/` — superseded samples; no history, best effort.
- `events/` — immutable, rate-budgeted (`rate = "low"`, ≤1/min). An operator
  action earns a record; a scenario step does not.
- `@rpc/` — the verbatim procedure plane. **A value reply always means success;
  a failure always rides the reply-error channel** with a namespaced `error/…`
  name. Queryables here are never `complete`.

Writes are origin-scoped and concrete, always: `RemoteOrigin::parse` rejects a
wildcard, so a fleet-wide write has no spelling.

### Key Backend Components

- `main.rs` - Application entry, Zenoh session, query handlers
- `network.rs` - Interface discovery via nlink
- `tc_commands.rs` - TC netem execution with intelligent parameter removal
- `bandwidth.rs` - `/proc/net/dev` parsing per namespace
- `preset_loader.rs` - Custom preset loading from directories
- `scenario/` - Scenario system:
  - `execution.rs` - Scenario execution engine, step timing, pause/resume
  - `manager.rs` - Scenario storage and template loading
  - `loader.rs` - JSON5 file loading from directories
  - `zenoh_handlers.rs` - Query handlers for scenario operations

### Key Frontend Components

- `main.rs` - Iced application entry
- `app.rs` - Main state, namespace grouping, message routing
- `interface.rs` - TcInterface component with feature checkboxes
- `zenoh_manager.rs` - Pub/sub subscriptions, query/reply client
- `messages.rs` - UI message types
- `scenario_view.rs` - Scenario list, execution cards, progress UI
- `scenario_manager.rs` - Scenario state, execution tracking, queries

### TC Feature Model

The frontend uses `TcFeatures` with individual `TcFeature<T>` for: Loss, Delay, Duplicate, Reorder, Corrupt, RateLimit. Each has an `enabled` checkbox and config struct. Backend uses delete+add strategy when removing parameters (tc netem replace preserves old values).

## Key Technologies

- **Iced 0.14**: GUI framework with tokio integration
- **Zenoh**: Pub/sub + query/reply messaging
- **zenkey / zenkey-build** (crates.io): the keyspace-v2 convention. The subject/
  procedure vocabulary is `tcgui-shared/registry/tc.toml`, compiled by
  `zenkey-build` from `tcgui-shared/build.rs` into typed builders
  (`tcgui_shared::registry::tc`); `tcgui_shared::topics` wraps them (public API
  unchanged) and the backend serves the same TOML on `@rpc/tc/introspect`.
  Never build keys with `format!` — go through `topics`/the generated registry.
  Origins mint via `zenkey::AppProfile("tcgui", "tcgui-host-id-v1")` — changing
  the salt re-keys every fleet.
- **zblob** (crates.io): resumable blob transfer; the backend serves the
  (currently empty) `@blob/artifact` plane for future diagnostics bundles.
- **nlink**: Linux netlink for interface enumeration and TC operations
- **tokio**: Async runtime

## Security Model

- Frontend runs unprivileged
- Backend uses Linux capabilities (`CAP_NET_ADMIN`) instead of root
- Set capabilities: `just set-caps` (calls `setcap cap_net_admin+ep`)

## Preset System

Custom presets define reusable network condition configurations. JSON5 format with implicit `enabled: true` for present features.

- **Loading**: `./presets`, `~/.config/tcgui/presets`, `/usr/share/tcgui/presets`
- **Built-in**: SatelliteLink, CellularNetwork, PoorWiFi, WanLink, UnreliableConnection, etc.
- **Usage**: Select in UI dropdown or reference by ID in scenario steps

Key components:
- `tcgui-shared/src/preset_json.rs` - JSON5 parsing for preset files
- `tcgui-shared/src/presets.rs` - PresetSource, PresetList, CustomPreset types
- `tcgui-backend/src/preset_loader.rs` - File loading and PresetResolver

See `docs/preset-format.md` for format specification.

## Scenario System

Scenarios define sequences of TC configurations applied over time. JSON5 format with human-readable durations.

- **Loading**: `./scenarios`, `~/.config/tcgui/scenarios`, `/usr/share/tcgui/scenarios`
- **Execution**: One scenario per interface, multiple interfaces can run simultaneously
- **Features**: Pause/resume, loop mode, cleanup on failure, real-time progress
- **Preset References**: Steps can use `preset: "preset-id"` instead of inline `tc_config`

See `docs/scenario-format.md` for format specification.

## Code Quality Standards

- Zero compiler warnings policy
- Clippy with `-D warnings` (warnings as errors)
- No dead code (cargo machete)
- No unused dependencies (cargo udeps)
