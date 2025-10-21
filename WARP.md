# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

**tcgui** is a Rust workspace containing a split-binary application for controlling network traffic shaping using Linux's tc (traffic control) command with **network namespace support**. It consists of a modular frontend GUI built with Iced and a privileged backend that handles network operations, communicating via Zenoh pub-sub messaging.

The application provides **comprehensive network scenario management** with built-in templates, **interface selection dialogs**, **real-time execution monitoring**, and secure traffic control operations across multiple network namespaces. The latest version includes a complete **scenario execution engine** with dynamic network condition changes over time.

### Latest Major Features (December 2024)

- **🎬 Network Scenarios**: Complete scenario execution system with built-in templates
- **🚀 Fast Network Degradation**: 30-second rapid testing scenario (10 progressive steps)
- **🎯 Interface Selection Dialog**: Interactive namespace and interface chooser
- **📊 Real-time Execution Monitoring**: Live step progression with "Step 3/10" display
- **⏯️ Execution Controls**: Pause, resume, and stop running scenarios
- **🔄 Auto-refresh**: Scenarios automatically reload when switching tabs
- **🔧 Fixed TC Synchronization**: Scenario execution properly updates frontend UI
- **🌐 Tabbed Interface**: Modern UI with Interfaces and Scenarios tabs

### Workspace Structure

The project is organized as a Rust workspace with three crates:

```
tcgui/
├── Cargo.toml                   # Workspace configuration
├── README.md                    # Main project documentation
├── tcgui-shared/                # Shared library crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs              # Message types and protocol definitions
│       └── errors.rs           # Error handling types
├── tcgui-backend/               # Backend binary crate  
│   ├── Cargo.toml
│   ├── README.md               # Backend-specific documentation
│   └── src/
│       ├── main.rs             # Service entry point and event loop
│       ├── network.rs          # Network interface monitoring
│       ├── tc_commands.rs      # Traffic control execution
│       └── bandwidth.rs        # Bandwidth statistics collection
└── tcgui-frontend/              # Frontend binary crate
    ├── Cargo.toml
    ├── README.md               # Frontend-specific documentation
    └── src/
        ├── main.rs             # Application entry point
        ├── app.rs              # Main application logic
        ├── interface.rs        # Interface component
        ├── messages.rs         # Message type definitions
        └── zenoh_manager.rs    # Communication layer
```

### Architecture

The application is split into two binaries for security and privilege separation:

#### Frontend Binary (`tcgui-frontend`)
- **Tabbed Interface**: Modern UI with Interfaces and Scenarios tabs
- **Scenario Management**: Complete scenario execution and monitoring system
- **Interface Selection**: Interactive dialog for choosing target namespace/interface
- **Real-time Progress**: Live step progression tracking ("Step 3/10 (45.2%)")
- **Execution Controls**: Pause, resume, and stop scenario operations
- **Auto-refresh**: Automatic scenario reload when switching to Scenarios tab
- **Namespace-Grouped UI**: Interfaces organized by network namespace with scrollable view
- **Real-time Monitoring**: Live bandwidth statistics and status updates
- **Zenoh Client**: Communicates with backend via pub-sub messaging
- **Read-only Namespaces**: Cannot create/modify namespaces (security design)
- **No Privileges Required**: Runs as regular user with input validation
- **Responsive Design**: Handles many interfaces with smooth scrolling

#### Backend Binary (`tcgui-backend`)
- **Scenario Execution Engine**: Complete system for running dynamic network scenarios
- **Built-in Templates**: 6 scenario templates including Fast Network Degradation (30s)
- **Real-time Updates**: Frequent execution progress updates with step tracking
- **TC Configuration Publishing**: Fixed synchronization between scenario execution and UI
- **Network Operations**: rtnetlink integration for multi-namespace interface monitoring
- **TC Command Execution**: Namespace-aware TC operations with structured config support
- **Bandwidth Monitoring**: Real-time statistics collection from `/proc/net/dev`
- **Interface Discovery**: Automatic detection across multiple network namespaces
- **Zenoh Server**: Receives commands and publishes updates with health monitoring
- **Requires Root/CAP_NET_ADMIN**: Needs elevated privileges for network operations

#### Shared Library (`tcgui-shared`)
- **Scenario System**: Complete scenario types (NetworkScenario, ScenarioExecution, ScenarioStep)
- **Execution State**: ExecutionState enum with Running/Paused/Stopped/Completed/Failed
- **Template Support**: ScenarioMetadata with tags, versioning, and template flags
- **TC Configuration**: TcNetemConfig with structured parameter support
- **Message Protocol**: Namespace-aware structured communication types
- **Scenario Messages**: ScenarioRequest/Response for management operations
- **Execution Messages**: ScenarioExecutionRequest/Response for runtime control
- **Error Handling**: Comprehensive error types for backend and frontend
- **Network Types**: NetworkNamespace, NetworkInterface, InterfaceType definitions
- **Bandwidth Statistics**: NetworkBandwidthStats with rate calculations
- **Zenoh Topics**: Topic definitions and routing for pub-sub communication
- **Serde Integration**: JSON serialization for message passing

### Key Technologies

