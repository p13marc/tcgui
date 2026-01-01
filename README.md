# TC GUI

Linux traffic control (tc netem) GUI with network namespace support. Split-architecture: unprivileged Iced frontend communicates with a privileged Rust backend via Zenoh.

## Features

- **Traffic Control**: Loss, delay, duplicate, reorder, corrupt, rate limiting
- **Network Scenarios**: Predefined sequences of TC changes over time
- **Namespace Support**: Monitor and control interfaces across namespaces
- **Real-time Monitoring**: Live bandwidth statistics

## Quick Start

```bash
# Build and set capabilities
just build
just set-caps

# Run (two terminals)
just run-backend    # Terminal 1: requires sudo first time for caps
just run-frontend   # Terminal 2
```

Or use `just run` for automatic backend management.

## Architecture

```
Frontend (unprivileged)          Backend (CAP_NET_ADMIN)
        │                                │
        ├──── Pub/Sub ◄────────────────┤ Interface lists, bandwidth, health
        │                                │
        └──── Query/Reply ─────────────► TC operations, scenario execution
                        
                    Zenoh
```

### Crates

| Crate | Description |
|-------|-------------|
| `tcgui-frontend` | Iced GUI application |
| `tcgui-backend` | Privileged service (tc, nlink) |
| `tcgui-shared` | Common types and messages |

## Development

```bash
just dev          # Format + check + clippy + test
just dev-fast     # Format + fast-clippy + fast-test (60% faster)
just dev-minimal  # Format + fast-test (~2 seconds)
```

See [CLAUDE.md](CLAUDE.md) for full development commands.

## Scenarios

Scenarios define sequences of TC configurations applied over time. See [docs/scenario-format.md](docs/scenario-format.md) for the JSON5 format specification.

```json5
{
    id: "example",
    name: "Example Scenario",
    steps: [
        { duration: "30s", description: "Add delay", tc_config: { delay: { base_ms: 100 } } },
        { duration: "30s", description: "Add loss", tc_config: { loss: { percentage: 5 } } },
    ],
}
```

Scenarios are loaded from:
- `./scenarios`
- `~/.config/tcgui/scenarios`
- `/usr/share/tcgui/scenarios`

## Security

The backend uses Linux capabilities instead of root:

```bash
sudo setcap cap_net_admin+ep target/release/tcgui-backend
```

This grants only network administration privileges, not full root access.

## Requirements

- Linux with tc/netem support
- Rust 1.70+
- `just` command runner

## Documentation

- [Scenario Format](docs/scenario-format.md) - JSON5 specification
- [Best Practices](docs/best-practices.md) - Scenario design guidelines
- [Troubleshooting](docs/troubleshooting.md) - Common issues
- [Examples](docs/examples.md) - Annotated scenarios

## License

MIT
