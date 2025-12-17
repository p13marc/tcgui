# Qdisc Expansion Plan: Beyond Netem

This document provides a detailed implementation plan for expanding TC GUI to support additional Linux qdisc types beyond the current netem implementation.

## Overview

| Phase | Task | Effort | Dependencies |
|-------|------|--------|--------------|
| 1 | Design qdisc abstraction layer | 2-3 days | None |
| 2 | Refactor netem to use abstraction | 2-3 days | Phase 1 |
| 3 | Implement HTB (Hierarchical Token Bucket) | 3-4 days | Phase 2 |
| 4 | Implement TBF (Token Bucket Filter) | 2-3 days | Phase 2 |
| 5 | Add qdisc selection UI | 2-3 days | Phase 3, 4 |
| 6 | Implement qdisc chaining | 3-4 days | Phase 5 |

---

## Background: Linux Qdiscs

Linux Traffic Control uses queueing disciplines (qdiscs) to control how packets are queued and transmitted. The main categories are:

### Classless Qdiscs (Simple)
- **netem** - Network emulation (delay, loss, corruption) - *currently implemented*
- **tbf** - Token Bucket Filter (rate limiting with burst)
- **pfifo/bfifo** - Simple FIFO queues
- **sfq** - Stochastic Fairness Queueing

### Classful Qdiscs (Hierarchical)
- **htb** - Hierarchical Token Bucket (bandwidth allocation, prioritization)
- **cbq** - Class Based Queueing (older, complex)
- **prio** - Priority queueing

### Priority Targets
1. **TBF** - Simpler rate limiting than current netem rate (burst control)
2. **HTB** - Professional bandwidth management with classes
3. **netem + htb** - Chaining for realistic network simulation

---

## Phase 1: Design Qdisc Abstraction Layer

### 1.1 Define Core Traits

Create `tcgui-shared/src/qdisc/mod.rs`:

```rust
//! Qdisc abstraction layer for extensible traffic control.

use serde::{Deserialize, Serialize};

/// Identifies a qdisc type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QdiscType {
    Netem,
    Tbf,
    Htb,
    // Future: Sfq, Prio, etc.
}

impl QdiscType {
    /// Get the tc command name for this qdisc
    pub fn tc_name(&self) -> &'static str {
        match self {
            QdiscType::Netem => "netem",
            QdiscType::Tbf => "tbf",
            QdiscType::Htb => "htb",
        }
    }
    
    /// Whether this qdisc supports child classes
    pub fn is_classful(&self) -> bool {
        matches!(self, QdiscType::Htb)
    }
    
    /// User-friendly display name
    pub fn display_name(&self) -> &'static str {
        match self {
            QdiscType::Netem => "Network Emulator (netem)",
            QdiscType::Tbf => "Token Bucket Filter (tbf)",
            QdiscType::Htb => "Hierarchical Token Bucket (htb)",
        }
    }
}

/// Common trait for all qdisc configurations
pub trait QdiscConfig: Clone + Send + Sync + 'static {
    /// Get the qdisc type
    fn qdisc_type(&self) -> QdiscType;
    
    /// Validate the configuration
    fn validate(&self) -> Result<(), QdiscValidationError>;
    
    /// Build tc command arguments (excluding "tc qdisc add/replace dev X root")
    fn build_tc_args(&self) -> Vec<String>;
    
    /// Parse tc qdisc show output into this config type
    fn parse_tc_output(output: &str) -> Option<Self> where Self: Sized;
    
    /// Check if this config is effectively "empty" (no changes)
    fn is_empty(&self) -> bool;
}

/// Validation error for qdisc configurations
#[derive(Debug, Clone)]
pub struct QdiscValidationError {
    pub qdisc: QdiscType,
    pub field: String,
    pub message: String,
}
```

### 1.2 Define Configuration Types

Create `tcgui-shared/src/qdisc/netem.rs`:

