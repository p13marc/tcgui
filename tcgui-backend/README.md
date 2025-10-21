# TC GUI Backend

The privileged backend service for TC GUI that handles all network operations, interface monitoring, and traffic control command execution.

## 🔧 Overview

The backend is a Rust service that runs with root privileges to perform network operations that require elevated permissions. It communicates with the frontend via Zenoh pub-sub messaging, providing a secure separation of concerns.

## 🏗️ Architecture

```
┌─────────────────────────────────────┐
│           TC Backend                │
├─────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────────┐ │
│  │   Network   │  │   TC Commands   │ │
│  │  Monitoring │  │   Execution     │ │
│  └─────────────┘  └─────────────────┘ │
│  ┌─────────────┐  ┌─────────────────┐ │
│  │  Bandwidth  │  │ Zenoh Messaging │ │
│  │  Monitoring │  │  Communication  │ │
│  └─────────────┘  └─────────────────┘ │
└─────────────────────────────────────┘
```

## 🚀 Features

- **🌐 Network Interface Discovery**: Automatic detection of all network interfaces
- **📊 Network Namespace Support**: Monitors interfaces across multiple namespaces
- **⚡ Real-time Monitoring**: Live interface state and bandwidth tracking
- **🎯 Smart TC Management**: Intelligent traffic control with complete parameter removal
- **🧹 Auto-Cleanup**: Automatically removes TC qdisc when no meaningful parameters remain
- **🔄 Parameter Intelligence**: Uses delete+add strategy for clean parameter removal vs replace
- **📡 Pub-Sub Communication**: High-performance messaging with frontend
- **🔄 Auto-Discovery**: Dynamic detection of interface changes

## 📁 Module Structure

### Core Modules

- **`main.rs`**: Application entry point and main event loop
- **`network.rs`**: Network interface discovery and monitoring via rtnetlink
- **`tc_commands.rs`**: Traffic control command execution and management
- **`bandwidth.rs`**: Bandwidth statistics collection and reporting

## 🔐 Security Model

### Required Permissions

The backend requires **root privileges** for:
- Network interface enumeration via netlink
- Traffic control command execution (`tc`)
- Network namespace operations
- Raw network statistics access

### Privilege Usage

```bash
# Running with minimal required privileges
sudo ./tcgui-backend

# Network operations performed:
sudo tc qdisc replace dev <interface> root netem loss 5%
sudo tc qdisc del dev <interface> root
ip netns exec <namespace> tc qdisc show dev <interface>
```

## 🚀 Usage

### Command Line Options

```bash
tcgui-backend [OPTIONS]

OPTIONS:
    -v, --verbose                    Enable verbose logging
        --exclude-loopback           Exclude loopback interface (lo) from monitoring
    -b, --backend-name <NAME>        Set custom backend name (default: hostname)
    -h, --help                       Print help information
```

### Starting the Service

```bash
# Development mode
sudo cargo run -p tcgui-backend

# With verbose logging
sudo RUST_LOG=debug cargo run -p tcgui-backend -- --verbose

# Exclude loopback interface from monitoring
sudo cargo run -p tcgui-backend -- --exclude-loopback

# Production mode (after building)
sudo ./target/release/tcgui-backend --verbose --exclude-loopback
```

### Environment Variables

- **`RUST_LOG`**: Control logging level (`debug`, `info`, `warn`, `error`)
- **`RUST_BACKTRACE`**: Enable backtrace on panics (`1` or `full`)

## 📡 Communication Architecture

### New Pub/Sub + Query/Reply Design

The backend uses a modern communication architecture with separate patterns:

**Published Topics** (Backend → Frontend):
- `tcgui/{backend}/interfaces/list` - Interface discovery updates
- `tcgui/{backend}/bandwidth/{namespace}/{interface}` - Real-time bandwidth statistics
- `tcgui/{backend}/interfaces/events` - Interface state changes  
- `tcgui/{backend}/health` - Backend health status

**Query Services** (Frontend → Backend):
- `tcgui/{backend}/query/tc` - Traffic control operations (query/reply)
- `tcgui/{backend}/query/interface` - Interface control operations (query/reply)

### Message Types

#### Query Messages (Frontend → Backend)

```rust
// TC Operations Query
struct TcRequest {
    namespace: String,
    interface: String,
    operation: TcOperation, // Apply{loss, correlation} or Remove
}

struct TcResponse {
    success: bool,
    message: String,
    applied_config: Option<TcConfiguration>,
    error_code: Option<i32>,
}

// Interface Control Query  
struct InterfaceControlRequest {
    namespace: String,
    interface: String,
    operation: InterfaceControlOperation, // Enable or Disable
}

struct InterfaceControlResponse {
    success: bool,
    message: String,
    new_state: bool, // true = up, false = down
    error_code: Option<i32>,
}
```

#### Published Messages (Backend → Frontend)

