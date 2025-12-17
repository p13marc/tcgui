# TC GUI Improvement Plan v2

This document outlines the next phase of improvements for the TC GUI codebase, following completion of the initial improvement plan.

## Overview

| Phase | Task | Priority | Effort | Status |
|-------|------|----------|--------|--------|
| 1 | Remove dead_code annotations | High | 2-3 hours | **Done** |
| 1 | Replace unwrap() calls in zenoh_manager | High | 1-2 hours | **Done** |
| 1 | Create scenario-format.md documentation | Medium | 1 hour | **Done** |
| 2 | Add frontend unit tests | Medium | 1-2 days | **Done** |
| 2 | Complete preset system implementation | Medium | 4-6 hours | Deferred* |
| 2 | Refactor service layer architecture | Medium | 1 day | N/A** |
| 3 | Optimize HashMap cloning in event loop | Low | 2-3 hours | N/A*** |
| 4 | Expand qdisc support beyond netem | Low | 1-2 weeks | See qdisc-expansion-plan.md |

*Preset system types exist; UI integration deferred to future work.
**Service layer already well-structured with TcService, NetworkService, BandwidthService.
***Pattern not found in current codebase; may have been refactored already.

---

## Phase 1: Immediate Fixes (Code Quality)

### 1.1 Remove dead_code Annotations

**Problem:**
The codebase has 23 `#[allow(dead_code)]` annotations. While some are legitimate (Debug trait usage, future API), many indicate unused code that should be removed or code that should be activated.

**Locations to audit:**
- `tcgui-backend/src/tc_config.rs` - Helper functions
- `tcgui-backend/src/netlink_events.rs` - LinkInfo struct fields
- `tcgui-backend/src/scenario/` - Scenario-related code
- `tcgui-frontend/src/` - UI components and state

**Implementation:**
1. Run analysis: `grep -rn "allow(dead_code)" tcgui-*/src/`
2. For each annotation, determine:
   - Is the code used via Debug trait? → Keep annotation with comment
   - Is the code a public API for future use? → Keep or export
   - Is the code truly unused? → Remove the code entirely
3. Remove annotations that are no longer needed

**Verification:**
```bash
just clippy  # Should pass without dead_code warnings
just test    # Ensure nothing breaks
```

---

### 1.2 Replace unwrap() Calls in zenoh_manager

**Problem:**
`tcgui-frontend/src/zenoh_manager.rs` contains multiple `unwrap()` calls that can panic in production, particularly around Zenoh session handling and message parsing.

**Risk:**
- Frontend crashes on network issues
- Poor user experience
- Silent failures

**Implementation:**

Replace panicking unwraps with proper error handling:

```rust
// Before:
let response = reply.result().unwrap();
let value = response.payload().deserialize::<TcResponse>().unwrap();

// After:
let response = reply.result()
    .map_err(|e| TcguiError::ZenohError(format!("Query failed: {:?}", e)))?;
let value = response.payload()
    .deserialize::<TcResponse>()
    .map_err(|e| TcguiError::DeserializationError(e.to_string()))?;
```

**Key areas to fix:**
- `query_tc()` method
- `query_interface()` method
- `query_scenario_*()` methods
- Subscription handlers

**Verification:**
```bash
# Search for remaining unwraps
grep -n "\.unwrap()" tcgui-frontend/src/zenoh_manager.rs
# Should be zero or only documented safe unwraps

just test-frontend
```

---

### 1.3 Create scenario-format.md Documentation

**Problem:**
`CLAUDE.md` references `docs/scenario-format.md` but the file doesn't exist. This leaves users without documentation on how to create scenarios.

**Location:**
Create `docs/scenario-format.md`

**Content to include:**
- JSON5 format specification
- Field descriptions (name, description, interface, steps)
- Step configuration (duration, tc_config fields)
- Example scenarios
- Loading locations (`./scenarios`, `~/.config/tcgui/scenarios`, `/usr/share/tcgui/scenarios`)
- Execution behavior (loop mode, pause/resume, cleanup)

**Template:**
```markdown
# Scenario Format Specification

Scenarios define sequences of TC configurations applied over time.

## File Format

Scenarios use JSON5 format for human-readable configuration with comments.

## Schema

```json5
{
  // Unique identifier (derived from filename if not specified)
  "id": "network-degradation",
  
  // Human-readable name
  "name": "Network Degradation Test",
  
  // Optional description
  "description": "Simulates progressive network degradation",
  
  // Steps to execute in order
  "steps": [
    {
      "duration": "10s",  // Human-readable duration
      "loss": 1.0,        // Packet loss percentage
      "delay_ms": 50      // Delay in milliseconds
    },
    // ... more steps
  ]
}
```

## Example

```json5
{
  "name": "High Latency Test",
  "steps": [
    { "duration": "30s", "delay_ms": 100, "delay_jitter_ms": 20 },
    { "duration": "30s", "delay_ms": 200, "delay_jitter_ms": 50 },
    { "duration": "30s" }  // Cleanup step (no TC config)
  ]
}
```
```

**Verification:**
- File exists and is valid markdown
- Links from CLAUDE.md work

---

## Phase 2: Feature Completion

### 2.1 Add Frontend Unit Tests

**Problem:**
Frontend crate has no unit tests. All 99 tests are in the backend.