```rust
//! Netem qdisc configuration (refactored from TcNetemConfig)

use super::{QdiscConfig, QdiscType, QdiscValidationError};
use serde::{Deserialize, Serialize};

/// Network emulator qdisc configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetemConfig {
    pub loss: LossConfig,
    pub delay: DelayConfig,
    pub duplicate: DuplicateConfig,
    pub reorder: ReorderConfig,
    pub corrupt: CorruptConfig,
    pub rate: RateConfig,  // Note: netem rate is simpler than tbf
}

// ... existing config structs (LossConfig, DelayConfig, etc.)

impl QdiscConfig for NetemConfig {
    fn qdisc_type(&self) -> QdiscType {
        QdiscType::Netem
    }
    
    fn validate(&self) -> Result<(), QdiscValidationError> {
        // Existing validation logic
    }
    
    fn build_tc_args(&self) -> Vec<String> {
        let mut args = vec!["netem".to_string()];
        
        if self.loss.enabled && self.loss.percentage > 0.0 {
            args.extend(["loss".to_string(), "random".to_string()]);
            args.push(format!("{}%", self.loss.percentage));
            if self.loss.correlation > 0.0 {
                args.push(format!("{}%", self.loss.correlation));
            }
        }
        
        // ... delay, duplicate, reorder, corrupt, rate
        
        args
    }
    
    fn parse_tc_output(output: &str) -> Option<Self> {
        // Parse "qdisc netem 8001: root refcnt 2 limit 1000 delay 100ms loss 5%"
        if !output.contains("netem") {
            return None;
        }
        // ... parsing logic
    }
    
    fn is_empty(&self) -> bool {
        !self.loss.enabled 
            && !self.delay.enabled 
            && !self.duplicate.enabled
            && !self.reorder.enabled
            && !self.corrupt.enabled
            && !self.rate.enabled
    }
}
```

Create `tcgui-shared/src/qdisc/tbf.rs`:

```rust
//! Token Bucket Filter qdisc configuration

use super::{QdiscConfig, QdiscType, QdiscValidationError};
use serde::{Deserialize, Serialize};

/// Token Bucket Filter configuration
/// 
/// TBF provides precise rate limiting with configurable burst size.
/// Unlike netem's rate parameter, TBF offers:
/// - Exact rate enforcement
/// - Configurable burst (bucket size)
/// - Latency control (max time a packet can wait)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TbfConfig {
    /// Whether TBF is enabled
    pub enabled: bool,
    /// Rate limit in bits per second (e.g., 1000000 = 1 Mbps)
    pub rate_bps: u64,
    /// Burst size in bytes (bucket size)
    /// Minimum: rate / HZ (typically rate / 250)
    /// Recommended: rate / 8 for 1 second worth of data
    pub burst_bytes: u32,
    /// Maximum latency in milliseconds
    /// How long a packet can wait in the queue
    pub latency_ms: Option<u32>,
    /// Alternative to latency: queue limit in bytes
    pub limit_bytes: Option<u32>,
    /// Minimum burst for timing (mtu, typically 1500)
    pub mtu: Option<u32>,
}

impl Default for TbfConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rate_bps: 1_000_000,  // 1 Mbps default
            burst_bytes: 15000,   // ~10 packets at 1500 MTU
            latency_ms: Some(50), // 50ms max latency
            limit_bytes: None,
            mtu: None,
        }
    }
}

impl QdiscConfig for TbfConfig {
    fn qdisc_type(&self) -> QdiscType {
        QdiscType::Tbf
    }
    
    fn validate(&self) -> Result<(), QdiscValidationError> {
        if self.rate_bps == 0 {
            return Err(QdiscValidationError {
                qdisc: QdiscType::Tbf,
                field: "rate_bps".to_string(),
                message: "Rate must be greater than 0".to_string(),
            });
        }
        
        // Minimum burst = rate / HZ (assume HZ=250)
        let min_burst = (self.rate_bps / 250) as u32;
        if self.burst_bytes < min_burst {
            return Err(QdiscValidationError {
                qdisc: QdiscType::Tbf,
                field: "burst_bytes".to_string(),
                message: format!(
                    "Burst must be at least {} bytes for rate {} bps",
                    min_burst, self.rate_bps
                ),
            });
        }
        
        if self.latency_ms.is_none() && self.limit_bytes.is_none() {
            return Err(QdiscValidationError {
                qdisc: QdiscType::Tbf,
                field: "latency_ms/limit_bytes".to_string(),
                message: "Either latency or limit must be specified".to_string(),
            });
        }
        
        Ok(())
    }
    
    fn build_tc_args(&self) -> Vec<String> {
        let mut args = vec!["tbf".to_string()];
        
        // Rate (required)
        args.extend(["rate".to_string(), format!("{}bit", self.rate_bps)]);
        
        // Burst (required)
        args.extend(["burst".to_string(), format!("{}", self.burst_bytes)]);
        
        // Latency OR limit (one required)
        if let Some(latency) = self.latency_ms {
            args.extend(["latency".to_string(), format!("{}ms", latency)]);
        } else if let Some(limit) = self.limit_bytes {
            args.extend(["limit".to_string(), format!("{}", limit)]);
        }
        
        // Optional MTU
        if let Some(mtu) = self.mtu {
            args.extend(["mtu".to_string(), format!("{}", mtu)]);
        }
        
        args
    }
    
    fn parse_tc_output(output: &str) -> Option<Self> {
        if !output.contains("tbf") {
            return None;
        }
        // Parse "qdisc tbf 8001: root refcnt 2 rate 1Mbit burst 15Kb lat 50ms"
        // ... parsing logic
    }
    
    fn is_empty(&self) -> bool {
        !self.enabled
    }
}
```