- **Iced 0.14.0-dev**: Modern GUI framework with scrollable widgets and responsive design
- **Zenoh 1.5.1**: High-performance pub-sub messaging with health monitoring and liveliness
- **rtnetlink 0.18.1**: Linux netlink interface for multi-namespace network operations
- **tokio**: Full-featured async runtime for concurrent operations
- **tracing & tracing-subscriber**: Structured logging with configurable levels
- **serde & serde_json**: JSON serialization for cross-process communication
- **clap**: Command-line argument parsing with help generation
- **futures-util**: Stream processing for network interface monitoring
- **anyhow & thiserror**: Comprehensive error handling and reporting

## Development Commands

### Modern Just-Based Workflow

The project uses **Just** (command runner) for streamlined development with comprehensive quality assurance:

#### Fast Development Cycles (NEW!)
```bash
# Ultra-fast iteration (2 seconds) - Perfect for rapid development
just dev-minimal      # Format + tests only

# Balanced development (30 seconds) - Good speed/quality balance
just dev-fast         # Format + fast-lint + tests (60% faster than full dev)

# Complete validation (80 seconds) - Full quality verification
just dev              # Format + check + lint + tests

# Component-specific workflows
just dev-backend      # Backend-only development cycle
just dev-frontend     # Frontend-only development cycle
```

#### Quality Assurance Pipeline
```bash
# Pre-commit quality gate (essential checks)
just pre-commit       # Format-check + check + clippy + test-fast + security-fast

# Complete quality verification (matches WARP.md standards)
just quality          # Full pipeline: format + check + clippy + test + coverage + security + deps

# Continuous integration pipeline
just ci               # Format-check + check + clippy + test + coverage + security + deps
```

#### Individual Quality Checks
```bash
# Core checks
just fmt              # Format code (auto-fix)
just check            # Compilation check (zero warnings policy)
just clippy           # Lint analysis (strict mode)
just clippy-fast      # Fast lint analysis (lib targets only)
just test             # Full test suite
just test-fast        # Fast test suite (lib targets only)

# Advanced analysis
just coverage         # Code coverage with HTML report
just security         # Security vulnerability audit
just unused-deps      # Check for unused dependencies
just outdated-deps    # Check for outdated dependencies
just deadcode         # Dead code detection across workspace
just miri-key-modules # Memory safety verification for critical modules
```

### Workspace Commands

#### Build Commands
```bash
# Build entire workspace
just build            # Build all components (debug)
just build-release    # Build all components (release)

# Build specific components
just build-backend    # Backend only (release mode)
just build-frontend   # Frontend only (debug mode)

# Check compilation without building
cargo check --workspace              # All crates
cargo check -p tcgui-backend        # Specific crate
```

### Package Generation & Distribution (NEW!)

The project includes comprehensive DEB and RPM packaging for Linux distributions:

#### Setup Packaging Tools
```bash
# Install packaging tools (cargo-deb, cargo-generate-rpm)
just setup-packaging-tools
```

#### Generate Packages
```bash
# Generate all packages (DEB + RPM for both components)
just package

# Generate specific formats
just package-deb                    # Debian/Ubuntu packages only
just package-rpm                    # Fedora/RHEL/CentOS packages only

# Generate specific components
just package-backend deb            # Backend DEB package only
just package-frontend rpm           # Frontend RPM package only
```

#### Package Management
```bash
# List generated packages with metadata
just list-packages

# Validate package structure and content
just validate-packages

# Test package installation/removal (requires sudo)
just test-packages

# Clean old packages and artifacts
just clean-packages
```

### Local CI Testing (NEW!)

Docker-free local CI simulation that matches GitHub Actions:

#### Local CI Commands
```bash
# Complete CI simulation (all checks)
just local-ci

# Fast local quality checks (development)
just local-check

# Component-specific testing
./scripts/local-ci.sh backend       # Backend analysis only
./scripts/local-ci.sh frontend      # Frontend analysis only
./scripts/local-ci.sh security      # Security analysis only
./scripts/local-ci.sh deps          # Dependency management only

# Workflow validation
just validate-workflows              # Validate GitHub Actions YAML files
```

#### Benefits of Local CI
- **⚡ Faster Execution**: No Docker overhead, native Fedora performance
- **🔧 Zero Dependencies**: Works without Docker installation
- **🎨 Rich Output**: Colored logging with progress indicators
- **🔄 Graceful Degradation**: Works even with missing optional tools
- **📊 Comprehensive Coverage**: Matches GitHub Actions workflows exactly

#### Running the Application

**Start Backend First (requires root privileges):**
```bash
# Run backend with elevated privileges
sudo cargo run -p tcgui-backend

# Or with debug logging
sudo RUST_LOG=debug cargo run -p tcgui-backend -- --verbose
```

**Start Frontend (separate terminal):**
```bash
# Run frontend as regular user
cargo run -p tcgui-frontend

# Or with debug logging
RUST_LOG=debug cargo run -p tcgui-frontend -- --verbose
```

**Alternative: Use Pre-built Binaries**
```bash
# Backend (requires sudo)
sudo ./target/debug/tcgui-backend --verbose

# Frontend (regular user)
./target/debug/tcgui-frontend --verbose
```

## Comprehensive Quality Assurance System (ENHANCED!)

The project implements a rigorous quality assurance system following strict standards:

### Quality Standards Enforced

