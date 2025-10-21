# TC GUI Frontend

The graphical user interface for TC GUI that provides an intuitive way to manage network traffic control across multiple network namespaces.

## 🎨 Overview

The frontend is a native GUI application built with Rust and the Iced framework. It runs as an unprivileged user and communicates with the privileged backend via Zenoh messaging to provide a secure, responsive interface for network traffic control.

## 🏗️ Architecture

```
┌─────────────────────────────────────┐
│           TC Frontend               │
├─────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────────┐ │
│  │    Main     │  │   Interface     │ │
│  │ Application │  │  Components     │ │
│  └─────────────┘  └─────────────────┘ │
│  ┌─────────────┐  ┌─────────────────┐ │
│  │   Message   │  │ Zenoh Messaging │ │
│  │  Handling   │  │  Communication  │ │
│  └─────────────┘  └─────────────────┘ │
└─────────────────────────────────────┘
```

## 🚀 Features

- **🎛️ Intuitive GUI**: Modern interface built with Iced framework with direct feature control
- **⚡ One-Click TC Features**: Direct checkboxes for Loss, Delay, Duplicate, Reorder, Corrupt, Rate Limiting
- **✨ Instant Application**: Features apply immediately when checked - no "Apply" button needed
- **📊 Namespace Grouping**: Interfaces organized by network namespace
- **📈 Real-time Monitoring**: Live bandwidth statistics and status updates
- **🔄 Scrollable Interface**: Handle many interfaces with smooth scrolling
- **🎯 Complete TC Control**: Full parameter control with expandable sliders
- **📱 Responsive Design**: Adapts to different window sizes
- **🔐 Security**: Runs without privileges, read-only for namespaces
- **🌐 Multi-Backend Support**: Connects to multiple named backend instances

## 📁 Module Structure

### Core Modules

- **`main.rs`**: Application entry point and Iced application runner
- **`app.rs`**: Main application logic and state management
- **`interface.rs`**: Individual interface component and TC controls
- **`messages.rs`**: Message type definitions and event handling
- **`zenoh_manager.rs`**: Communication layer with backend

### Component Hierarchy

```
TcGui (Main App)
├── NamespaceGroup (Per Namespace)
│   └── TcInterface (Per Interface)
│       ├── Status Display
│       ├── Bandwidth Monitor
│       └── TC Controls
└── ZenohManager (Communication)
```

## 🎮 User Interface

### Main Window Layout

```
┌────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Contrôleur netem (tc) - Backend: Connected                                      │
├────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ┌─ Namespace: default ────────────────────────────────────────────────────────────────────────┐ │ ↕
│ │  Interface: [eth0____] ✅ Interface UP | TC: ──                           │ │ │
│ │  │                                                                    │ │ │
│ │  │ Traffic Control:     │ Bandwidth Stats:                        │ │ │ Scrollable
│ │  │ Loss: [====|────] 10%  │ RX: 1.2 MB/s (1.5M packets)            │ │ │ Content
│ │  │ Corr: [────|────] 0%   │ TX: 800 KB/s (900K packets)           │ │ │ Area
│ │  │ [✓] Interface Enabled  │ No errors or drops                     │ │ │
│ │  │ [ ] Enable TC         │                                        │ │ │
│ │  └────────────────────────────────────────────────────────────────────────┘ │ │
│ │  Status: OK: Configuration applied successfully                                │ │
│ └─────────────────────────────────────────────────────────────────────────────┘ │ │
│ ┌─ Namespace: test-ns ──────────────────────────────────────────────────────────────────────┐ │ │
│ │  eth1 [Physical] [UP] [5% loss] ─ RX: 2.5 GB/s TX: 1.8 GB/s               │ │ ↕
│ └─────────────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### Interface Controls

Each network interface provides comprehensive management capabilities:

#### Visual Indicators
- **Interface status**: Color-coded UP (✅) / DOWN (❌) indicators
- **TC activity**: Interface icon shows 🔧 when TC features active, 📡 when normal
- **Progress indicators**: ⚙️ symbols during state changes
- **Error highlighting**: Red text for errors, green for success

#### Direct Feature Control
- **Feature Checkboxes**: Direct toggles for LSS, DLY, DUP, RO, CR, RL features
- **Instant Application**: Checking a feature applies it immediately - no "Apply" button
- **Expandable Parameters**: Sliders appear when features are enabled for fine control
- **Auto-Defaults**: Features use sensible defaults (1% loss, 10ms delay, etc.)
- **Complete Removal**: Unchecking features properly removes them from TC qdisc

#### Bandwidth Statistics Display
- **Real-time rates**: Live RX/TX speeds with automatic unit scaling
- **Cumulative counters**: Total bytes and packets since interface creation
- **Error tracking**: Displays errors and drops when non-zero
- **Multi-unit support**: Automatic B/s, KB/s, MB/s, GB/s formatting

#### Status Management
- **Message history**: Recent status messages with OK/ERR prefixes
- **Bounded storage**: Maximum 100 messages to prevent memory growth
- **Operation feedback**: Success/failure reporting for all operations

## 🚀 Usage

### Command Line Options

```bash
tcgui-frontend [OPTIONS]