Create `tcgui-shared/src/qdisc/htb.rs`:

```rust
//! Hierarchical Token Bucket qdisc configuration

use super::{QdiscConfig, QdiscType, QdiscValidationError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTB Root qdisc configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtbConfig {
    /// Whether HTB is enabled
    pub enabled: bool,
    /// Default class for unclassified traffic (class ID)
    pub default_class: Option<u32>,
    /// Rate to bandwidth ratio (r2q) - advanced tuning
    pub r2q: Option<u32>,
    /// Child classes
    pub classes: HashMap<u32, HtbClass>,
}

/// HTB Class configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtbClass {
    /// Class ID (1:10, 1:20, etc. - we store just the minor number)
    pub class_id: u32,
    /// Parent class ID (None = attached to root)
    pub parent: Option<u32>,
    /// Guaranteed rate in bits per second
    pub rate_bps: u64,
    /// Maximum rate (burst) in bits per second
    pub ceil_bps: Option<u64>,
    /// Burst size in bytes
    pub burst_bytes: Option<u32>,
    /// Ceil burst size in bytes
    pub cburst_bytes: Option<u32>,
    /// Priority (lower = higher priority)
    pub priority: Option<u8>,
    /// Quantum (advanced: bytes to send before switching classes)
    pub quantum: Option<u32>,
}

impl Default for HtbConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_class: Some(10),
            r2q: None,
            classes: HashMap::new(),
        }
    }
}

impl HtbConfig {
    /// Create a simple single-class HTB for rate limiting
    pub fn simple_rate_limit(rate_bps: u64) -> Self {
        let mut classes = HashMap::new();
        classes.insert(10, HtbClass {
            class_id: 10,
            parent: None,
            rate_bps,
            ceil_bps: Some(rate_bps),
            burst_bytes: None,
            cburst_bytes: None,
            priority: None,
            quantum: None,
        });
        
        Self {
            enabled: true,
            default_class: Some(10),
            r2q: None,
            classes,
        }
    }
    
    /// Create a two-tier bandwidth allocation
    /// 
    /// Example: 80% guaranteed for high priority, 20% for low priority
    /// Both can burst to full bandwidth if available
    pub fn two_tier(total_bps: u64, high_priority_percent: u8) -> Self {
        let high_rate = (total_bps * high_priority_percent as u64) / 100;
        let low_rate = total_bps - high_rate;
        
        let mut classes = HashMap::new();
        
        // High priority class
        classes.insert(10, HtbClass {
            class_id: 10,
            parent: None,
            rate_bps: high_rate,
            ceil_bps: Some(total_bps),
            burst_bytes: None,
            cburst_bytes: None,
            priority: Some(1),
            quantum: None,
        });
        
        // Low priority class
        classes.insert(20, HtbClass {
            class_id: 20,
            parent: None,
            rate_bps: low_rate,
            ceil_bps: Some(total_bps),
            burst_bytes: None,
            cburst_bytes: None,
            priority: Some(2),
            quantum: None,
        });
        
        Self {
            enabled: true,
            default_class: Some(20),  // Unclassified goes to low priority
            r2q: None,
            classes,
        }
    }
}

impl QdiscConfig for HtbConfig {
    fn qdisc_type(&self) -> QdiscType {
        QdiscType::Htb
    }
    
    fn validate(&self) -> Result<(), QdiscValidationError> {
        if self.classes.is_empty() {
            return Err(QdiscValidationError {
                qdisc: QdiscType::Htb,
                field: "classes".to_string(),
                message: "HTB requires at least one class".to_string(),
            });
        }
        
        if let Some(default) = self.default_class {
            if !self.classes.contains_key(&default) {
                return Err(QdiscValidationError {
                    qdisc: QdiscType::Htb,
                    field: "default_class".to_string(),
                    message: format!("Default class {} does not exist", default),
                });
            }
        }
        
        for (id, class) in &self.classes {
            if class.rate_bps == 0 {
                return Err(QdiscValidationError {
                    qdisc: QdiscType::Htb,
                    field: format!("classes[{}].rate_bps", id),
                    message: "Class rate must be greater than 0".to_string(),
                });
            }
            
            if let Some(ceil) = class.ceil_bps {
                if ceil < class.rate_bps {
                    return Err(QdiscValidationError {
                        qdisc: QdiscType::Htb,
                        field: format!("classes[{}].ceil_bps", id),
                        message: "Ceil must be >= rate".to_string(),
                    });
                }
            }
        }
        
        Ok(())
    }
    
    fn build_tc_args(&self) -> Vec<String> {
        let mut args = vec!["htb".to_string()];
        
        if let Some(default) = self.default_class {
            args.extend(["default".to_string(), format!("{:x}", default)]);
        }
        
        if let Some(r2q) = self.r2q {
            args.extend(["r2q".to_string(), format!("{}", r2q)]);
        }
        
        args
    }
    
    /// Build tc class commands for all classes
    pub fn build_class_commands(&self, interface: &str, handle: &str) -> Vec<Vec<String>> {
        self.classes.values().map(|class| {
            let mut args = vec![
                "class".to_string(),
                "add".to_string(),
                "dev".to_string(),
                interface.to_string(),
                "parent".to_string(),
                if let Some(parent) = class.parent {
                    format!("{}:{:x}", handle, parent)
                } else {
                    format!("{}:", handle)
                },
                "classid".to_string(),
                format!("{}:{:x}", handle, class.class_id),
                "htb".to_string(),
                "rate".to_string(),
                format!("{}bit", class.rate_bps),
            ];
            
            if let Some(ceil) = class.ceil_bps {
                args.extend(["ceil".to_string(), format!("{}bit", ceil)]);
            }
            
            if let Some(burst) = class.burst_bytes {
                args.extend(["burst".to_string(), format!("{}", burst)]);
            }
            
            if let Some(cburst) = class.cburst_bytes {
                args.extend(["cburst".to_string(), format!("{}", cburst)]);
            }
            
            if let Some(prio) = class.priority {
                args.extend(["prio".to_string(), format!("{}", prio)]);
            }
            
            if let Some(quantum) = class.quantum {
                args.extend(["quantum".to_string(), format!("{}", quantum)]);
            }
            
            args
        }).collect()
    }
    
    fn parse_tc_output(output: &str) -> Option<Self> {
        if !output.contains("htb") {
            return None;
        }
        // Complex parsing for HTB classes required
        // ... parsing logic
    }
    
    fn is_empty(&self) -> bool {
        !self.enabled || self.classes.is_empty()
    }
}
```