1. **Zero Compiler Warnings**: All `cargo check` warnings should be resolved
2. **Zero Clippy Issues**: All clippy suggestions should be addressed  
3. **Consistent Formatting**: Code formatted with `cargo fmt`
4. **Security Verification**: No vulnerabilities in dependency tree
5. **Test Coverage**: Comprehensive test suite with coverage analysis
6. **Documentation**: Complete API documentation for all public interfaces
7. **No Unused Dependencies**: All dependencies should be actively used
8. **Current Dependencies**: Regular dependency updates and security monitoring
9. **Memory Safety**: Miri verification for critical unsafe code

### Quality Tools Installed

Run `just setup-tools` to install all quality assurance tools:

- **cargo-tarpaulin**: Code coverage analysis
- **cargo-audit**: Security vulnerability scanning
- **cargo-deny**: Dependency license and security checks
- **cargo-udeps**: Unused dependency detection
- **cargo-machete**: Dead code analysis across workspace
- **cargo-nextest**: Next-generation test runner
- **cargo-outdated**: Dependency update checking

### Code Quality Guidelines

**Recommended Practices:**

#### 🎯 **Quality Goals**
1. **Minimize Warnings**: Address compiler and clippy warnings when practical
2. **Safety First**: Avoid `unsafe` code unless absolutely necessary with proper justification
3. **Clean Code**: Remove obviously unused imports and variables
4. **Reasonable Flexibility**: Use `#[allow(...)]` annotations when appropriate for development

#### 🔧 **Development Approach**
- **Focus on Functionality**: Prioritize working features over perfect cleanliness
- **Iterative Improvement**: Address code quality issues over time
- **Practical Solutions**: Balance ideal practices with development velocity

**Rationale**: Code quality should support development productivity, not hinder it. Perfect cleanliness can be achieved incrementally.

#### 📦 **Suggested Completion Workflow**
**After completing tasks, consider running `just dev` to check for issues.**

This helps ensure:
- ✅ Code compiles correctly
- ✅ Major clippy issues are addressed
- ✅ Tests are passing
- ✅ Code is reasonably formatted

### Example Quality Workflow
```bash
# 1. Check current state
just dev                             # Fast development cycle

# 2. Pre-commit verification
just pre-commit                      # Essential checks before commit

# 3. Complete quality verification
just quality                         # Full quality pipeline

# 4. Component-specific checks (when working on specific parts)
just dev-backend                     # Backend-focused workflow
just dev-frontend                    # Frontend-focused workflow

# 5. Advanced analysis (periodic)
just coverage                        # Code coverage with HTML report
just miri-key-modules               # Memory safety verification
just unused-deps                    # Clean up unused dependencies
```

### Enhanced Testing (Modern Workflows)

#### Fast Testing (Development)
```bash
# Ultra-fast test cycle (library tests only, ~2-5 seconds)
just test-fast

# Nextest runner (parallel execution, faster than cargo test)
just test-nextest

# Component-specific tests
just test-backend                    # Backend tests only
just test-frontend                   # Frontend tests only
just test-shared                     # Shared library tests only
```

#### Comprehensive Testing
```bash
# Full test suite with all features
just test                           # Complete test suite

# Test with coverage analysis
just coverage                       # HTML coverage reports in target/tarpaulin/

# Performance regression testing
just test-bench                    # Run benchmark tests
```

#### Legacy Testing (Still Supported)
```bash
# Standard cargo testing
cargo test --workspace              # All workspace tests
cargo test -p tcgui-shared         # Specific crate tests
cargo test --workspace -- --nocapture  # With stdout output
cargo test -p <crate_name> <test_name>  # Specific test
```

#### Test Analysis
```bash
# Memory safety testing for critical modules
just miri-key-modules              # Miri verification

# Test documentation examples
cargo test --doc --workspace       # Doc tests

# Integration test isolation
cargo test --test integration      # Run integration tests only
```

## Code Patterns

### Split-Binary Architecture with Namespace Support
The application uses a secure client-server pattern with namespace-aware operations:
- **Frontend**: Iced GUI with scrollable, namespace-grouped interface display
- **Backend**: Root-privileged service supporting multiple network namespaces
- **Communication**: Pub-sub messaging via Zenoh with health monitoring and bandwidth updates
- **Security**: Frontend is read-only for namespaces, cannot create/modify network resources

### Modular Frontend Architecture (Iced)
The frontend uses a component-based architecture with clear separation:
```rust
// Main application with namespace grouping
struct TcGui {
    namespaces: HashMap<String, NamespaceGroup>,
    backend_connected: bool,
    message_sender: Option<mpsc::UnboundedSender<FrontendMessage>>,
}

// Per-namespace component grouping
struct NamespaceGroup {
    pub namespace: NetworkNamespace,
    pub tc_interfaces: HashMap<String, TcInterface>,
}

// Individual interface component with TC controls
struct TcInterface {
    name: String,
    loss_input: String,
    correlation_input: String,
    status_messages: VecDeque<StatusMessage>,
    bandwidth_stats: Option<NetworkBandwidthStats>,
}
```

### Namespace-Aware Backend Processing
TC commands are executed with namespace context:
```rust
// Backend receives namespace-aware commands
FrontendMessage::ApplyTc { namespace, interface, loss, correlation } => {
    let result = tc_manager.apply_tc_config_in_namespace(
        &namespace, &interface, loss, correlation
    ).await;
    tc_manager.send_tc_result_with_namespace(&namespace, &interface, result).await;
}
```