OPTIONS:
    -v, --verbose    Enable verbose logging
    -h, --help       Print help information
```

### Starting the Application

```bash
# Development mode
cargo run -p tcgui-frontend

# With verbose logging
RUST_LOG=debug cargo run -p tcgui-frontend -- --verbose

# Production mode (after building)
./target/release/tcgui-frontend --verbose
```

### Environment Variables

- **`RUST_LOG`**: Control logging level (`debug`, `info`, `warn`, `error`)
- **`RUST_BACKTRACE`**: Enable backtrace on panics (`1` or `full`)

## 📡 Communication Architecture

### Modern Pub/Sub + Query/Reply Design

```
Frontend ──────► Backend
   │                │
   │   Queries      │ 
   │                ▼
   │            Process
   │                │
   │   Replies      │
   ◄──────────────────┘
   │                │
   ◄── Pub/Sub Data ──┘
```

### Communication Patterns

**Subscribed Topics** (Backend → Frontend):
- `tcgui/{backend}/interfaces/list` - Interface discovery updates
- `tcgui/{backend}/bandwidth/{namespace}/{interface}` - Real-time bandwidth statistics
- `tcgui/{backend}/interfaces/events` - Interface state changes
- `tcgui/{backend}/health` - Backend health monitoring

**Query Services** (Frontend → Backend):
- `tcgui/{backend}/query/tc` - Traffic control operations
- `tcgui/{backend}/query/interface` - Interface enable/disable operations

### Message Types

#### Query Messages (Frontend → Backend)

```rust
// TC Operations
struct TcRequest {
    namespace: String,
    interface: String,
    operation: TcOperation, // Apply{loss, correlation} or Remove
}

// Interface Control
struct InterfaceControlRequest {
    namespace: String,
    interface: String,
    operation: InterfaceControlOperation, // Enable or Disable
}
```

#### Subscription Messages (Backend → Frontend)

```rust
// Interface Discovery
struct InterfaceListUpdate {
    namespaces: Vec<NetworkNamespace>,
    timestamp: u64,
    backend_name: String,
}

// Real-time Bandwidth
struct BandwidthUpdate {
    namespace: String,
    interface: String,
    stats: NetworkBandwidthStats,
    backend_name: String,
}

// Interface Events
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

## 🎨 UI Components

### TcInterface Component

The core interface component provides comprehensive traffic control management:

```rust
pub struct TcInterface {
    state: InterfaceState,            // Comprehensive interface state with TC features
    status: Vec<String>,              // Status message history (bounded to 100)
    applying: bool,                   // TC operation in progress
    bandwidth_stats: Option<NetworkBandwidthStats>, // Current bandwidth data
}

pub struct InterfaceState {
    name: String,                     // Interface name
    is_up: bool,                      // Current interface UP/DOWN state
    has_tc_qdisc: bool,               // TC qdisc currently configured
    interface_enabled: bool,          // User's desired interface state
    applying_interface_state: bool,   // Interface state change in progress
    features: TcFeatures,             // All TC feature configurations
}

pub struct TcFeatures {
    loss: TcFeature<LossConfig>,      // Packet loss with correlation
    delay: TcFeature<DelayConfig>,    // Delay with jitter and correlation
    duplicate: TcFeature<DuplicateConfig>, // Packet duplication
    reorder: TcFeature<ReorderConfig>,     // Packet reordering
    corrupt: TcFeature<CorruptConfig>,     // Packet corruption
    rate_limit: TcFeature<RateLimitConfig>, // Rate limiting
}
```

#### Key Features:
- **State separation**: Distinguishes between current state (backend) and desired state (user)
- **Real-time updates**: Bandwidth statistics with automatic unit formatting (B/s to GB/s)
- **Visual indicators**: Color-coded status for UP/DOWN, TC configuration
- **Bounded message history**: Prevents memory growth with status message cap
- **Auto-apply functionality**: Immediate TC updates when sliders change
- **Error handling**: Graceful display of operation successes and failures
- **Progress indicators**: Visual feedback during state changes
- **Visual feedback**: Color-coded status messages and interface states

### Message Handling

The application uses the Elm architecture pattern:

```rust
enum TcGuiMessage {
    BackendMessage(BackendMessage),
    BackendConnectionStatus(bool),
    BackendLiveness(bool),
    BandwidthUpdate { interface: String, stats: NetworkBandwidthStats },
    TcInterfaceMessage(String, TcInterfaceMessage),
    SetupMessageChannel(mpsc::UnboundedSender<FrontendMessage>),
    SendToBackend(FrontendMessage),
}
```

### State Management

The main application maintains:
- **Namespace organization**: Interfaces grouped by network namespace
- **Connection status**: Backend availability and health
- **Message channels**: Bi-directional communication with backend
- **UI state**: Input values, status messages, bandwidth data

## 📊 Data Display

### Bandwidth Formatting

The frontend automatically formats bandwidth values:

```rust
// Examples of automatic formatting
1024 bytes/sec      → "1.00 KB/s"
1048576 bytes/sec   → "1.00 MB/s" 
1073741824 bytes/sec → "1.00 GB/s"
```

### Status Messages

Status messages include:
- **Timestamp**: When the status was recorded
- **Color coding**: Green for success, red for errors, yellow for warnings
- **Message capping**: Limited to prevent memory issues
- **Auto-scrolling**: Most recent messages visible

### Interface Status Indicators

Visual indicators show:
- **Interface type**: Physical, Virtual, Veth, Bridge, TUN, TAP, Loopback
- **Link state**: UP (green) or DOWN (red)
- **TC status**: Shows if netem is configured
- **Namespace**: Clear grouping by network namespace

## 🔐 Security Model

### No Privileges Required

The frontend runs as a regular user with:
- **No root access**: Cannot perform privileged operations
- **Read-only namespaces**: Cannot create or modify namespaces
- **Network isolation**: Communicates only via Zenoh messaging
- **Input validation**: Sanitizes all user input before sending

### Safe Operations

All network operations are:
- **Validated by backend**: Frontend cannot bypass security
- **Authenticated communication**: Secure pub-sub messaging
- **Error handling**: Graceful degradation on permission issues
- **Audit trail**: All operations logged by backend

## 🎯 Supported TC Operations

### Traffic Control Features

The frontend supports configuring:

1. **Packet Loss**:
   - Range: 0.0% to 100.0%
   - Precision: One decimal place
   - Validation: Automatic range checking

2. **Loss Correlation**:
   - Range: 0.0% to 100.0% (optional)
   - Effect: Makes consecutive packet loss more likely
   - Usage: Leave empty for no correlation

### TC Command Generation