### 1.3 Unified Configuration Enum

Add to `tcgui-shared/src/qdisc/mod.rs`:

```rust
/// Unified qdisc configuration that can hold any qdisc type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UnifiedQdiscConfig {
    Netem(NetemConfig),
    Tbf(TbfConfig),
    Htb(HtbConfig),
}

impl UnifiedQdiscConfig {
    pub fn qdisc_type(&self) -> QdiscType {
        match self {
            Self::Netem(_) => QdiscType::Netem,
            Self::Tbf(_) => QdiscType::Tbf,
            Self::Htb(_) => QdiscType::Htb,
        }
    }
    
    pub fn validate(&self) -> Result<(), QdiscValidationError> {
        match self {
            Self::Netem(c) => c.validate(),
            Self::Tbf(c) => c.validate(),
            Self::Htb(c) => c.validate(),
        }
    }
    
    pub fn build_tc_args(&self) -> Vec<String> {
        match self {
            Self::Netem(c) => c.build_tc_args(),
            Self::Tbf(c) => c.build_tc_args(),
            Self::Htb(c) => c.build_tc_args(),
        }
    }
}
```

---

## Phase 2: Refactor Netem to Use Abstraction

### 2.1 Update TcCommandManager

Modify `tcgui-backend/src/tc_commands.rs`:

```rust
use tcgui_shared::qdisc::{QdiscConfig, UnifiedQdiscConfig};

impl TcCommandManager {
    /// Apply any qdisc configuration
    pub async fn apply_qdisc(
        &self,
        namespace: &str,
        interface: &str,
        config: &UnifiedQdiscConfig,
    ) -> Result<String> {
        config.validate().map_err(|e| TcguiError::TcCommandError {
            message: format!("Qdisc validation failed: {}", e.message),
        })?;
        
        // Check existing qdisc
        let existing = self.check_existing_qdisc(namespace, interface).await?;
        
        // Determine action based on existing qdisc
        let action = if existing.is_empty() || existing.contains("noqueue") {
            "add"
        } else if self.same_qdisc_type(&existing, config.qdisc_type()) {
            "replace"
        } else {
            // Different qdisc type - need to delete first
            self.remove_tc_config_in_namespace(namespace, interface).await?;
            "add"
        };
        
        self.execute_qdisc_command(namespace, interface, action, config).await
    }
    
    async fn execute_qdisc_command(
        &self,
        namespace: &str,
        interface: &str,
        action: &str,
        config: &UnifiedQdiscConfig,
    ) -> Result<String> {
        let mut cmd = self.build_base_command(namespace);
        cmd.args(["qdisc", action, "dev", interface, "root"]);
        cmd.args(&config.build_tc_args());
        
        info!("Executing TC command: {:?}", cmd);
        
        let output = cmd.output().await?;
        
        // For HTB, also add classes
        if let UnifiedQdiscConfig::Htb(htb_config) = config {
            let handle = "1";  // Standard root handle
            for class_args in htb_config.build_class_commands(interface, handle) {
                let mut class_cmd = self.build_base_command(namespace);
                class_cmd.args(&class_args);
                class_cmd.output().await?;
            }
        }
        
        self.process_output(output, action)
    }
    
    fn same_qdisc_type(&self, existing: &str, qtype: QdiscType) -> bool {
        existing.contains(qtype.tc_name())
    }
}
```