### Enhanced Message Protocol
Namespace-aware communication with comprehensive message types:
- **FrontendMessage**: `ApplyTc`, `RemoveTc`, `ListInterfaces`, `SubscribeUpdates`
- **BackendMessage**: `TcResult`, `InterfaceList`, `InterfaceUpdate`, `BackendStatus`, `BandwidthUpdate`
- **Topics**: 
  - `tcgui/frontend/commands` - Frontend to backend commands
  - `tcgui/backend/responses` - Backend responses and status
  - `tcgui/backend/interfaces` - Interface change notifications
  - `tcgui/backend/liveliness` - Backend health monitoring
  - `tcgui/backend/bandwidth` - Real-time bandwidth statistics

### Multi-Namespace Network Monitoring
Backend monitors interfaces across all network namespaces:
- **Default namespace**: Direct rtnetlink access for host interfaces
- **Named namespaces**: `ip netns exec` for namespace-specific operations
- **Real-time updates**: Interface state changes detected via netlink streams
- **Bandwidth tracking**: Statistics collected from `/proc/net/dev` with rate calculations

## Important Implementation Notes

### Security Model and Privileges
- **Backend requires root privileges**: Network operations require CAP_NET_ADMIN capability
- **Frontend runs unprivileged**: GUI operates as regular user with no network access
- **Read-only frontend design**: Cannot create/delete namespaces or virtual interfaces
- **Input validation**: Frontend sanitizes all user input before sending to backend
- **Privilege separation**: All network operations isolated in backend service
- **Secure communication**: Zenoh pub-sub messaging with health monitoring

### Network Namespace Operations
- **Multi-namespace support**: Discovers and monitors all existing network namespaces
- **Namespace-aware TC commands**: 
  - Default: `sudo tc qdisc replace dev eth0 root netem loss 5%`
  - Namespaced: `sudo ip netns exec test-ns tc qdisc replace dev veth0 root netem loss 5%`
- **Interface discovery**: Uses `ip -j link show` for JSON-formatted interface enumeration
- **Real-time monitoring**: Detects interface changes across all namespaces
- **Interface types**: Supports Physical, Virtual, Veth, Bridge, TUN, TAP, Loopback interfaces

### Communication and Performance
- **High-performance messaging**: Zenoh pub-sub with minimal latency
- **Health monitoring**: Backend liveliness detection and automatic reconnection
- **Bandwidth monitoring**: 2-second intervals with automatic rate calculations
- **Message serialization**: JSON format for cross-language compatibility
- **Error propagation**: Detailed error reporting from backend to frontend
- **Connection resilience**: Automatic retry and graceful degradation

### GUI Architecture and UX
- **Namespace-grouped display**: Interfaces organized by network namespace
- **Scrollable interface**: Smooth scrolling for environments with many interfaces
- **Real-time updates**: Live bandwidth statistics and interface status changes
- **Visual indicators**: Color-coded status (Connected/Disconnected, UP/DOWN, TC active)
- **Status message management**: Bounded message history to prevent memory issues
- **Responsive design**: Adapts to window resizing with consistent spacing
- **Input validation**: Range checking for loss percentage (0-100%) and correlation

## Development Environment

### System Requirements
- **Rust edition 2024**: Latest stable toolchain (1.70+)
- **Linux system**: Fedora Linux tested, requires kernel 3.2+ for netlink support
- **Network namespaces**: Optional, kernel 2.6.24+ for full namespace support
- **Root access**: Backend requires sudo privileges for network operations
- **System tools**: `tc` (traffic control), `ip` (network configuration), `sudo`

### Project Structure and Dependencies
- **Workspace-level**: Centralized dependency management with consistent versions
- **tcgui-shared**: Core types (serde, tracing, thiserror for error handling)
- **tcgui-backend**: Network ops (rtnetlink, zenoh, futures-util, tokio, anyhow)
- **tcgui-frontend**: GUI (iced with advanced features, zenoh, clap, tracing)
- **Common patterns**: Async/await throughout, structured logging, comprehensive error handling

### Development Workflow
- **Code quality**: All warnings fixed, clippy-clean, properly formatted
- **Documentation**: Comprehensive README files for each component
- **Testing**: Workspace-wide test suite with module-specific tests
- **Incremental development**: Use `-p <crate_name>` for targeted builds
- **Clean builds**: `cargo clean` to clear all artifacts when needed

### Modern Quality Enforcement Workflow (ENHANCED!)

**RULE: Use `just dev` workflows for systematic quality enforcement.**

When working on this project, WARP should follow this enhanced workflow:

#### 1. **Session Startup Quality Check**
```bash
# Start every session with comprehensive quality verification
just pre-commit                     # Essential checks: format-check + check + clippy + test-fast + security-fast
```

#### 2. **Active Development Cycle**
```bash
# Ultra-fast iteration (recommended for rapid development)
just dev-fast                       # Format + fast-lint + tests (60% faster)

# Component-specific workflows (when working on specific parts)
just dev-backend                    # Backend-focused development
just dev-frontend                   # Frontend-focused development
```

#### 3. **Quality Gate Before Commits**
```bash
# Complete verification before committing
just quality                        # Full quality pipeline with coverage and security
```

#### 4. **Systematic Warning Resolution**
When any quality check fails:

- **Unused imports**: `cargo fix --workspace` + manual cleanup
- **Dead code warnings**: Consider removing or adding `#[allow(dead_code)]` if needed for development
- **Clippy suggestions**: Apply recommended fixes when practical
- **Unused variables**: Remove or prefix with `_` if intentionally unused
- **Deprecated features**: Update to modern alternatives

