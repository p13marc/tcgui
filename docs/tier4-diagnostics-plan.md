# Tier 4: Network Diagnostics Feature Implementation Plan

**Feature**: "Test Network" / "Diagnose" button per interface  
**Goal**: Verify TC rules are working by comparing configured vs actual network behavior  
**Estimated Effort**: 1-2 days

---

## Overview

Add a diagnostics feature that allows users to:
1. Click a "Diagnose" button on any interface
2. Run connectivity and performance tests
3. See results comparing configured TC settings vs measured behavior

---

## Implementation Steps

### Phase 1: Shared Types (tcgui-shared)

**File: `tcgui-shared/src/lib.rs`**

1. Add Zenoh topic for diagnostics queries:
   ```rust
   kedefine!(
       pub diagnostics_query_keys: "tcgui/${backend:*}/query/diagnostics",
   );
   
   pub fn diagnostics_query_service(backend_name: &str) -> OwnedKeyExpr
   ```

2. Add request/response types:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct DiagnosticsRequest {
       pub namespace: String,
       pub interface: String,
       pub target: Option<String>,  // Optional target IP/host
       pub timeout_ms: u32,
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct DiagnosticsResponse {
       pub success: bool,
       pub message: String,
       pub results: DiagnosticsResults,
       pub error_code: Option<i32>,
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct DiagnosticsResults {
       pub link_status: LinkStatus,
       pub connectivity: Option<ConnectivityResult>,
       pub latency: Option<LatencyResult>,
       pub configured_tc: Option<TcNetemConfig>,
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct LinkStatus {
       pub is_up: bool,
       pub has_carrier: bool,
       pub mtu: u32,
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ConnectivityResult {
       pub target: String,
       pub reachable: bool,
       pub method: String,  // "ping", "tcp", "arp"
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct LatencyResult {
       pub target: String,
       pub min_ms: f32,
       pub avg_ms: f32,
       pub max_ms: f32,
       pub packet_loss_percent: f32,
       pub samples: u32,
   }
   ```

---

### Phase 2: Backend Diagnostics Service (tcgui-backend)

**New file: `tcgui-backend/src/diagnostics.rs`**

1. Create `DiagnosticsService` struct:
   ```rust
   pub struct DiagnosticsService {
       network_manager: Arc<NetworkManager>,
       tc_manager: Arc<TcCommandManager>,
   }
   
   impl DiagnosticsService {
       pub async fn run_diagnostics(
           &self,
           namespace: &str,
           interface: &str,
           target: Option<&str>,
           timeout_ms: u32,
       ) -> Result<DiagnosticsResults, TcguiError>;
       
       async fn check_link_status(&self, ns: &str, iface: &str) -> Result<LinkStatus>;
       async fn check_connectivity(&self, ns: &str, iface: &str, target: &str) -> Result<ConnectivityResult>;
       async fn measure_latency(&self, ns: &str, iface: &str, target: &str, samples: u32) -> Result<LatencyResult>;
       async fn get_current_tc_config(&self, ns: &str, iface: &str) -> Result<Option<TcNetemConfig>>;
   }
   ```

2. Implementation approach:
   - **Link status**: Use nlink `get_links()` to check `is_up()` and `has_carrier()`
   - **Connectivity**: Use `ping` command via tokio::process (respects namespace)
   - **Latency measurement**: Parse `ping -c N` output for min/avg/max/loss
   - **TC config**: Use existing `TcCommandManager::get_netem_options()`

3. Namespace handling:
   - For non-root namespace: Use `ip netns exec {ns} ping ...`
   - For root namespace: Direct `ping` command

**File: `tcgui-backend/src/main.rs`**

4. Add diagnostics queryable in `TcBackend::run()`:
   ```rust
   let diagnostics_queryable = session
       .declare_queryable(&topics::diagnostics_query_service(&backend_name))
       .await?;
   ```

5. Add handler in tokio::select! loop:
   ```rust
   query = diagnostics_queryable.recv_async() => {
       if let Err(e) = self.handle_diagnostics_query(query).await {
           error!("Failed to handle diagnostics query: {}", e);
       }
   }
   ```

6. Implement `handle_diagnostics_query()`:
   - Deserialize `DiagnosticsRequest`
   - Call `diagnostics_service.run_diagnostics()`
   - Serialize and reply with `DiagnosticsResponse`

---

### Phase 3: Frontend Query Integration (tcgui-frontend)

**File: `tcgui-frontend/src/query_manager.rs`**

1. Add diagnostics query sender field:
   ```rust
   pub struct QueryManager {
       // ... existing fields ...
       diagnostics_query_sender: Option<UnboundedSender<DiagnosticsQueryMessage>>,
   }
   ```

2. Add diagnostics query method:
   ```rust
   pub fn run_diagnostics(
       &self,
       backend_name: String,
       namespace: String,
       interface: String,
       target: Option<String>,
   ) -> Result<oneshot::Receiver<DiagnosticsResponse>, String>
   ```

**File: `tcgui-frontend/src/zenoh_manager.rs`**

3. Add diagnostics query channel setup and handling (similar to TC query pattern)

**File: `tcgui-frontend/src/messages.rs`**

4. Add app-level message:
   ```rust
   pub enum TcGuiMessage {
       // ... existing ...
       SetupDiagnosticsQueryChannel(UnboundedSender<DiagnosticsQueryMessage>),
       DiagnosticsResult { namespace: String, interface: String, result: DiagnosticsResponse },
   }
   ```

---

### Phase 4: Frontend UI (tcgui-frontend)

**File: `tcgui-frontend/src/interface/messages.rs`**

1. Add message variant:
   ```rust
   pub enum TcInterfaceMessage {
       // ... existing ...
       StartDiagnostics,
       DiagnosticsInProgress,
       DiagnosticsComplete(DiagnosticsResponse),
   }
   ```

**File: `tcgui-frontend/src/interface/state.rs`**

2. Add diagnostics state:
   ```rust
   pub struct InterfaceState {
       // ... existing ...
       pub diagnostics_running: bool,
       pub diagnostics_result: Option<DiagnosticsResponse>,
   }
   ```

**File: `tcgui-frontend/src/interface/base.rs`**

3. Add "Diagnose" button to main row:
   - Place after the status indicator
   - Use icon (Zap, Activity, or similar)
   - Disable while diagnostics are running
   - Show spinner during execution

4. Add diagnostics result display:
   - Option A: Expandable panel below main row (like feature controls)
   - Option B: Modal/overlay with detailed results
   - Option C: Inline status summary with tooltip for details

   Recommended: **Option A** - Expandable panel showing:
   ```
   ┌─────────────────────────────────────────────────────────────┐
   │ Diagnostics Results                                    [X]  │
   ├─────────────────────────────────────────────────────────────┤
   │ Link Status:    UP, Carrier OK, MTU 1500                    │
   │ Connectivity:   Gateway 192.168.1.1 - Reachable (ping)      │
   │ Latency:        min: 0.5ms  avg: 1.2ms  max: 3.1ms          │
   │ Packet Loss:    0.0%                                        │
   │                                                             │
   │ TC Configuration Active:                                    │
   │   Delay: 50ms ± 10ms                                        │
   │   Loss: 1.0%                                                │
   │                                                             │
   │ Comparison:                                                 │
   │   Configured delay: 50ms, Measured: 51.2ms avg  [OK]        │
   │   Configured loss: 1.0%, Measured: 0.8%         [OK]        │
   └─────────────────────────────────────────────────────────────┘
   ```

5. Handle message in `update()`:
   ```rust
   TcInterfaceMessage::StartDiagnostics => {
       self.state.diagnostics_running = true;
       self.state.diagnostics_result = None;
       // Return command to trigger query (via parent)
   }
   ```

**File: `tcgui-frontend/src/message_handlers.rs`**

6. Add handler for diagnostics:
   ```rust
   pub fn handle_start_diagnostics(
       app: &mut TcGui,
       namespace: &str,
       interface: &str,
   ) -> Task<TcGuiMessage>
   ```

---

### Phase 5: Auto-detect Target

For a better UX, the backend should auto-detect a reasonable target:

1. **Default gateway**: Query route table for interface's gateway
2. **DNS server**: Parse `/etc/resolv.conf`
3. **Fallback**: Use 8.8.8.8 or configurable default

Implementation in `diagnostics.rs`:
```rust
async fn detect_target(&self, namespace: &str, interface: &str) -> Option<String> {
    // 1. Try to get default gateway for this interface
    if let Some(gateway) = self.get_default_gateway(namespace, interface).await {
        return Some(gateway);
    }
    
    // 2. Fallback to first DNS server
    if let Some(dns) = self.get_dns_server().await {
        return Some(dns);
    }
    
    // 3. Ultimate fallback
    Some("8.8.8.8".to_string())
}
```

---

## File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `tcgui-shared/src/lib.rs` | Modify | Add DiagnosticsRequest/Response types, topic |
| `tcgui-backend/src/diagnostics.rs` | Create | DiagnosticsService implementation |
| `tcgui-backend/src/main.rs` | Modify | Add queryable and handler |
| `tcgui-backend/src/lib.rs` | Modify | Export diagnostics module |
| `tcgui-frontend/src/query_manager.rs` | Modify | Add diagnostics query method |
| `tcgui-frontend/src/zenoh_manager.rs` | Modify | Add diagnostics channel handling |
| `tcgui-frontend/src/messages.rs` | Modify | Add DiagnosticsResult message |
| `tcgui-frontend/src/interface/messages.rs` | Modify | Add diagnostics messages |
| `tcgui-frontend/src/interface/state.rs` | Modify | Add diagnostics state fields |
| `tcgui-frontend/src/interface/base.rs` | Modify | Add button and results panel |
| `tcgui-frontend/src/message_handlers.rs` | Modify | Add diagnostics handler |
| `tcgui-frontend/src/app.rs` | Modify | Route diagnostics messages |

---

## Testing Plan

1. **Unit tests** (tcgui-shared):
   - DiagnosticsRequest/Response serialization
   - Default values handling

2. **Integration tests** (tcgui-backend):
   - DiagnosticsService with mock network
   - Namespace isolation
   - Timeout handling

3. **Manual testing**:
   - Run diagnostics on physical interface
   - Run diagnostics on veth pair
   - Run diagnostics in network namespace
   - Test with and without TC rules applied
   - Test connectivity failure scenarios

---

## Future Enhancements (Out of Scope)

- Continuous monitoring mode (periodic diagnostics)
- Historical comparison (track diagnostics over time)
- Export diagnostics report
- Custom target configuration in UI
- Advanced metrics from nlink 0.6.0 diagnostics module (when available)

---

## Dependencies

- nlink 0.5.0 (current) - sufficient for Phase 1-5
- nlink 0.6.0 diagnostics module - future enhancement opportunity
- `ping` command - available on all Linux systems