**Impact:**
- UI logic is untested
- Refactoring is risky
- No regression protection

**Implementation:**

Add tests for key frontend modules:

1. **Message handling tests** (`tcgui-frontend/src/messages.rs`)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_conversion() {
        // Test Message to InterfaceMessage conversion
    }
}
```

2. **State management tests** (`tcgui-frontend/src/app.rs`)
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_namespace_grouping() {
        // Test interfaces are grouped correctly
    }
    
    #[test]
    fn test_interface_selection() {
        // Test interface selection state
    }
}
```

3. **Scenario manager tests** (`tcgui-frontend/src/scenario_manager.rs`)
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_scenario_loading() {
        // Test scenario list handling
    }
    
    #[test]
    fn test_execution_tracking() {
        // Test execution state updates
    }
}
```

**Target coverage:** 50%+ for frontend crate

**Verification:**
```bash
just test-frontend
just coverage  # Check frontend coverage
```

---

### 2.2 Complete Preset System Implementation

**Problem:**
Preset-related code exists but is incomplete. The preset system would allow users to save and apply common TC configurations.

**Current state:**
- Some preset types may be defined in shared crate
- No UI for preset management
- No persistence layer

**Implementation:**

1. **Define preset types** (if not complete):
```rust
// tcgui-shared/src/preset.rs
pub struct TcPreset {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub config: TcNetemConfig,
}
```

2. **Add preset storage** (backend):
- Load from `~/.config/tcgui/presets/`
- JSON5 or TOML format
- Query/reply handlers for CRUD operations

3. **Add preset UI** (frontend):
- Preset selector dropdown per interface
- "Save as Preset" button
- Preset management view

**Verification:**
```bash
just test
# Manual: Create, apply, and delete presets
```

---

### 2.3 Refactor Service Layer Architecture

**Problem:**
`tcgui-backend/src/services/` directory exists but is underutilized. Business logic is mixed into main.rs handlers.

**Goal:**
Clean separation between:
- Query handlers (receive/respond)
- Service layer (business logic)
- Data access (TC commands, network discovery)

**Implementation:**

1. **Create service traits:**
```rust
// tcgui-backend/src/services/tc_service.rs
pub trait TcService {
    async fn apply_config(&self, interface: &str, config: &TcNetemConfig) -> Result<()>;
    async fn clear_config(&self, interface: &str) -> Result<()>;
    async fn get_current_config(&self, interface: &str) -> Result<Option<TcNetemConfig>>;
}
```

2. **Implement services:**
```rust
pub struct TcServiceImpl {
    tc_manager: TcManager,
}

impl TcService for TcServiceImpl {
    // Delegate to tc_manager with proper error handling
}
```

3. **Inject into handlers:**
```rust
impl TcBackend {
    async fn handle_tc_query(&self, query: Query) -> Result<()> {
        let request: TcRequest = parse_query(&query)?;
        let response = self.tc_service.process(request).await?;
        reply(query, response).await
    }
}
```

**Benefits:**
- Testable business logic (mock services)
- Cleaner handler code
- Easier to add new features

---

## Phase 3: Performance Optimization

### 3.1 Optimize HashMap Cloning in Event Loop

**Problem:**
In `tcgui-backend/src/main.rs`, HashMaps are cloned in the event loop for iteration, causing unnecessary allocations.

**Pattern to fix:**
```rust
// Before:
for (key, publisher) in self.publishers.clone() {
    // ...
}

// After:
let keys: Vec<String> = self.publishers.keys().cloned().collect();
for key in keys {
    if let Some(publisher) = self.publishers.get(&key) {
        // ...
    }
}
```

**Verification:**
```bash
just test
# Profile with: cargo flamegraph
```

---

## Phase 4: Long-term Improvements

### 4.1 Expand Qdisc Support Beyond Netem

**Problem:**
Currently only supports netem qdisc. Users may want HTB, TBF, or other qdiscs.

**Implementation:**
1. Define qdisc trait
2. Implement netem (existing)
3. Add HTB support (hierarchical token bucket)
4. Add TBF support (token bucket filter)

```rust
pub trait Qdisc {
    fn name(&self) -> &str;
    fn build_command(&self, interface: &str) -> Vec<String>;
    fn parse_output(&self, output: &str) -> Result<Self>;
}

pub enum QdiscConfig {
    Netem(TcNetemConfig),
    Htb(HtbConfig),
    Tbf(TbfConfig),
}
```

**Verification:**
```bash
just test
# Manual testing with different qdisc types
```

---

## Appendix: Execution Order

### Recommended Order

1. **Phase 1.2** - Fix unwrap() calls (prevents crashes)
2. **Phase 1.1** - Remove dead_code (cleanup)
3. **Phase 1.3** - Create documentation (quick win)
4. **Phase 2.1** - Add frontend tests (enables safe refactoring)
5. **Phase 2.3** - Service layer refactor (cleaner code)
6. **Phase 2.2** - Complete presets (feature)
7. **Phase 3.1** - HashMap optimization
8. **Phase 4.1** - Qdisc expansion (see `docs/qdisc-expansion-plan.md`)

### Quality Gates

After each task:
```bash
just dev       # Format + check + clippy + tests
just pre-commit  # Final verification
```

After completing a phase:
```bash
just coverage  # Ensure coverage is maintained or improved
```