#### 5. **Advanced Analysis (Periodic)**
```bash
# Weekly/monthly quality maintenance
just unused-deps                    # Clean unused dependencies
just deadcode                       # Workspace-wide dead code detection
just miri-key-modules              # Memory safety verification
just coverage                       # Test coverage analysis with HTML reports
```

**Quality Standards Maintained**:
- ✅ **Minimize warnings**: Address warnings when practical via `just dev`
- ✅ **Reasonable code cleanliness**: Remove obvious unused code, allow development flexibility
- ✅ **Clean imports**: Remove unused imports and obvious unused code
- ✅ **Consistent formatting**: Automatic formatting via `just fmt`
- ✅ **Practical lint compliance**: Apply clippy suggestions when they improve code quality
- ✅ **Test stability**: All tests pass via `just test-fast` or `just test`
- ✅ **Security verification**: No vulnerabilities via `just security`
- ✅ **Coverage monitoring**: HTML reports via `just coverage`

### **Development-Friendly Code Management**

**GUIDELINE: Balance code cleanliness with development productivity.**

When encountering dead code warnings:

1. **Unused Functions/Methods**: 
   - If obviously not needed → Consider removing
   - If part of ongoing development → Use `#[allow(dead_code)]` temporarily
   - If it's part of a public API → Either use it in tests or document its purpose

2. **Unused Struct Fields**:
   - If the field is never read → Consider if it's needed for future features
   - If it's only written but never used → May be part of incomplete feature
   - If it's for "future use" → Document the purpose or use `#[allow(dead_code)]`

3. **Unused Imports**:
   - Remove obvious unused imports
   - Use tools like `cargo fix` to automatically clean up when practical

4. **Unused Variables**:
   - Remove if truly unused
   - Prefix with `_` if intentionally unused (e.g., destructuring)

**Rationale**: Code under development may have temporarily unused elements. Allow reasonable flexibility while maintaining overall code quality.

**Modern Just-Based Workflow Example**:
```bash
# 1. Session startup - comprehensive quality check
just pre-commit                            # Essential quality verification

# 2. Active development cycle (choose one based on speed needs)
just dev-minimal                           # Ultra-fast (~2 seconds) - format + test only
just dev-fast                              # Balanced (~30 seconds) - format + fast-lint + tests
just dev                                   # Complete (~80 seconds) - full quality checks

# 3. Manual issue resolution when quality checks fail:
# - Review output from `just dev` or `just pre-commit`
# - Address obvious dead code or add #[allow(dead_code)] if needed
# - Apply practical clippy suggestions
# - Remove unused imports via `cargo fix --workspace`
# - Clean up unused variables (remove or prefix with `_`)

# 4. Pre-commit verification (before git commit)
just quality                               # Full quality pipeline with coverage

# 5. Component-specific workflows (when working on specific parts)
just dev-backend                           # Backend-focused quality checks
just dev-frontend                          # Frontend-focused quality checks

# 6. Advanced analysis (periodic maintenance)
just unused-deps                           # Clean unused dependencies  
just deadcode                              # Workspace-wide dead code detection
just miri-key-modules                      # Memory safety verification
just coverage                              # Generate HTML coverage reports

# 7. Commit clean code
git add -A
git commit -m "quality: address major code quality issues"
```

This systematic approach ensures the codebase maintains professional quality standards and prevents technical debt accumulation.

**Dead Code Successfully Eliminated (October 2025)**:
- ✅ Removed unused `stats_history_size` field from `BandwidthService`
- ✅ Removed unused `description` and `interface_count` fields from `NamespaceInfo`
- ✅ Removed all unused logging functions from `error_handling.rs`
- ✅ Removed unused `set_retry_config` method from `ServiceResilienceManager`
- ✅ Cleaned up unused wildcard imports from `utils/mod.rs`
- ✅ **Result**: Zero dead code warnings - codebase is 100% clean

### Runtime Environment
- **Backend startup**: Must run with `sudo` before frontend
- **Separate terminals**: Run backend and frontend in different terminals
- **Logging configuration**: Use `RUST_LOG=debug` for detailed debugging
- **Network requirements**: Backend discovers namespaces automatically
- **Error monitoring**: Both components provide structured error reporting

### Debugging and Monitoring
- **Backend logs**: Network operations, TC command execution, interface discovery
- **Frontend logs**: GUI events, user interactions, Zenoh communication
- **Health monitoring**: Backend liveliness tracking and connection status
- **Performance metrics**: Bandwidth statistics, message throughput, update rates
- **Error tracking**: Comprehensive error propagation from backend to frontend

### Code Quality Standards
- **Zero warnings**: All clippy and compiler warnings resolved
- **Consistent formatting**: `cargo fmt` applied workspace-wide
- **Error handling**: Comprehensive error types with proper propagation
- **Documentation**: Comprehensive inline docs and README files for all components
- **Testing**: Unit tests for critical functionality with comprehensive coverage
- **Security**: Input validation, privilege separation, safe communication
- **Clean codebase**: Major unused methods and obvious dead code removed
- **API documentation**: Full `cargo doc` documentation for all public interfaces

## Network Scenario System (NEW - December 2024)

### Complete Scenario Execution Engine

The TC GUI project now includes a **comprehensive scenario execution system** that enables automated, dynamic network condition changes over time. This represents a major architectural enhancement that transforms the application from a manual TC control tool into a sophisticated network testing platform.

