# TC GUI - Network Traffic Control Interface

A modern, graphical interface for managing Linux network traffic control (tc) with network namespace support. Built with Rust, featuring a split-architecture design for security and performance.

## 🌟 Features

### Core Traffic Control
- **🏛️ Simple Traffic Control**: One-click feature toggles for `tc netem` - no complex workflows
- **⚡ Instant Application**: Check a feature checkbox and it applies immediately - no "Apply" buttons
- **🎯 Complete TC Features**: Loss, Delay, Duplicate, Reorder, Corrupt, and Rate Limiting with full parameter control
- **📊 Network Namespace Support**: Monitor and control interfaces across multiple network namespaces
- **🎯 Comprehensive Interface Types**: Supports Physical, Virtual, Veth, Bridge, TUN, TAP, and Loopback interfaces

### Network Scenarios (NEW!) 🎬
- **📋 Scenario Templates**: Built-in templates for common network testing patterns
- **🚀 Fast Network Degradation**: 30-second rapid testing scenario with 10 progressive steps
- **📱 Mobile Device Simulation**: Progressive signal degradation as device moves away from base station
- **🌐 Network Congestion**: Daily usage patterns with varying congestion levels
- **🔌 Intermittent Connectivity**: Connection drops and recovery patterns
- **🎯 Interface Selection**: Interactive dialog to choose target namespace and interface
- **🔄 Real-time Execution**: Live step progression with visual feedback in UI
- **⏯️ Execution Control**: Pause, resume, and stop running scenarios
- **🔄 Auto-refresh**: Scenarios automatically reload when switching to Scenarios tab

### Monitoring & UI
- **⚡ Real-time Monitoring**: Live bandwidth monitoring with automatic unit formatting (B/s, KB/s, MB/s, GB/s)
- **🌐 Modern UI**: Built with Iced for responsive, native performance with tabbed interface
- **📈 Rate Calculations**: Real-time bandwidth rate monitoring with counter wraparound handling
- **🎮 Active Executions**: Live monitoring of running scenarios with progress tracking
- **📊 Step Progression**: Real-time step counter showing current progress ("Step 3/10")

### Architecture & Security
- **🔒 Security-First Architecture**: Privilege separation with frontend/backend split
- **🌐 Multi-Backend Support**: Named backend instances for distributed deployments
- **📡 Modern Communication**: Zenoh pub/sub + query/reply patterns for scalable messaging
- **🛡️ Permission Handling**: Graceful handling of namespace access restrictions
- **🔧 Fixed TC Synchronization**: Scenario execution properly updates frontend UI in real-time

## 🏗️ Architecture

TC GUI uses a modern split-binary architecture with separate pub/sub and query/reply communication patterns:

```
┌───────────────────────┐    Zenoh Topics     ┌──────────────────────┐
│   Frontend (GUI)    │ ◄─────────────────── │    Backend (Root)    │
│                     │                     │                      │
│ • Tabbed Interface  │  Pub/Sub Topics:    │ • Network Operations │
│ • Scenario Manager  │  • Interface Lists  │ • TC Command Exec    │
│ • Interface Dialog  │  • Bandwidth Stats  │ • Scenario Engine    │
│ • Real-time Stats   │  • Execution Updates │ • Template System    │
│ • Progress Monitor  │  • Health Status    │ • Multi-NS Support   │
│ • No Privileges     │  • Interface Events │ • Bandwidth Monitor  │
│                     │                     │                      │
│                     │  Query/Reply:       │                      │
│                     │  • TC Operations ──► │                      │
│                     │  • Scenario Ops ──►  │                      │
│                     │  • Interface Ctrl ──►│                      │
└───────────────────────┘                     └──────────────────────┘
```

### Components

- **Frontend (`tcgui-frontend`)**: Unprivileged GUI application for user interaction
- **Backend (`tcgui-backend`)**: Privileged service handling network operations
- **Shared (`tcgui-shared`)**: Common message types and communication protocol

### Communication Patterns

**Pub/Sub Topics** (Backend → Frontend):
- `tcgui/{backend}/interfaces/list` - Interface discovery updates
- `tcgui/{backend}/bandwidth/{namespace}/{interface}` - Real-time bandwidth statistics
- `tcgui/{backend}/interfaces/events` - Interface state changes
- `tcgui/{backend}/health` - Backend health status

**Query/Reply Services** (Frontend → Backend):
- `tcgui/{backend}/query/tc` - Traffic control operations
- `tcgui/{backend}/query/interface` - Interface enable/disable operations