```rust
// Interface Discovery
struct InterfaceListUpdate {
    namespaces: Vec<NetworkNamespace>,
    timestamp: u64,
    backend_name: String,
}

// Bandwidth Statistics
struct BandwidthUpdate {
    namespace: String,
    interface: String,
    stats: NetworkBandwidthStats,
    backend_name: String,
}

// Interface State Changes
struct InterfaceStateEvent {
    namespace: String,
    interface: NetworkInterface,
    event_type: InterfaceEventType,
    timestamp: u64,
    backend_name: String,
}

// Backend Health
struct BackendHealthStatus {
    backend_name: String,
    status: String,
    timestamp: u64,
    metadata: BackendMetadata,
    namespace_count: usize,
    interface_count: usize,
}
```

## 🔍 Network Operations

### Interface Discovery

```rust
// Discovers all network interfaces across namespaces
let interfaces = network_manager.discover_interfaces().await?;

// Namespace-specific discovery
let ns_interfaces = network_manager.discover_interfaces_in_namespace("test-ns").await?;
```

### Interface Monitoring

The backend continuously monitors:
- Interface state changes (UP/DOWN)
- New interface creation
- Interface removal
- Network namespace changes

### TC Command Execution

```rust
// Apply comprehensive traffic control configuration
let result = tc_manager.apply_tc_config_in_namespace(
    "default", "eth0", 
    5.0,           // loss percentage
    Some(10.0),    // loss correlation
    Some(100.0),   // delay_ms
    Some(10.0),    // delay_jitter_ms
    Some(25.0),    // delay_correlation
    Some(2.0),     // duplicate_percent
    Some(15.0),    // duplicate_correlation
    Some(5.0),     // reorder_percent
    Some(20.0),    // reorder_correlation
    Some(3),       // reorder_gap
    Some(1.0),     // corrupt_percent
    Some(10.0),    // corrupt_correlation
    Some(1000)     // rate_limit_kbps
).await;

// Intelligent parameter removal - automatically detects when to use replace vs delete+add
let result = tc_manager.apply_tc_config_in_namespace(
    "default", "eth0", 
    0.0,     // Remove loss
    None,    // Remove loss correlation
    Some(50.0), // Keep delay but change value
    None,    // Remove delay jitter
    None,    // Remove delay correlation
    None,    // Remove duplicate (will trigger delete+add for clean removal)
    None,    // Remove duplicate correlation
    None,    // Remove reorder (will trigger delete+add for clean removal)
    None,    // Remove reorder correlation
    None,    // Remove reorder gap
    None,    // Remove corrupt
    None,    // Remove corrupt correlation
    None     // Remove rate limit
).await;

// Complete removal when no parameters remain
let result = tc_manager.remove_tc_config_in_namespace("default", "eth0").await;
```

### Intelligent Parameter Management

The backend includes smart logic for proper TC parameter management:

**Problem**: Linux `tc netem replace` preserves old parameters not explicitly specified
```bash
# This doesn't remove reorder - it preserves it!
tc qdisc replace dev lo root netem delay 10ms  # reorder still active!
```

**Solution**: Backend detects when parameters need removal and uses `delete + add`:
```bash
# Backend automatically does this when removing parameters:
tc qdisc del dev lo root
tc qdisc add dev lo root netem delay 10ms  # clean slate, no old reorder
```

**Automatic Detection**:
- **Replace strategy**: Used when only adding/modifying parameters
- **Delete+Add strategy**: Used when any existing parameters need removal
- **Complete removal**: Removes entire qdisc when no meaningful parameters remain
- **Meaningful check**: Loss > 0, delay > 0, duplicate > 0, reorder > 0, corrupt > 0, rate > 0

## 📊 Bandwidth Monitoring

### Namespace-Aware Statistics Collection

The backend provides comprehensive bandwidth monitoring with full namespace support:

```rust
pub struct NetworkBandwidthStats {
    pub rx_bytes: u64,           // Total bytes received
    pub rx_packets: u64,         // Total packets received
    pub rx_errors: u64,          // Receive errors (checksum, frame, etc.)
    pub rx_dropped: u64,         // Received packets dropped
    pub tx_bytes: u64,           // Total bytes transmitted
    pub tx_packets: u64,         // Total packets transmitted
    pub tx_errors: u64,          // Transmit errors (collision, carrier, etc.)
    pub tx_dropped: u64,         // Transmitted packets dropped
    pub timestamp: u64,          // Unix timestamp when collected
    pub rx_bytes_per_sec: f64,   // Current RX rate (calculated from deltas)
    pub tx_bytes_per_sec: f64,   // Current TX rate (calculated from deltas)
}
```

### Data Sources and Processing

- **`/proc/net/dev`**: Raw network interface statistics from kernel
- **Namespace isolation**: Separate `/proc/net/dev` access per namespace via `ip netns exec`
- **Rate calculations**: Delta-based bytes-per-second calculation with timestamp tracking
- **Counter wraparound handling**: Saturating arithmetic prevents negative rates
- **Namespace+interface keying**: Statistics stored as "namespace/interface" keys
- **Permission graceful handling**: Returns empty stats on namespace access denied
- **2-second monitoring intervals**: Regular updates without system overload