#### Core Scenario Features

##### **Built-in Template Library**
```bash
# 6 Built-in Scenario Templates Available:
1. Fast Network Degradation (30s)  - Rapid testing with 10 progressive steps
2. Mobile Device Simulation (2m)    - Signal degradation as device moves away
3. Network Congestion (5m)         - Daily usage patterns with varying load
4. Intermittent Connectivity (4m)   - Connection drops and recovery cycles
5. Quality Degradation (3m)         - Gradual service quality decline
6. Load Testing (10m)              - Stress testing with multiple impairments
```

##### **Interactive Interface Selection**
- **🎯 Dialog-based Selection**: Modal dialog for choosing target namespace and interface
- **📋 Namespace Grouping**: Interfaces organized by network namespace
- **🔍 Smart Sorting**: "default" namespace first, interfaces alphabetically sorted
- **✅ Validation**: Execute button only enabled when both namespace and interface selected
- **🎨 Visual Feedback**: Selected items highlighted with colors

##### **Real-time Execution Monitoring**
- **📊 Step Progression**: Live display showing "Step 3/10 (45.2%)"
- **⏯️ Execution Controls**: Pause, resume, and stop running scenarios
- **🔄 UI Synchronization**: Checkboxes and sliders update in real-time with scenario steps
- **📈 Progress Tracking**: Percentage complete and estimated time remaining
- **🎮 Active Executions**: Dedicated section showing all running scenarios

#### Technical Architecture

##### **Scenario Data Structures**
```rust
// Core scenario definition
struct NetworkScenario {
    id: String,
    name: String,
    description: String,
    steps: Vec<ScenarioStep>,
    loop_scenario: bool,
    metadata: ScenarioMetadata,
}

// Individual scenario step
struct ScenarioStep {
    timestamp_ms: u64,              // When to apply (ms from start)
    duration_ms: Option<u64>,       // How long to maintain
    tc_config: TcNetemConfig,       // TC configuration to apply
    description: String,            // Human-readable description
    transition_type: TransitionType, // How to transition
}

// Execution runtime state
struct ScenarioExecution {
    scenario: NetworkScenario,
    start_time: u64,
    current_step: usize,            // Currently executing step
    state: ExecutionState,          // Running/Paused/Stopped/etc
    target_namespace: String,
    target_interface: String,
    stats: ExecutionStats,
}
```

##### **Backend Execution Engine**
- **Multi-threaded Execution**: Each scenario runs in its own async task
- **Precise Timing**: Millisecond-accurate step transitions with pause/resume support
- **Control Messages**: Async message passing for pause/resume/stop operations
- **Progress Updates**: Frequent execution state updates sent to frontend
- **Resource Management**: Automatic cleanup of completed executions

##### **Frontend Scenario Management**
- **Tabbed Interface**: Dedicated "Scenarios" tab with auto-refresh
- **Template Browser**: Grid view of available scenario templates with metadata
- **Execution Dashboard**: Real-time monitoring of active scenario executions
- **Interface Dialog**: Modal selection dialog with namespace/interface picker
- **State Synchronization**: Real-time updates of TC checkboxes/sliders during execution

#### Fast Network Degradation Scenario (Showcase)

The **Fast Network Degradation** scenario serves as the primary demonstration of the system's capabilities:

```
📊 Scenario Timeline (30 seconds total):
┌─────┬─────────────────────────────────────────┬─────────────────────┐
│Step │ Time Range │ Network Condition                │ TC Configuration    │
├─────┼─────────────────────────────────────────┼─────────────────────┤
│ 1   │ 0-3s       │ Perfect connection               │ No impairments     │
│ 2   │ 3-6s       │ Light packet loss (2%)          │ Loss: 2%           │
│ 3   │ 6-9s       │ Latency introduced (100ms, 5%)   │ + Delay: 100ms     │
│ 4   │ 9-12s      │ Jitter + correlation (8%, 25%)   │ + Jitter: 50ms     │
│ 5   │ 12-15s     │ Packet duplication (12%, 3%)     │ + Duplicate: 3%    │
│ 6   │ 15-18s     │ Packet reordering (15%, 5%)      │ + Reorder: 5%      │
│ 7   │ 18-21s     │ Packet corruption (20%, 2%)      │ + Corrupt: 2%      │
│ 8   │ 21-24s     │ Rate limiting (25%, 1Mbps)       │ + Rate: 1Mbps      │
│ 9   │ 24-27s     │ Severe degradation (all active)  │ All impairments    │
│ 10  │ 27-30s     │ Network recovery                 │ Back to perfect     │
└─────┴─────────────────────────────────────────┴─────────────────────┘
```

#### Key Technical Achievements

##### **Fixed TC Synchronization Bug** 🔧
- **Problem**: Scenario execution applied TC configurations but frontend UI didn't reflect changes
- **Root Cause**: `TcOperation::ApplyConfig` handler was publishing `None` instead of actual config
- **Solution**: Implemented proper structured config to legacy parameter conversion
- **Result**: Real-time UI updates now work perfectly - checkboxes and sliders reflect scenario steps

##### **Enhanced Execution Updates**
- **Frequent Updates**: Progress updates sent at step start, after TC application, and during execution
- **Step Progression**: Proper "Step X/Y" counter that increments correctly
- **State Synchronization**: Frontend receives actual TC configuration for each step
- **Visual Feedback**: Users can watch their interface controls change as scenario progresses