## 🚀 Quick Start

### Prerequisites

- **Linux System** (tested on Fedora Linux)
- **Rust 1.70+** with edition 2024 support
- **`just` command runner** (install with: `sudo dnf install just`)
- **Linux capabilities support** (CAP_NET_ADMIN)
- **Network namespaces** support (optional, for advanced features)

### Installation & Quick Start

1. **Install prerequisites and clone**:
   ```bash
   # Install just command runner
   # Fedora/RHEL:
   sudo dnf install just
   # Ubuntu/Debian:
   # sudo apt install just
   # Arch:
   # sudo pacman -S just
   # Or install from source:
   # cargo install just
   
   # Clone and setup
   git clone https://github.com/your-username/tcgui.git
   cd tcgui
   just setup
   ```

2. **Run the application**:
   ```bash
   just run
   ```

That's it! The frontend automatically spawns and manages the backend process.

### Alternative: Manual Setup

If you prefer manual control:

1. **Build and set capabilities**:
   ```bash
   just build-backend
   just set-caps
   ```

2. **Run backend manually**:
   ```bash
   just run-backend
   # OR directly:
   ./target/release/tcgui-backend --exclude-loopback --verbose --name trefze3
   ```

3. **Run frontend** (separate terminal):
   ```bash
   ./target/debug/tcgui-frontend --verbose
   # OR with automatic backend spawning:
   ./target/debug/tcgui-frontend --backend trefze3 --verbose
   ```

### Available Just Commands

```bash
# Quick Development Workflows
just dev                # Complete development cycle (format, check, lint, test)
just dev-fast           # Ultra-fast cycle (format, fast-lint, test) - 60% faster
just dev-minimal        # Minimal cycle (format, test only) - ~2 seconds
just dev-backend        # Backend-only development cycle
just dev-frontend       # Frontend-only development cycle

# Build Commands
just build              # Build all components
just build-backend      # Build backend only
just build-frontend     # Build frontend only
just build-release      # Build all components (release mode)

# Quality Assurance
just quality            # Full quality verification pipeline
just pre-commit         # Pre-commit quality gate
just test               # Full test suite
just test-fast          # Fast test suite (lib targets only)
just clippy             # Lint analysis (strict mode)
just clippy-fast        # Fast lint analysis (lib targets only)
just coverage           # Code coverage analysis
just security           # Security vulnerability audit

# Package Generation
just package            # Generate all packages (DEB + RPM)
just package-deb        # Generate DEB packages only
just package-rpm        # Generate RPM packages only
just list-packages      # List generated packages
just validate-packages  # Validate package structure
just test-packages      # Test package installation (requires sudo)

# Local CI Testing
just local-ci           # Docker-free CI simulation
just local-check        # Fast local quality checks
just validate-workflows # Validate GitHub Actions workflows

# Run Commands
just run                # Run frontend with auto backend
just run-backend        # Run backend manually

# Setup and Maintenance
just setup-tools        # Install all quality tools
just setup-packaging-tools # Install DEB/RPM packaging tools
just setup              # Complete setup (build + capabilities)
just clean              # Clean build artifacts
just help               # Show detailed help
```

## 🎮 Usage

### Getting Started

1. **Launch the application**:
   ```bash
   just run  # Automatic backend + frontend
   ```
   
2. **Choose your workflow**:
   - **Manual Traffic Control**: Direct interface configuration
   - **Network Scenarios**: Automated network condition sequences

### Network Scenarios (Recommended for Testing) 🎬

1. **Navigate to Scenarios tab** (scenarios auto-load)
2. **Choose a scenario template**:
   - **Fast Network Degradation**: 30-second demo with 10 progressive steps
   - **Mobile Device Simulation**: Signal degradation over 2 minutes
   - **Network Congestion**: 5-minute congestion patterns
   - **Intermittent Connectivity**: Connection drop patterns
   - **Quality Degradation**: Gradual service quality decline

3. **Execute a scenario**:
   - Click **"▶️ Execute"** on any scenario
   - **Select interface**: Choose namespace and target interface from dialog
   - Click **"Execute Scenario"** to start

4. **Monitor execution**:
   - Watch **"Active Executions"** section for progress
   - Observe **step progression**: "Step 3/10 (45.2%)"
   - See **real-time UI updates**: checkboxes and sliders reflect current TC state
   - Use **execution controls**: ⏸️ Pause, ▶️ Resume, ⏹️ Stop

### Manual Traffic Control

