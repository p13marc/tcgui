# TC GUI Communication Architecture Refactor

## Current Architecture Issues

The current system uses a single large enum (`FrontendMessage`, `BackendMessage`) for all communication, which has several limitations:
- All messages share the same QoS settings
- No request/reply semantics for operations that need confirmation
- Complex routing logic for different message types
- Difficult to optimize different types of data flows

## New Architecture Design

### Message Types and Communication Patterns

1. **Interface Discovery (Pub/Sub)**
   - Topic: `tcgui/{backend_name}/interfaces/list`
   - Message: `InterfaceListUpdate`
   - QoS: Reliable delivery, history depth=1 (last known good state)
   - Publisher: Backend
   - Subscriber: Frontend

2. **Bandwidth Updates (Pub/Sub)**  
   - Topic: `tcgui/{backend_name}/bandwidth/{namespace}/{interface}`
   - Message: `BandwidthStats`
   - QoS: Best effort, high frequency, no history
   - Publisher: Backend
   - Subscriber: Frontend

3. **Interface State Updates (Pub/Sub)**
   - Topic: `tcgui/{backend_name}/interfaces/events`
   - Message: `InterfaceStateEvent`
   - QoS: Reliable delivery, history depth=10
   - Publisher: Backend
   - Subscriber: Frontend

4. **Traffic Control Operations (RPC)**
   - Service: `tcgui/{backend_name}/rpc/tc`
   - Request: `TcRequest` (Apply/Remove TC)
   - Response: `TcResponse` (Success/Failure with details)
   - QoS: Reliable with timeout
   - Client: Frontend
   - Server: Backend

5. **Interface Control Operations (RPC)**
   - Service: `tcgui/{backend_name}/rpc/interface`
   - Request: `InterfaceControlRequest` (Enable/Disable)
   - Response: `InterfaceControlResponse`
   - QoS: Reliable with timeout
   - Client: Frontend
   - Server: Backend

6. **Backend Health/Liveliness (Pub/Sub)**
   - Topic: `tcgui/{backend_name}/health`
   - Message: `BackendHealthStatus`
   - QoS: Reliable, history depth=1
   - Publisher: Backend
   - Subscriber: Frontend

### Topics Structure

```
tcgui/
└── {backend_name}/
    ├── interfaces/
    │   ├── list          (InterfaceListUpdate - pub/sub)
    │   └── events        (InterfaceStateEvent - pub/sub)
    ├── bandwidth/
    │   └── {namespace}/
    │       └── {interface}   (BandwidthStats - pub/sub)
    ├── health            (BackendHealthStatus - pub/sub)
    └── rpc/
        ├── tc            (TcRequest/TcResponse - RPC)
        └── interface     (InterfaceControlRequest/InterfaceControlResponse - RPC)
```

### Message Types

#### Pub/Sub Messages

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceListUpdate {
    pub namespaces: Vec<NetworkNamespace>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStats {
    pub namespace: String,
    pub interface: String,
    pub stats: NetworkBandwidthStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceStateEvent {
    pub namespace: String,
    pub interface: NetworkInterface,
    pub event_type: InterfaceEventType,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealthStatus {
    pub backend_name: String,
    pub status: String,
    pub timestamp: u64,
    pub metadata: BackendMetadata,
}
```

#### RPC Messages

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcRequest {
    pub namespace: String,
    pub interface: String,
    pub operation: TcOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TcOperation {
    Apply { loss: f32, correlation: Option<f32> },
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcResponse {
    pub success: bool,
    pub message: String,
    pub applied_config: Option<TcConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceControlRequest {
    pub namespace: String,
    pub interface: String,
    pub operation: InterfaceControlOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InterfaceControlOperation {
    Enable,
    Disable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceControlResponse {
    pub success: bool,
    pub message: String,
    pub new_state: bool, // true = up, false = down
}
```

### QoS Configuration

- **Interface List**: Reliable delivery, Keep Last 1
- **Bandwidth Updates**: Best Effort, no history (real-time stream)
- **Interface Events**: Reliable delivery, Keep Last 10 
- **Health Status**: Reliable delivery, Keep Last 1
- **RPC Operations**: Reliable with 5-second timeout

### Benefits

1. **Granular QoS**: Each message type can have optimal delivery guarantees
2. **Request/Reply Semantics**: TC and interface operations get proper confirmation
3. **Better Performance**: High-frequency bandwidth updates don't affect control messages
4. **Easier Routing**: Topic-based routing is simpler than enum-based routing
5. **Scalability**: Each message type can be optimized independently
6. **Debugging**: Easier to trace specific types of messages

### Migration Strategy

1. Create new message types and communication layer
2. Implement backend with both old and new communication
3. Update frontend to use new communication
4. Remove old communication layer