##### **Auto-refresh Implementation**
- **Tab Switching**: Scenarios and templates automatically reload when user clicks "Scenarios" tab
- **Backend Discovery**: Queries all connected backends for latest scenario list
- **No Manual Refresh**: Eliminates need for "Refresh" buttons - seamless UX

#### Usage Examples

##### **Quick Testing Workflow**
```bash
# 1. Start TC GUI
just run

# 2. Navigate to "Scenarios" tab (auto-loads scenarios)
# 3. Click "▶️ Execute" on "Fast Network Degradation Test"
# 4. Select interface: Choose namespace (e.g., "default") and interface (e.g., "eth0")
# 5. Click "Execute Scenario"
# 6. Watch real-time execution:
#    - Step counter: "Step 3/10 (30.0%)"
#    - UI updates: Checkboxes and sliders change automatically
#    - tc monitor: Verify actual TC commands being applied
#    - Execution controls: Pause/Resume/Stop as needed
```

##### **Development Testing**
```bash
# Perfect for rapid network testing during development:
# - 30-second complete test cycle
# - All TC features demonstrated
# - Visual confirmation in UI
# - Automatic cleanup and recovery
```

### Impact on Development Workflow

#### **Before Scenarios**
- Manual TC parameter configuration
- Static testing conditions
- Time-consuming test setup
- Limited test scenario variety
- No automated testing sequences

#### **After Scenarios**
- **⚡ Rapid Testing**: 30-second comprehensive test
- **🔄 Automated Sequences**: Dynamic condition changes
- **🎯 Targeted Testing**: Specific network patterns
- **📊 Real-time Feedback**: Live UI synchronization
- **🎮 Interactive Control**: Pause/resume/stop capabilities
- **🔧 Development Friendly**: Perfect for testing UI features

### Quality Assurance Integration

The scenario system integrates seamlessly with the existing quality assurance framework:

```bash
# Quality checks now include scenario validation
just dev                    # Includes scenario template compilation
just test                   # Scenario execution engine tests
just clippy                 # Scenario code lint analysis
just coverage               # Scenario system code coverage

# All 6 scenario templates validated during build:
# ✅ Fast Network Degradation - 10 steps, 30s duration
# ✅ Mobile Device Simulation - 4 steps, 120s duration  
# ✅ Network Congestion - 8 steps, 300s duration
# ✅ Intermittent Connectivity - 6 steps, 240s duration
# ✅ Quality Degradation - 5 steps, 180s duration
# ✅ Load Testing - 12 steps, 600s duration
```

## Final Project Status (December 2024)

### Architecture Completion
The TC GUI project has achieved a mature, production-ready state with:

#### ✅ **Core Functionality Complete**
- **Multi-namespace traffic control**: Full support for TC operations across network namespaces
- **Real-time bandwidth monitoring**: Comprehensive statistics with namespace isolation
- **Responsive GUI**: Modern Iced-based interface with smooth scrolling and real-time updates
- **Secure architecture**: Complete privilege separation between frontend and backend

#### ✅ **Code Quality Excellence**
- **Zero compiler warnings**: All clippy and rustc warnings resolved
- **Clean architecture**: Major unused methods and obvious dead code addressed
- **Comprehensive documentation**: Full API docs via `cargo doc` for all modules
- **Extensive test coverage**: 9 bandwidth monitoring tests plus additional component tests
- **Memory safety**: Bounded message histories and proper resource management

#### ✅ **Advanced Features**
- **Namespace-aware bandwidth**: Statistics tracked with "namespace/interface" keys
- **Counter wraparound handling**: Robust rate calculations using saturating arithmetic
- **Permission graceful handling**: Smooth degradation when namespace access is restricted
- **Visual state management**: Clear separation of current vs desired interface states
- **Auto-scaling units**: Bandwidth display automatically formats B/s to GB/s

### Recent Major Improvements

#### Bandwidth Monitoring System Overhaul
- **Fixed critical namespace routing bug**: Bandwidth updates now include namespace context
- **Enhanced rate calculations**: Proper delta-based calculation with time difference handling
- **Improved error handling**: Graceful namespace access permission failures
- **Added comprehensive testing**: 9 unit tests covering parsing, rates, and namespace grouping

#### Code Quality and Documentation
- **Removed all unused methods**: Cleaned up 6+ unused methods from frontend and backend
- **Eliminated unused struct fields**: Removed unused fields like `session` in NetworkManager
- **Cleaned up unused imports**: Removed all unused import statements across crates
- **Added comprehensive API documentation**: Full rustdoc documentation for `cargo doc`
- **Updated all README files**: Current feature descriptions and usage instructions
- **Enhanced error messages**: Detailed debugging information and operation feedback

#### Frontend Improvements
- **Enhanced interface component**: Better state management and visual indicators
- **Improved bandwidth display**: Automatic unit formatting and error/drop counters
- **Better status management**: Bounded message history prevents memory growth
- **Visual progress indicators**: Real-time feedback during operations

### Technical Achievements

#### Performance Optimization
- **Efficient namespace grouping**: Interfaces batched by namespace for optimal processing
- **2-second monitoring intervals**: Balanced between responsiveness and system load
- **Delta-based rate calculations**: Minimal CPU overhead for bandwidth monitoring
- **Namespace-isolated statistics**: Prevents interface name conflicts across namespaces