### Namespace-Specific Access

```bash
# Default namespace (direct access)
cat /proc/net/dev

# Named namespace (via ip netns exec)
ip netns exec test-ns cat /proc/net/dev
```

## 🌐 Network Namespace Support

### Namespace Discovery

```bash
# Automatically discovers existing namespaces
ip netns list

# Example output handled:
# test-ns
# container-ns
# vrf-red
```

### Interface Enumeration

```bash
# Default namespace
ip link show

# Named namespaces  
ip netns exec test-ns ip -j link show
```

### TC Operations in Namespaces

```bash
# Default namespace
sudo tc qdisc replace dev eth0 root netem loss 5%

# Named namespace
sudo ip netns exec test-ns tc qdisc replace dev veth0 root netem loss 5%
```

## 🐛 Error Handling

### Error Types

```rust
pub enum BackendError {
    InitializationError { message: String },
    NetworkError { message: String },
    RtnetlinkError(rtnetlink::Error),
    NetworkStatsError(std::io::Error),
    Common(TcguiError),
}
```

### Common Error Scenarios

1. **Permission Denied**: Backend not running as root
2. **Interface Not Found**: Target interface doesn't exist
3. **Namespace Not Found**: Target namespace doesn't exist
4. **TC Command Failed**: Invalid tc parameters or system error
5. **Zenoh Connection Failed**: Communication layer issues

### Error Recovery

- **Automatic retries** for transient network errors
- **Graceful degradation** when optional features fail
- **Detailed error reporting** to frontend
- **Connection recovery** for Zenoh communication issues

## 🔧 Development

### Building

```bash
# Development build
cargo build -p tcgui-backend

# Release build with optimizations
cargo build -p tcgui-backend --release

# Check for compilation errors
cargo check -p tcgui-backend
```

### Testing

```bash
# Run all tests
cargo test -p tcgui-backend

# Run with output
cargo test -p tcgui-backend -- --nocapture

# Test specific modules
cargo test -p tcgui-backend network::tests
```

### Linting

```bash
# Run clippy
cargo clippy -p tcgui-backend

# Fix automatically fixable issues
cargo fix -p tcgui-backend

# Format code
cargo fmt -p tcgui-backend
```

## 📋 Dependencies

### Core Dependencies

- **`rtnetlink`**: Linux netlink interface for network operations
- **`zenoh`**: High-performance pub-sub messaging
- **`tokio`**: Async runtime for concurrent operations
- **`tracing`**: Structured logging framework
- **`serde_json`**: JSON serialization for messages

### Network Dependencies

- **`futures-util`**: Stream processing utilities
- **`netlink-packet-route`**: Low-level netlink packet handling

## 🚨 System Requirements

### Operating System

- **Linux kernel 3.2+**: For netlink support
- **Network namespaces**: Optional, kernel 2.6.24+
- **Traffic control**: `tc` utility from iproute2 package

### Permissions

- **Root privileges**: Required for network operations
- **CAP_NET_ADMIN**: Minimum capability for network management
- **Sudo access**: For tc command execution

### Network Tools

```bash
# Required system tools
which tc          # Traffic control utility
which ip          # Network configuration utility
which sudo        # Privilege escalation

# Optional for namespace support
ip netns list     # Network namespace support
```

## 🔍 Monitoring and Debugging

### Log Levels

```bash
# Error level only
sudo RUST_LOG=error ./tcgui-backend

# Info level (recommended)
sudo RUST_LOG=info ./tcgui-backend

# Debug level (verbose)
sudo RUST_LOG=debug ./tcgui-backend

# Trace level (very verbose)
sudo RUST_LOG=trace ./tcgui-backend
```

### Debug Information

The backend logs detailed information about:
- Network interface discovery results
- TC command execution and results
- Zenoh communication events
- Error conditions and recovery attempts
- Performance metrics and timing

### Health Monitoring

The backend provides health information via:
- **Liveliness tokens**: Zenoh-based availability indication
- **Status messages**: Regular backend status updates
- **Error reporting**: Detailed error information to frontend
- **Performance metrics**: Operation timing and success rates

## 📈 Performance Characteristics

### Resource Usage

- **Memory**: ~10-50MB depending on interface count
- **CPU**: Low usage, event-driven architecture
- **Network**: Minimal overhead, efficient pub-sub messaging
- **Disk I/O**: Occasional reads from `/proc/net/dev`

### Scalability

- **Interfaces**: Tested with 100+ network interfaces
- **Namespaces**: Supports dozens of network namespaces
- **Update Rate**: Sub-second interface change detection
- **Throughput**: Handles high-frequency bandwidth updates

The backend is designed for efficiency and can handle complex network environments with many interfaces and namespaces while maintaining low resource usage.