### 2.2 Update Request/Response Types

Modify `tcgui-shared/src/lib.rs`:

```rust
/// Traffic control operations (updated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TcOperation {
    /// Apply qdisc configuration (unified)
    ApplyQdisc { config: UnifiedQdiscConfig },
    
    /// Legacy: Apply netem configuration
    #[deprecated(note = "Use ApplyQdisc with UnifiedQdiscConfig::Netem")]
    ApplyConfig { config: TcNetemConfig },
    
    /// Remove all traffic control configuration
    Remove,
    
    /// Get current qdisc configuration
    Query,
}

/// Updated response with qdisc info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcResponse {
    pub success: bool,
    pub message: String,
    pub applied_config: Option<UnifiedQdiscConfig>,
    pub qdisc_type: Option<QdiscType>,
    pub error_code: Option<i32>,
}
```

---

## Phase 3: Implement HTB

### 3.1 HTB Command Builder

Add to `tcgui-backend/src/tc_commands.rs`:

```rust
impl TcCommandManager {
    /// Apply HTB configuration with classes
    pub async fn apply_htb(
        &self,
        namespace: &str,
        interface: &str,
        config: &HtbConfig,
    ) -> Result<String> {
        // 1. Add root HTB qdisc
        let qdisc_args = config.build_tc_args();
        self.execute_tc_command_with_args(namespace, interface, "add", &qdisc_args).await?;
        
        // 2. Add each class
        let handle = "1";
        for class_args in config.build_class_commands(interface, handle) {
            let mut cmd = self.build_base_command(namespace);
            cmd.args(&class_args);
            
            let output = cmd.output().await?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!("Failed to add HTB class: {}", stderr);
            }
        }
        
        Ok("HTB configuration applied".to_string())
    }
}
```

### 3.2 HTB Detection

```rust
impl TcCommandManager {
    /// Detect current HTB configuration
    pub async fn detect_htb_config(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<Option<HtbConfig>> {
        // Get qdisc info
        let qdisc_output = self.check_existing_qdisc(namespace, interface).await?;
        if !qdisc_output.contains("htb") {
            return Ok(None);
        }
        
        // Get class info
        let class_output = self.get_tc_classes(namespace, interface).await?;
        
        // Parse into HtbConfig
        HtbConfig::parse_full(&qdisc_output, &class_output)
    }
    
    async fn get_tc_classes(&self, namespace: &str, interface: &str) -> Result<String> {
        let mut cmd = self.build_base_command(namespace);
        cmd.args(["class", "show", "dev", interface]);
        
        let output = cmd.output().await?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
```

---

## Phase 4: Implement TBF

### 4.1 TBF Command Builder

```rust
impl TcCommandManager {
    /// Apply TBF configuration
    pub async fn apply_tbf(
        &self,
        namespace: &str,
        interface: &str,
        config: &TbfConfig,
    ) -> Result<String> {
        let args = config.build_tc_args();
        self.execute_tc_command_with_args(namespace, interface, "add", &args).await
    }
}
```

### 4.2 TBF Detection

```rust
impl TcCommandManager {
    /// Detect current TBF configuration
    pub async fn detect_tbf_config(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<Option<TbfConfig>> {
        let output = self.check_existing_qdisc(namespace, interface).await?;
        TbfConfig::parse_tc_output(&output)
    }
}
```

---

## Phase 5: Add Qdisc Selection UI

### 5.1 Qdisc Selector Component

Create `tcgui-frontend/src/components/qdisc_selector.rs`:

```rust
use iced::{Element, Length};
use iced::widget::{column, pick_list, row, text};
use tcgui_shared::qdisc::QdiscType;

#[derive(Debug, Clone)]
pub enum QdiscSelectorMessage {
    QdiscTypeSelected(QdiscType),
}

pub struct QdiscSelector {
    selected: QdiscType,
    available: Vec<QdiscType>,
}

impl QdiscSelector {
    pub fn new() -> Self {
        Self {
            selected: QdiscType::Netem,
            available: vec![QdiscType::Netem, QdiscType::Tbf, QdiscType::Htb],
        }
    }
    
    pub fn view(&self) -> Element<QdiscSelectorMessage> {
        let selector = pick_list(
            &self.available,
            Some(self.selected),
            QdiscSelectorMessage::QdiscTypeSelected,
        );
        
        column![
            text("Qdisc Type"),
            selector,
            text(self.selected.description()),
        ]
        .spacing(5)
        .into()
    }
    
    pub fn update(&mut self, message: QdiscSelectorMessage) {
        match message {
            QdiscSelectorMessage::QdiscTypeSelected(qtype) => {
                self.selected = qtype;
            }
        }
    }
}

impl QdiscType {
    fn description(&self) -> &'static str {
        match self {
            QdiscType::Netem => "Network emulation: delay, loss, corruption, reordering",
            QdiscType::Tbf => "Token bucket: precise rate limiting with burst control",
            QdiscType::Htb => "Hierarchical: bandwidth allocation with priorities",
        }
    }
}
```

### 5.2 Integrate with Interface View

Update `tcgui-frontend/src/interface/base.rs`:

```rust
use crate::components::qdisc_selector::{QdiscSelector, QdiscSelectorMessage};

pub struct TcInterface {
    // ... existing fields
    qdisc_selector: QdiscSelector,
    netem_config: NetemConfigView,
    tbf_config: TbfConfigView,
    htb_config: HtbConfigView,
}

impl TcInterface {
    pub fn view(&self) -> Element<InterfaceMessage> {
        let qdisc_view = self.qdisc_selector.view()
            .map(InterfaceMessage::QdiscSelector);
        
        let config_view = match self.qdisc_selector.selected() {
            QdiscType::Netem => self.netem_config.view(),
            QdiscType::Tbf => self.tbf_config.view(),
            QdiscType::Htb => self.htb_config.view(),
        };
        
        column![
            qdisc_view,
            config_view,
        ]
        .into()
    }
}
```

---

## Phase 6: Implement Qdisc Chaining (Advanced)

### 6.1 Chaining Architecture

For realistic network simulation, chain netem with rate limiting:

```
                    ┌─────────┐    ┌─────────┐
  Packets ────────► │   HTB   │───►│  netem  │────► Network
                    │ (rate)  │    │ (delay) │
                    └─────────┘    └─────────┘
```

This requires using HTB as the root qdisc with netem as a leaf qdisc.

### 6.2 Chained Configuration

```rust
/// Configuration for chained qdiscs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainedQdiscConfig {
    /// Root qdisc (typically HTB for rate control)
    pub root: UnifiedQdiscConfig,
    /// Leaf qdisc (typically netem for emulation)
    pub leaf: Option<UnifiedQdiscConfig>,
}

impl ChainedQdiscConfig {
    /// Create HTB + netem chain for realistic WAN simulation
    pub fn wan_simulation(
        rate_bps: u64,
        delay_ms: f32,
        loss_percent: f32,
    ) -> Self {
        let htb = HtbConfig::simple_rate_limit(rate_bps);
        
        let netem = NetemConfig {
            delay: DelayConfig {
                enabled: true,
                base_ms: delay_ms,
                ..Default::default()
            },
            loss: LossConfig {
                enabled: loss_percent > 0.0,
                percentage: loss_percent,
                ..Default::default()
            },
            ..Default::default()
        };
        
        Self {
            root: UnifiedQdiscConfig::Htb(htb),
            leaf: Some(UnifiedQdiscConfig::Netem(netem)),
        }
    }
}
```

### 6.3 Chained Command Builder

