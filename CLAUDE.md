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
├── tcgui-backend/    # Privileged service (CAP_NET_ADMIN): tc commands, rtnetlink
└── tcgui-frontend/   # Iced GUI: displays interfaces, sends TC requests
```

### Communication Pattern (Zenoh)

**Pub/Sub (Backend → Frontend):**
- `tcgui/{backend}/interfaces/list` - Interface discovery
- `tcgui/{backend}/bandwidth/{namespace}/{interface}` - Real-time stats
- `tcgui/{backend}/interfaces/events` - State changes
- `tcgui/{backend}/health` - Backend health

**Query/Reply (Frontend → Backend):**
- `tcgui/{backend}/query/tc` - TC operations (TcRequest/TcResponse)
- `tcgui/{backend}/query/interface` - Interface enable/disable
- `tcgui/{backend}/query/scenario` - Scenario CRUD operations
- `tcgui/{backend}/query/scenario/execution` - Start/stop/pause/resume scenarios

**Pub/Sub (Scenario Updates):**
- `tcgui/{backend}/scenario/execution/{namespace}/{interface}` - Execution status updates

**Pub/Sub (Preset Updates):**
- `tcgui/{backend}/presets/list` - Available presets (built-in + custom)

### Key Backend Components

- `main.rs` - Application entry, Zenoh session, query handlers
- `network.rs` - Interface discovery via rtnetlink
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
- **rtnetlink**: Linux netlink for interface enumeration
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