1. **Navigate to Interfaces tab**
2. **Select a network interface** from the namespace-grouped list
3. **Configure traffic control features**:
   - **Loss (LSS)**: Check to enable packet loss, adjust percentage (0-100%) and correlation
   - **Delay (DLY)**: Check to enable packet delay, set base delay, jitter, and correlation
   - **Duplicate (DUP)**: Check to enable packet duplication with percentage and correlation
   - **Reorder (RO)**: Check to enable packet reordering with percentage, correlation, and gap
   - **Corrupt (CR)**: Check to enable packet corruption with percentage and correlation
   - **Rate Limit (RL)**: Check to enable bandwidth limiting in kbps
4. **Features apply immediately** when checked - no separate "Apply" button needed
5. **Monitor results** in the status messages and interface icons
6. **Remove features** by unchecking their checkboxes

### Network Namespace Support

The GUI automatically discovers and displays interfaces grouped by network namespace:

- **Default namespace**: Standard system interfaces
- **Named namespaces**: Custom network namespaces (if any exist)
- **Real-time updates**: Interface changes are reflected automatically

### Bandwidth Monitoring

- **Live statistics**: Real-time RX/TX bandwidth rates calculated from `/proc/net/dev`
- **Comprehensive metrics**: Bytes, packets, errors, and drops tracking
- **Automatic formatting**: Units automatically scaled (B/s, KB/s, MB/s, GB/s)
- **Namespace isolation**: Statistics tracked per namespace+interface key
- **Rate calculations**: Proper handling of counter wraparounds and time deltas
- **2-second intervals**: Regular updates without overwhelming the system

## 🔧 Development

### Project Structure

```
tcgui/
├── Cargo.toml                 # Workspace configuration
├── tcgui-shared/              # Shared library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs            # Message protocol & types
│       └── errors.rs         # Error handling
├── tcgui-backend/             # Privileged backend
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs           # Backend service
│       ├── network.rs        # Interface monitoring
│       ├── tc_commands.rs    # TC command execution
│       └── bandwidth.rs      # Bandwidth monitoring
└── tcgui-frontend/            # GUI frontend
    ├── Cargo.toml
    └── src/
        ├── main.rs           # Application entry
        ├── app.rs            # Main application logic
        ├── interface.rs      # Interface component
        ├── messages.rs       # Message types
        └── zenoh_manager.rs  # Communication layer
```

### Development Commands

```bash
# Check all crates
cargo check --workspace

# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Lint with clippy
cargo clippy --workspace

# Format code
cargo fmt --all

# Build documentation
cargo doc --workspace --open
```

### Key Technologies