```rust
impl TcCommandManager {
    pub async fn apply_chained(
        &self,
        namespace: &str,
        interface: &str,
        config: &ChainedQdiscConfig,
    ) -> Result<String> {
        // 1. Apply root qdisc
        self.apply_qdisc(namespace, interface, &config.root).await?;
        
        // 2. If leaf exists and root is classful, attach to class
        if let (Some(leaf), UnifiedQdiscConfig::Htb(htb)) = (&config.leaf, &config.root) {
            let default_class = htb.default_class.unwrap_or(10);
            
            // Add leaf qdisc to default class
            let mut cmd = self.build_base_command(namespace);
            cmd.args([
                "qdisc", "add", "dev", interface,
                "parent", &format!("1:{:x}", default_class),
                "handle", "10:",
            ]);
            cmd.args(&leaf.build_tc_args());
            
            cmd.output().await?;
        }
        
        Ok("Chained qdisc configuration applied".to_string())
    }
}
```

---

## File Structure After Implementation

```
tcgui-shared/src/
├── lib.rs
├── qdisc/
│   ├── mod.rs              # Traits, QdiscType, UnifiedQdiscConfig
│   ├── netem.rs            # NetemConfig (refactored from TcNetemConfig)
│   ├── tbf.rs              # TbfConfig
│   ├── htb.rs              # HtbConfig, HtbClass
│   └── chained.rs          # ChainedQdiscConfig

tcgui-backend/src/
├── tc_commands.rs          # Updated with qdisc abstraction
├── qdisc/
│   ├── mod.rs              # Command builders
│   ├── netem_builder.rs    # Netem-specific command logic
│   ├── tbf_builder.rs      # TBF-specific command logic
│   ├── htb_builder.rs      # HTB-specific command logic (with classes)
│   └── detector.rs         # Parse tc output to detect qdisc configs

tcgui-frontend/src/
├── components/
│   ├── qdisc_selector.rs   # Qdisc type picker
│   ├── netem_config.rs     # Netem configuration UI
│   ├── tbf_config.rs       # TBF configuration UI
│   └── htb_config.rs       # HTB configuration UI (with class editor)
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tbf_validation() {
        let config = TbfConfig {
            enabled: true,
            rate_bps: 1_000_000,
            burst_bytes: 15000,
            latency_ms: Some(50),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_tbf_build_args() {
        let config = TbfConfig {
            enabled: true,
            rate_bps: 1_000_000,
            burst_bytes: 15000,
            latency_ms: Some(50),
            ..Default::default()
        };
        let args = config.build_tc_args();
        assert!(args.contains(&"tbf".to_string()));
        assert!(args.contains(&"rate".to_string()));
    }
    
    #[test]
    fn test_htb_class_hierarchy() {
        let config = HtbConfig::two_tier(10_000_000, 80);
        assert_eq!(config.classes.len(), 2);
        assert!(config.validate().is_ok());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_apply_tbf_qdisc() {
    let tc = TcCommandManager::new();
    let config = TbfConfig::default();
    
    // Requires root/CAP_NET_ADMIN and test interface
    let result = tc.apply_qdisc(
        "default",
        "veth-test",
        &UnifiedQdiscConfig::Tbf(config),
    ).await;
    
    assert!(result.is_ok());
}
```

---

## Rollout Plan

1. **Phase 1-2**: Internal refactoring, no user-visible changes
2. **Phase 3-4**: New qdisc types behind feature flag
3. **Phase 5**: UI changes with qdisc selector
4. **Phase 6**: Advanced chaining for power users

---

## References

- [Linux TC Manual](https://man7.org/linux/man-pages/man8/tc.8.html)
- [HTB Documentation](http://luxik.cdi.cz/~devik/qos/htb/manual/userg.htm)
- [TBF Documentation](https://tldp.org/HOWTO/Traffic-Control-HOWTO/classless-qdiscs.html#qs-tbf)
- [Netem Documentation](https://man7.org/linux/man-pages/man8/tc-netem.8.html)