#### Robustness Features
- **Counter wraparound protection**: Uses saturating arithmetic for network counter resets
- **Connection resilience**: Automatic backend detection and reconnection
- **Permission error handling**: Graceful degradation when namespace access denied
- **Memory management**: Bounded collections prevent unbounded growth

#### Developer Experience
- **Comprehensive logging**: Structured tracing throughout all components
- **Clear error propagation**: Detailed error types with context information
- **Modular architecture**: Clean separation between network, TC, and bandwidth modules
- **Excellent documentation**: Both API docs and user guides for all components

### Production Readiness

The TC GUI project is now ready for production use with:

1. **Stable Architecture**: Well-tested namespace-aware bandwidth monitoring
2. **Security Best Practices**: Complete privilege separation and input validation
3. **Comprehensive Error Handling**: Graceful degradation and detailed error reporting
4. **Professional Documentation**: Complete API documentation and usage guides
5. **Clean Codebase**: Minimal warnings, obvious dead code removed, consistent formatting
6. **Extensive Testing**: Unit tests covering critical bandwidth monitoring logic

### Future Enhancement Opportunities

While the core functionality is complete, potential areas for future enhancement include:

- **Extended TC Features**: Support for additional netem parameters (delay, jitter, reordering)
- **Configuration Persistence**: Save and restore TC configurations across reboots
- **Graphical Bandwidth Display**: Real-time charts and historical bandwidth graphs
- **Advanced Filtering**: Interface filtering by type, namespace, or activity level
- **Export Functionality**: Export bandwidth statistics and TC configurations

The project demonstrates excellent Rust development practices and serves as a comprehensive example of secure, high-performance system programming with modern GUI frameworks.

## Comprehensive Quality Assurance System (Updated November 2025)

### **Quality-First Development Philosophy**

The TC GUI project enforces **industry-leading quality standards** through a multi-layered approach:

- **High Quality Standards**: Minimal compiler warnings, practical clippy compliance, clean code
- **Automated Quality Gates**: Pre-commit hooks, CI/CD pipelines, and comprehensive testing
- **Security-First Mindset**: Regular vulnerability scanning, dependency analysis, and memory safety verification
- **Performance Monitoring**: Code coverage analysis, benchmark tracking, and regression detection

### **Development Workflows**

The project provides multiple quality workflows for different development scenarios:

#### **Daily Development**
```bash
# Fast iteration cycle - quick feedback
just dev

# Full quality verification before commits
just pre-commit

# Comprehensive quality pipeline
just quality
```

#### **Component-Specific Development**
```bash
# Backend-focused workflow
just backend

# Frontend-focused workflow  
just frontend

# Shared library workflow
just shared
```

#### **Integration and Release**
```bash
# Pre-push verification (before git push)
just pre-push

# Release preparation checklist
just prepare-release

# Emergency hotfix workflow
just hotfix
```

### **Quality Tools Integration**

#### **Core Quality Tools**
- **cargo fmt**: Consistent code formatting (enforced)
- **cargo clippy**: Advanced linting with warnings as errors
- **cargo check**: Compilation verification with zero warnings
- **cargo test**: Comprehensive test suite (184 tests)
- **cargo doc**: Documentation generation and verification

#### **Advanced Analysis Tools**
- **cargo-tarpaulin**: Code coverage analysis with HTML reports
- **cargo-audit**: Security vulnerability scanning
- **cargo-deny**: Dependency license and security verification
- **cargo-udeps**: Unused dependency detection
- **cargo-machete**: Advanced dead code analysis
- **cargo-nextest**: Next-generation parallel test execution
- **miri**: Memory safety verification for unsafe code

#### **Setup and Installation**
```bash
# Install all quality tools in one command
just setup-tools
```

### **Automated Quality Gates**

#### **Pre-Commit Hooks**
Automatically run before every git commit:
- Rust formatting verification
- Clippy linting (strict mode)
- Compilation checks
- Security vulnerability scanning
- Dependency analysis
- Spelling and syntax validation

**Installation:**
```bash
pip install pre-commit
pre-commit install
```

#### **Continuous Integration Pipeline**
GitHub Actions workflow with comprehensive quality verification:
- **Fast Quality Gate**: Quick feedback on basic issues
- **Comprehensive Analysis**: Full quality pipeline with all tools
- **Security Analysis**: Vulnerability and dependency scanning
- **Cross-Platform Testing**: Multiple Rust versions and platforms
- **Code Coverage**: Automated coverage reporting with Codecov
- **Documentation**: API docs generation and testing
- **Release Readiness**: Comprehensive release preparation checks

### **Quality Standards Enforcement**

The quality system enforces these **mandatory standards** at each development iteration:

- ✅ **Zero compiler warnings** - `cargo check --workspace`
- ✅ **Zero clippy warnings** - `cargo clippy --workspace -- -D warnings`  
- ✅ **Consistent formatting** - `cargo fmt --all`
- ✅ **Full test coverage** - `cargo test --workspace`
- ✅ **Security verification** - `cargo audit` + `cargo deny check all`
- ✅ **No unused dependencies** - `cargo udeps --workspace`
- ✅ **No dead code** - `cargo machete`
- ✅ **Memory safety** - `cargo +nightly miri test` (key modules)

This comprehensive system ensures **production-ready code quality** at every commit.