- **[Rust](https://rust-lang.org/)**: Systems programming language
- **[Iced](https://github.com/iced-rs/iced)**: Native GUI framework
- **[Zenoh](https://zenoh.io/)**: High-performance pub-sub messaging
- **[rtnetlink](https://github.com/rust-netlink/rtnetlink)**: Linux netlink interface
- **[tokio](https://tokio.rs/)**: Async runtime

## 🛡️ Security Model

### Privilege Separation with Linux Capabilities

- **Frontend**: Runs as regular user, no elevated permissions
- **Backend**: Uses Linux capabilities (`CAP_NET_ADMIN`) instead of root
- **Communication**: Secure pub-sub messaging via Zenoh
- **Read-Only Frontend**: Cannot create/modify network namespaces
- **Automatic Cleanup**: Frontend manages backend lifecycle when using `--backend`

### Capabilities-Based Security

Instead of running the backend as root, TC GUI uses Linux capabilities for enhanced security:

```bash
# Set capabilities (done automatically by 'just set-caps')
sudo setcap cap_net_admin+ep target/release/tcgui-backend

# Verify capabilities
getcap target/release/tcgui-backend
# Output: target/release/tcgui-backend cap_net_admin=ep
```

**Benefits of capabilities over root:**
- ✅ **Principle of least privilege**: Only network admin capabilities
- ✅ **Enhanced security**: Cannot access files, processes, or other system resources
- ✅ **User-friendly**: Regular users can run the backend without sudo
- ✅ **Audit trail**: Capability usage is logged by the kernel

### Network Operations

The backend handles all privileged operations:
- Interface discovery via rtnetlink
- TC command execution (no sudo required with capabilities)
- Network namespace monitoring
- Bandwidth statistics collection

## 🌐 Network Namespace Features

### Supported Operations

- ✅ **Discover existing namespaces** automatically
- ✅ **Monitor interfaces** across all namespaces  
- ✅ **Apply TC netem** to interfaces in any namespace
- ✅ **Real-time interface updates** per namespace
- ❌ **Create/delete namespaces** (read-only design)
- ❌ **Create virtual interfaces** (read-only design)

### Interface Types Supported

- **Physical**: eth0, wlan0, etc.
- **Virtual**: veth pairs, bridges
- **Loopback**: lo interface
- **TUN/TAP**: Virtual network devices

## 📊 Monitoring Capabilities

### Real-time Statistics

- **Interface Status**: UP/DOWN state monitoring
- **Bandwidth Tracking**: RX/TX bytes and packets per second
- **TC Configuration**: Detection of active netem rules
- **Namespace Changes**: Dynamic interface discovery

### Performance Metrics

- **Low Latency**: Sub-second update intervals
- **Efficient Updates**: Only changed data transmitted
- **Scalable**: Supports many interfaces across namespaces

## 🐛 Troubleshooting

### Common Issues

1. **Backend won't start**:
   - Check capabilities are set: `just check-caps`
   - Set capabilities if missing: `just set-caps`
   - Check Zenoh port availability (default ports)

2. **Permission denied errors**:
   - Verify capabilities: `getcap target/release/tcgui-backend`
   - Re-set capabilities: `just set-caps`
   - For debug builds: `just set-caps-debug`

3. **Frontend can't connect**:
   - Verify backend is running: `ps aux | grep tcgui-backend`
   - Check firewall settings
   - Enable verbose logging: `just run` (includes verbose output)

4. **TC commands failing**:
   - Ensure `tc` utility is installed: `which tc`
   - Check capabilities instead of sudo permissions
   - Verify interface exists: `ip link show`

5. **No interfaces visible**:
   - Confirm backend has capabilities: `just check-caps`
   - Check rtnetlink permissions
   - Verify network interfaces exist: `ip link show`

### Debug Logging

Enable detailed logging for troubleshooting:

```bash
# Easy debugging with just
just run                    # Frontend with auto-backend (verbose)
just run-backend           # Backend only (verbose)

# Manual debugging
RUST_LOG=debug ./target/release/tcgui-backend --verbose --name trefze3
RUST_LOG=debug ./target/debug/tcgui-frontend --backend trefze3 --verbose
```

### Capabilities Troubleshooting

```bash
# Check current capabilities
just check-caps

# Remove and re-set capabilities
just remove-caps
just set-caps

# Verify tc command works without sudo
./target/release/tcgui-backend --help
```

## 📦 Distribution & Packaging

### Package Installation

TC GUI provides native packages for major Linux distributions:

#### Fedora/RHEL/CentOS (RPM)
```bash
# Install both backend and frontend packages
sudo rpm -i tcgui-backend-*.rpm tcgui-frontend-*.rpm

# Enable and start the backend service
sudo systemctl enable --now tcgui-backend

# Launch the GUI application
tcgui-frontend
```

#### Debian/Ubuntu (DEB)
```bash
# Install both packages
sudo dpkg -i tcgui-backend_*.deb tcgui-frontend_*.deb
sudo apt-get install -f  # Fix any missing dependencies

# Enable and start the backend service
sudo systemctl enable --now tcgui-backend

# Launch the GUI application
tcgui-frontend
```

### Package Features

- **🔒 Secure System Integration**: Systemd service with security hardening
- **🔧 Automatic Setup**: Sudoers configuration for network operations
- **🖥️ Desktop Integration**: Menu entry and application icon
- **📋 Complete Documentation**: Comprehensive man pages and README files
- **⚡ Easy Installation**: Single-command installation with dependency resolution

### Building Packages

Developers can build packages locally:

```bash
# Install packaging tools
just setup-packaging-tools

# Generate all packages (DEB + RPM)
just package

# Generate specific formats
just package-deb      # Debian/Ubuntu packages
just package-rpm      # Fedora/RHEL packages

# List generated packages
just list-packages

# Test package installation (requires sudo)
just test-packages
```

## 🏭 Quality Assurance & Development

### Comprehensive Quality System

TC GUI implements a rigorous quality assurance system with:

- **🚫 Zero Tolerance**: Zero compiler warnings, zero clippy issues, zero dead code
- **⚡ Fast Feedback**: Ultra-fast development workflows (2-30 seconds)
- **🔐 Security First**: Automated security vulnerability scanning
- **📊 Full Coverage**: Code coverage analysis and testing
- **🚀 Local CI**: Docker-free local testing that mimics GitHub Actions

### Development Workflow Options

```bash
# Ultra-fast iteration (2 seconds)
just dev-minimal      # Format + tests only

# Balanced development (30 seconds) 
just dev-fast         # Format + fast-lint + tests

# Complete validation (80 seconds)
just dev              # Format + check + lint + tests

# Component-specific
just dev-backend      # Backend development only
just dev-frontend     # Frontend development only
```

### Quality Tools

#### Core Quality Checks
- **Code Formatting**: `cargo fmt` with consistent style
- **Compilation**: Zero warnings policy with `cargo check`
- **Linting**: Strict `cargo clippy` analysis
- **Testing**: Comprehensive test suite with `cargo test`
- **Coverage**: Code coverage with `cargo-tarpaulin`

#### Security & Dependencies
- **Security Audit**: `cargo audit` for vulnerability scanning
- **Dependency Analysis**: `cargo deny` for license and security
- **Unused Dependencies**: `cargo udeps` for cleanup
- **Dead Code Analysis**: `cargo machete` for elimination
- **Outdated Dependencies**: `cargo outdated` for updates

#### Advanced Analysis
- **Memory Safety**: Miri verification for unsafe code
- **Documentation**: Complete API docs with `cargo doc`
- **Performance**: Benchmarking and optimization analysis

### Local CI Testing

Test your changes locally without Docker:

```bash
# Complete CI simulation
just local-ci

# Fast quality checks
just local-check

# Validate GitHub Actions workflows
just validate-workflows

# Component-specific testing
./scripts/local-ci.sh backend   # Backend only
./scripts/local-ci.sh frontend  # Frontend only
./scripts/local-ci.sh security  # Security analysis
```

### Pre-commit & CI Integration

```bash
# Pre-commit quality gate
just pre-commit       # Essential checks before commit

# Pre-push verification
just pre-push         # Comprehensive checks before push

# Complete quality pipeline
just quality          # Full verification (matches CI)
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes
4. Run tests: `cargo test --workspace`
5. Check formatting: `cargo fmt --all`
6. Run clippy: `cargo clippy --workspace`
7. Commit changes: `git commit -m 'Add amazing feature'`
8. Push to branch: `git push origin feature/amazing-feature`
9. Create a Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🎆 Architecture Improvements

### Recent Communication Refactor

The project has been completely refactored from a simple pub-sub architecture to a modern, scalable communication system:

**Previous Architecture:**
- Single pub-sub topic for all communication
- Mixed message types on shared topics
- Limited scalability for multiple backends

**New Architecture:**
- **Pub/Sub Topics**: High-frequency data (bandwidth, interface updates, health)
- **Query/Reply Services**: Command operations (TC config, interface control)
- **Multi-Backend Support**: Named backend instances with topic isolation
- **Type Safety**: Strongly-typed messages for each communication pattern
- **Scalability**: Independent scaling of pub/sub vs command operations

### Benefits

- ⭐ **Better Performance**: Optimized communication patterns for different data types
- ⭐ **Multi-Backend Ready**: Support for distributed deployments
- ⭐ **Type Safety**: Compile-time message validation
- ⭐ **Maintainability**: Clear separation of concerns
- ⭐ **Future-Proof**: Extensible architecture for new features

## 🎨 User Experience Design

### Simplified Interface

TC GUI features a streamlined, direct-manipulation interface:

- **✅ Direct Feature Control**: Each traffic control feature (Loss, Delay, etc.) has its own checkbox
- **✅ Immediate Application**: Checking a feature applies it instantly - no separate "Apply" button
- **✅ Visual Status**: Interface icons show TC status (🔧 = TC active, 📡 = normal)
- **✅ Parameter Sliders**: Expandable parameter controls appear when features are enabled
- **✅ Auto-Cleanup**: Backend intelligently removes TC qdisc when no features are active

### Smart Parameter Management

- **Automatic Defaults**: Features use sensible defaults when enabled (1% loss, 10ms delay, etc.)
- **Real-time Updates**: Parameter changes apply immediately while features are enabled
- **Complete Removal**: Unchecking features properly removes parameters from TC qdisc
- **Conflict Resolution**: Backend uses delete+add strategy to ensure clean parameter removal

## 🙏 Acknowledgments

- **Linux TC Team** for the powerful traffic control subsystem
- **Iced Team** for the excellent GUI framework
- **Zenoh Team** for high-performance messaging
- **Rust Community** for the amazing ecosystem