```bash
# Generated commands (executed by backend):
sudo tc qdisc replace dev eth0 root netem loss 5.0%
sudo tc qdisc replace dev eth0 root netem loss 5.0% correlation 10.0%
sudo tc qdisc del dev eth0 root
```

### Namespace-Aware Operations

Commands are automatically executed in the correct namespace:

```bash
# Default namespace
sudo tc qdisc replace dev eth0 root netem loss 5.0%

# Named namespace
sudo ip netns exec test-ns tc qdisc replace dev veth0 root netem loss 5.0%
```

## 🔧 Development

### Building

```bash
# Development build
cargo build -p tcgui-frontend

# Release build with optimizations
cargo build -p tcgui-frontend --release

# Check for compilation errors
cargo check -p tcgui-frontend
```

### Testing

```bash
# Run all tests
cargo test -p tcgui-frontend

# Run with output
cargo test -p tcgui-frontend -- --nocapture

# Test specific modules
cargo test -p tcgui-frontend app::tests
```

### Linting

```bash
# Run clippy
cargo clippy -p tcgui-frontend

# Fix automatically fixable issues
cargo fix -p tcgui-frontend

# Format code
cargo fmt -p tcgui-frontend
```

## 📋 Dependencies

### UI Framework

- **`iced`**: Native GUI framework with advanced features
  - `tokio` feature for async support
  - `debug` feature for development tools
  - Advanced widgets and layouts

### Communication

- **`zenoh`**: High-performance pub-sub messaging
- **`tokio`**: Async runtime for concurrent operations
- **`serde_json`**: JSON serialization for messages

### Utilities

- **`tracing`**: Structured logging framework
- **`clap`**: Command-line argument parsing

## 🎨 Theming and Styling

### Color Scheme

The application uses a consistent color scheme:

```rust
// Status indicators
Connected:    RGB(0.0, 0.8, 0.0)  // Green
Disconnected: RGB(0.8, 0.0, 0.0)  // Red  
Warning:      RGB(0.9, 0.7, 0.0)  // Yellow/Orange
Info:         RGB(0.6, 0.6, 0.6)  // Gray
Namespace:    RGB(0.2, 0.6, 1.0)  // Blue
```

### Typography

- **Title**: 24px for main application title
- **Headers**: 18px for namespace headers
- **Body text**: 14px for bandwidth summary
- **Interface text**: Default size for interface information

### Layout

- **Spacing**: Consistent 5px, 10px, 20px, 25px spacing
- **Padding**: 20px padding for namespace sections
- **Scrolling**: Smooth scrolling for content overflow
- **Responsive**: Adapts to window resizing

## 🐛 Error Handling

### User-Friendly Messages

The frontend displays clear error messages for:
- **Connection issues**: "Backend: Disconnected"
- **TC failures**: Detailed error descriptions from backend
- **Invalid input**: Input validation with helpful hints
- **Network problems**: Graceful degradation with status updates

### Recovery Mechanisms

- **Auto-reconnection**: Automatic retry on connection loss
- **State preservation**: Maintains UI state during reconnections
- **Graceful degradation**: Continues functioning with limited backend
- **User feedback**: Clear indication of system status

## 📈 Performance Characteristics

### Resource Usage

- **Memory**: ~20-100MB depending on interface count and history
- **CPU**: Low usage, event-driven architecture
- **GPU**: Minimal, efficient rendering with Iced
- **Network**: Lightweight pub-sub messaging only

### Responsiveness

- **UI updates**: 60 FPS rendering capability
- **Network updates**: Sub-second response to backend changes
- **Scroll performance**: Smooth scrolling with many interfaces
- **Input responsiveness**: Immediate feedback on user actions

### Scalability

- **Interfaces**: Handles hundreds of network interfaces
- **Namespaces**: Supports many network namespaces
- **Message throughput**: Efficient handling of high-frequency updates
- **Memory management**: Bounded status message history

The frontend is designed for excellent user experience while remaining lightweight and responsive even with complex network configurations.