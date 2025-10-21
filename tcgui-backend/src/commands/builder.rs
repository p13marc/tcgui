//! Enhanced TC command builder with validation and extensibility.
//!
//! This module provides a comprehensive builder pattern for generating
//! Linux traffic control (tc) commands with strong validation, extensibility,
//! and support for multiple qdisc types beyond just netem.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::process::Command;
use tracing::{debug, info, warn};

/// Traffic control qdisc types supported by the builder
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QdiscType {
    /// Network emulation qdisc (netem)
    Netem,
    /// Token bucket filter (tbf) for rate limiting
    Tbf,
    /// Class-based queueing (cbq)
    Cbq,
    /// Hierarchical token bucket (htb)
    Htb,
    /// Priority queueing (prio)
    Prio,
    /// Fair queueing (sfq)
    Sfq,
    /// Random early detection (red)
    Red,
    /// Controlled delay (codel)
    Codel,
    /// Fair queue codel (fq_codel)
    FqCodel,
    /// Ingress qdisc for ingress traffic
    Ingress,
    /// Custom qdisc type
    Custom(String),
}

impl fmt::Display for QdiscType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QdiscType::Netem => write!(f, "netem"),
            QdiscType::Tbf => write!(f, "tbf"),
            QdiscType::Cbq => write!(f, "cbq"),
            QdiscType::Htb => write!(f, "htb"),
            QdiscType::Prio => write!(f, "prio"),
            QdiscType::Sfq => write!(f, "sfq"),
            QdiscType::Red => write!(f, "red"),
            QdiscType::Codel => write!(f, "codel"),
            QdiscType::FqCodel => write!(f, "fq_codel"),
            QdiscType::Ingress => write!(f, "ingress"),
            QdiscType::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// TC command operation type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TcOperation {
    Add,
    Replace,
    Delete,
    Show,
    Change,
}

impl fmt::Display for TcOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TcOperation::Add => write!(f, "add"),
            TcOperation::Replace => write!(f, "replace"),
            TcOperation::Delete => write!(f, "del"),
            TcOperation::Show => write!(f, "show"),
            TcOperation::Change => write!(f, "change"),
        }
    }
}

/// TC command target (where to apply the qdisc)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TcTarget {
    /// Root qdisc
    Root,
    /// Ingress qdisc
    Ingress,
    /// Specific handle
    Handle(String),
    /// Parent handle
    Parent(String),
}

impl fmt::Display for TcTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TcTarget::Root => write!(f, "root"),
            TcTarget::Ingress => write!(f, "ingress"),
            TcTarget::Handle(handle) => write!(f, "handle {}", handle),
            TcTarget::Parent(parent) => write!(f, "parent {}", parent),
        }
    }
}

/// Netem-specific parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetemParams {
    pub loss_percent: Option<f32>,
    pub loss_correlation: Option<f32>,
    pub delay_ms: Option<f32>,
    pub delay_jitter_ms: Option<f32>,
    pub delay_correlation: Option<f32>,
    pub duplicate_percent: Option<f32>,
    pub duplicate_correlation: Option<f32>,
    pub reorder_percent: Option<f32>,
    pub reorder_correlation: Option<f32>,
    pub reorder_gap: Option<u32>,
    pub corrupt_percent: Option<f32>,
    pub corrupt_correlation: Option<f32>,
    pub rate_limit_kbps: Option<u32>,
}

/// Token bucket filter (TBF) parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TbfParams {
    pub rate: String,          // e.g., "1mbit", "100kbit"
    pub burst: Option<String>, // e.g., "32kbit", "1600b"
    pub limit: Option<String>, // e.g., "3000b"
    pub peakrate: Option<String>,
    pub mtu: Option<String>,
}

/// Hierarchical token bucket (HTB) parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtbParams {
    pub default_class: Option<String>,
    pub r2q: Option<u32>,
    pub direct_qlen: Option<u32>,
}

/// Priority qdisc parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrioParams {
    pub bands: Option<u32>,
    pub priomap: Option<Vec<u32>>,
}

/// Stochastic fair queueing (SFQ) parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SfqParams {
    pub perturb: Option<u32>,
    pub quantum: Option<u32>,
    pub limit: Option<u32>,
}

/// Random early detection (RED) parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedParams {
    pub limit: Option<u32>,
    pub min: Option<u32>,
    pub max: Option<u32>,
    pub avpkt: Option<u32>,
    pub burst: Option<u32>,
    pub probability: Option<f32>,
    pub bandwidth: Option<String>,
}

/// Qdisc-specific parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QdiscParams {
    Netem(NetemParams),
    Tbf(TbfParams),
    Htb(HtbParams),
    Prio(PrioParams),
    Sfq(SfqParams),
    Red(RedParams),
    /// No parameters for simple qdiscs
    None,
    /// Custom parameters as key-value pairs
    Custom(HashMap<String, String>),
}

/// Enhanced TC command builder
#[derive(Debug, Clone)]
pub struct TcCommandBuilder {
    /// Command operation (add, replace, delete, etc.)
    operation: Option<TcOperation>,
    /// Target device interface
    device: Option<String>,
    /// Network namespace (None for default)
    namespace: Option<String>,
    /// Qdisc type
    qdisc_type: Option<QdiscType>,
    /// Command target (root, ingress, handle, parent)
    target: Option<TcTarget>,
    /// Qdisc-specific parameters
    params: Option<QdiscParams>,
    /// Whether to use sudo
    use_sudo: bool,
    /// Additional raw arguments
    raw_args: Vec<String>,
    /// Validation enabled
    validate: bool,
}

impl TcCommandBuilder {
    /// Create a new TC command builder
    pub fn new() -> Self {
        Self {
            operation: None,
            device: None,
            namespace: None,
            qdisc_type: None,
            target: None,
            params: None,
            use_sudo: false,
            raw_args: Vec::new(),
            validate: true,
        }
    }

    /// Set the TC operation
    pub fn operation(mut self, op: TcOperation) -> Self {
        self.operation = Some(op);
        self
    }

    /// Set the target device
    pub fn device<S: Into<String>>(mut self, device: S) -> Self {
        self.device = Some(device.into());
        self
    }

    /// Set the network namespace
    pub fn namespace<S: Into<String>>(mut self, namespace: S) -> Self {
        let ns = namespace.into();
        self.namespace = if ns == "default" { None } else { Some(ns) };
        self
    }

    /// Set the qdisc type
    pub fn qdisc(mut self, qdisc_type: QdiscType) -> Self {
        self.qdisc_type = Some(qdisc_type);
        self
    }

    /// Set the command target
    pub fn target(mut self, target: TcTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Set qdisc parameters
    pub fn params(mut self, params: QdiscParams) -> Self {
        self.params = Some(params);
        self
    }

    /// Enable sudo usage
    pub fn with_sudo(mut self) -> Self {
        self.use_sudo = true;
        self
    }

    /// Add raw arguments
    pub fn raw_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.raw_args.extend(args.into_iter().map(|s| s.into()));
        self
    }

    /// Disable validation
    pub fn skip_validation(mut self) -> Self {
        self.validate = false;
        self
    }

    /// Build and validate the command
    pub fn build(self) -> Result<TcCommand> {
        if self.validate {
            self.validate_configuration()?;
        }

        Ok(TcCommand {
            operation: self.operation.unwrap_or(TcOperation::Add),
            device: self.device.unwrap_or_else(|| "eth0".to_string()),
            namespace: self.namespace,
            qdisc_type: self.qdisc_type.unwrap_or(QdiscType::Netem),
            target: self.target.unwrap_or(TcTarget::Root),
            params: self.params.unwrap_or(QdiscParams::None),
            use_sudo: self.use_sudo,
            raw_args: self.raw_args,
        })
    }

    /// Validate the command configuration
    fn validate_configuration(&self) -> Result<()> {
        // Validate operation
        let operation = self
            .operation
            .as_ref()
            .ok_or_else(|| anyhow!("TC operation must be specified"))?;

        // Validate device
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| anyhow!("Device interface must be specified"))?;

        if device.is_empty() {
            return Err(anyhow!("Device interface cannot be empty"));
        }

        // Validate qdisc type for operations that need it
        if matches!(
            operation,
            TcOperation::Add | TcOperation::Replace | TcOperation::Change
        ) {
            let qdisc_type = self.qdisc_type.as_ref().ok_or_else(|| {
                anyhow!("Qdisc type must be specified for {} operation", operation)
            })?;

            // Validate qdisc-specific parameters
            if let Some(ref params) = self.params {
                self.validate_qdisc_params(qdisc_type, params)?;
            }
        }

        // Validate namespace
        if let Some(ref namespace) = self.namespace {
            if namespace.is_empty() {
                return Err(anyhow!("Namespace cannot be empty"));
            }
        }

        Ok(())
    }

    /// Validate qdisc-specific parameters
    fn validate_qdisc_params(&self, qdisc_type: &QdiscType, params: &QdiscParams) -> Result<()> {
        match (qdisc_type, params) {
            (QdiscType::Netem, QdiscParams::Netem(netem)) => {
                self.validate_netem_params(netem)?;
            }
            (QdiscType::Tbf, QdiscParams::Tbf(tbf)) => {
                self.validate_tbf_params(tbf)?;
            }
            (QdiscType::Netem, _) => {
                warn!("Netem qdisc without netem parameters - using defaults");
            }
            (QdiscType::Tbf, _) => {
                return Err(anyhow!("TBF qdisc requires TBF parameters"));
            }
            _ => {
                // Other combinations are allowed
            }
        }
        Ok(())
    }

    /// Validate netem parameters
    fn validate_netem_params(&self, params: &NetemParams) -> Result<()> {
        // Validate loss percentage
        if let Some(loss) = params.loss_percent {
            if !(0.0..=100.0).contains(&loss) {
                return Err(anyhow!("Loss percentage must be between 0.0 and 100.0"));
            }
        }

        // Validate correlation values
        for (name, value) in [
            ("loss_correlation", params.loss_correlation),
            ("delay_correlation", params.delay_correlation),
            ("duplicate_correlation", params.duplicate_correlation),
            ("reorder_correlation", params.reorder_correlation),
            ("corrupt_correlation", params.corrupt_correlation),
        ] {
            if let Some(corr) = value {
                if !(0.0..=100.0).contains(&corr) {
                    return Err(anyhow!("{} must be between 0.0 and 100.0", name));
                }
            }
        }

        // Validate delay values
        if let Some(delay) = params.delay_ms {
            if delay < 0.0 {
                return Err(anyhow!("Delay cannot be negative"));
            }
        }

        if let Some(jitter) = params.delay_jitter_ms {
            if jitter < 0.0 {
                return Err(anyhow!("Delay jitter cannot be negative"));
            }
        }

        // Validate percentage values
        for (name, value) in [
            ("duplicate_percent", params.duplicate_percent),
            ("reorder_percent", params.reorder_percent),
            ("corrupt_percent", params.corrupt_percent),
        ] {
            if let Some(percent) = value {
                if !(0.0..=100.0).contains(&percent) {
                    return Err(anyhow!("{} must be between 0.0 and 100.0", name));
                }
            }
        }

        // Validate reorder gap
        if let Some(gap) = params.reorder_gap {
            if gap == 0 {
                return Err(anyhow!("Reorder gap must be greater than 0"));
            }
        }

        // Validate rate limit
        if let Some(rate) = params.rate_limit_kbps {
            if rate == 0 {
                return Err(anyhow!("Rate limit must be greater than 0"));
            }
        }

        Ok(())
    }

    /// Validate TBF parameters
    fn validate_tbf_params(&self, params: &TbfParams) -> Result<()> {
        if params.rate.is_empty() {
            return Err(anyhow!("TBF rate must be specified"));
        }

        // Validate rate format (basic check)
        if !params.rate.ends_with("bit") && !params.rate.ends_with("bps") {
            return Err(anyhow!(
                "TBF rate must end with 'bit' or 'bps' (e.g., '1mbit', '100kbps')"
            ));
        }

        Ok(())
    }
}

impl Default for TcCommandBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Built and validated TC command
#[derive(Debug, Clone)]
pub struct TcCommand {
    operation: TcOperation,
    device: String,
    namespace: Option<String>,
    qdisc_type: QdiscType,
    target: TcTarget,
    params: QdiscParams,
    use_sudo: bool,
    raw_args: Vec<String>,
}

impl TcCommand {
    /// Convert to system command
    pub fn to_command(&self) -> Command {
        let mut cmd = if self.use_sudo {
            let mut cmd = Command::new("sudo");
            if let Some(ref namespace) = self.namespace {
                cmd.args(["ip", "netns", "exec", namespace, "tc"]);
            } else {
                cmd.arg("tc");
            }
            cmd
        } else if let Some(ref namespace) = self.namespace {
            let mut cmd = Command::new("ip");
            cmd.args(["netns", "exec", namespace, "tc"]);
            cmd
        } else {
            Command::new("tc")
        };

        // Add operation and basic structure
        cmd.args(["qdisc", &self.operation.to_string()]);
        cmd.args(["dev", &self.device]);
        cmd.arg(self.target.to_string());

        // Add qdisc type and parameters
        if !matches!(self.operation, TcOperation::Delete | TcOperation::Show) {
            cmd.arg(self.qdisc_type.to_string());
            self.add_qdisc_params(&mut cmd);
        }

        // Add raw arguments
        if !self.raw_args.is_empty() {
            cmd.args(&self.raw_args);
        }

        cmd
    }

    /// Add qdisc-specific parameters to command
    fn add_qdisc_params(&self, cmd: &mut Command) {
        match &self.params {
            QdiscParams::Netem(params) => {
                self.add_netem_params(cmd, params);
            }
            QdiscParams::Tbf(params) => {
                self.add_tbf_params(cmd, params);
            }
            QdiscParams::Htb(params) => {
                self.add_htb_params(cmd, params);
            }
            QdiscParams::Prio(params) => {
                self.add_prio_params(cmd, params);
            }
            QdiscParams::Sfq(params) => {
                self.add_sfq_params(cmd, params);
            }
            QdiscParams::Red(params) => {
                self.add_red_params(cmd, params);
            }
            QdiscParams::Custom(params) => {
                for (key, value) in params {
                    cmd.args([key, value]);
                }
            }
            QdiscParams::None => {
                // No parameters to add
            }
        }
    }

    /// Add netem parameters
    fn add_netem_params(&self, cmd: &mut Command, params: &NetemParams) {
        if let Some(loss) = params.loss_percent {
            if loss > 0.0 {
                cmd.args(["loss", &format!("{}%", loss)]);
                if let Some(corr) = params.loss_correlation {
                    if corr > 0.0 {
                        cmd.arg(format!("{}%", corr));
                    }
                }
            }
        }

        if let Some(delay) = params.delay_ms {
            if delay > 0.0 {
                cmd.args(["delay", &format!("{}ms", delay)]);
                if let Some(jitter) = params.delay_jitter_ms {
                    if jitter > 0.0 {
                        cmd.arg(format!("{}ms", jitter));
                        if let Some(corr) = params.delay_correlation {
                            if corr > 0.0 {
                                cmd.arg(format!("{}%", corr));
                            }
                        }
                    }
                }
            }
        }

        if let Some(duplicate) = params.duplicate_percent {
            if duplicate > 0.0 {
                cmd.args(["duplicate", &format!("{}%", duplicate)]);
                if let Some(corr) = params.duplicate_correlation {
                    if corr > 0.0 {
                        cmd.arg(format!("{}%", corr));
                    }
                }
            }
        }

        if let Some(reorder) = params.reorder_percent {
            if reorder > 0.0 {
                cmd.args(["reorder", &format!("{}%", reorder)]);
                if let Some(corr) = params.reorder_correlation {
                    if corr > 0.0 {
                        cmd.arg(format!("{}%", corr));
                    }
                }
                if let Some(gap) = params.reorder_gap {
                    cmd.args(["gap", &format!("{}", gap)]);
                }
            }
        }

        if let Some(corrupt) = params.corrupt_percent {
            if corrupt > 0.0 {
                cmd.args(["corrupt", &format!("{}%", corrupt)]);
                if let Some(corr) = params.corrupt_correlation {
                    if corr > 0.0 {
                        cmd.arg(format!("{}%", corr));
                    }
                }
            }
        }

        if let Some(rate) = params.rate_limit_kbps {
            if rate > 0 {
                if rate >= 1000 {
                    cmd.args(["rate", &format!("{}mbit", rate / 1000)]);
                } else {
                    cmd.args(["rate", &format!("{}kbit", rate)]);
                }
            }
        }
    }

    /// Add TBF parameters
    fn add_tbf_params(&self, cmd: &mut Command, params: &TbfParams) {
        cmd.args(["rate", &params.rate]);

        if let Some(ref burst) = params.burst {
            cmd.args(["burst", burst]);
        }

        if let Some(ref limit) = params.limit {
            cmd.args(["limit", limit]);
        }

        if let Some(ref peakrate) = params.peakrate {
            cmd.args(["peakrate", peakrate]);
        }

        if let Some(ref mtu) = params.mtu {
            cmd.args(["mtu", mtu]);
        }
    }

    /// Add HTB parameters
    fn add_htb_params(&self, cmd: &mut Command, params: &HtbParams) {
        if let Some(ref default) = params.default_class {
            cmd.args(["default", default]);
        }

        if let Some(r2q) = params.r2q {
            cmd.args(["r2q", &format!("{}", r2q)]);
        }

        if let Some(direct_qlen) = params.direct_qlen {
            cmd.args(["direct_qlen", &format!("{}", direct_qlen)]);
        }
    }

    /// Add PRIO parameters
    fn add_prio_params(&self, cmd: &mut Command, params: &PrioParams) {
        if let Some(bands) = params.bands {
            cmd.args(["bands", &format!("{}", bands)]);
        }

        if let Some(ref priomap) = params.priomap {
            let priomap_str = priomap
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            cmd.args(["priomap", &priomap_str]);
        }
    }

    /// Add SFQ parameters
    fn add_sfq_params(&self, cmd: &mut Command, params: &SfqParams) {
        if let Some(perturb) = params.perturb {
            cmd.args(["perturb", &format!("{}", perturb)]);
        }

        if let Some(quantum) = params.quantum {
            cmd.args(["quantum", &format!("{}", quantum)]);
        }

        if let Some(limit) = params.limit {
            cmd.args(["limit", &format!("{}", limit)]);
        }
    }

    /// Add RED parameters
    fn add_red_params(&self, cmd: &mut Command, params: &RedParams) {
        if let Some(limit) = params.limit {
            cmd.args(["limit", &format!("{}", limit)]);
        }

        if let Some(min) = params.min {
            cmd.args(["min", &format!("{}", min)]);
        }

        if let Some(max) = params.max {
            cmd.args(["max", &format!("{}", max)]);
        }

        if let Some(avpkt) = params.avpkt {
            cmd.args(["avpkt", &format!("{}", avpkt)]);
        }

        if let Some(burst) = params.burst {
            cmd.args(["burst", &format!("{}", burst)]);
        }

        if let Some(probability) = params.probability {
            cmd.args(["probability", &format!("{}", probability)]);
        }

        if let Some(ref bandwidth) = params.bandwidth {
            cmd.args(["bandwidth", bandwidth]);
        }
    }

    /// Execute the command
    pub async fn execute(&self) -> Result<std::process::Output> {
        let mut cmd = self.to_command();
        debug!("Executing TC command: {:?}", cmd);

        let output = cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("TC command failed: {}", stderr));
        }

        info!("TC command executed successfully");
        Ok(output)
    }

    /// Get command arguments as vector
    pub fn to_args(&self) -> Vec<String> {
        let cmd = self.to_command();
        let program = cmd.get_program().to_string_lossy().to_string();
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();

        let mut result = vec![program];
        result.extend(args);
        result
    }
}

/// Display implementation for TcCommand to support string conversion
impl fmt::Display for TcCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cmd = self.to_command();
        write!(f, "{:?}", cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_netem_command() {
        let netem_params = NetemParams {
            loss_percent: Some(5.0),
            delay_ms: Some(100.0),
            ..Default::default()
        };

        let cmd = TcCommandBuilder::new()
            .operation(TcOperation::Add)
            .device("eth0")
            .qdisc(QdiscType::Netem)
            .params(QdiscParams::Netem(netem_params))
            .build()
            .unwrap();

        let args = cmd.to_args();
        assert!(args.contains(&"netem".to_string()));
        assert!(args.contains(&"loss".to_string()));
        assert!(args.contains(&"5%".to_string()));
        assert!(args.contains(&"delay".to_string()));
        assert!(args.contains(&"100ms".to_string()));
    }

    #[test]
    fn test_tbf_command() {
        let tbf_params = TbfParams {
            rate: "1mbit".to_string(),
            burst: Some("32kbit".to_string()),
            limit: Some("3000b".to_string()),
            peakrate: None,
            mtu: None,
        };

        let cmd = TcCommandBuilder::new()
            .operation(TcOperation::Replace)
            .device("eth1")
            .qdisc(QdiscType::Tbf)
            .params(QdiscParams::Tbf(tbf_params))
            .build()
            .unwrap();

        let args = cmd.to_args();
        assert!(args.contains(&"replace".to_string()));
        assert!(args.contains(&"tbf".to_string()));
        assert!(args.contains(&"rate".to_string()));
        assert!(args.contains(&"1mbit".to_string()));
        assert!(args.contains(&"burst".to_string()));
        assert!(args.contains(&"32kbit".to_string()));
    }

    #[test]
    fn test_namespace_command() {
        let cmd = TcCommandBuilder::new()
            .operation(TcOperation::Delete)
            .device("veth0")
            .namespace("test-ns")
            .qdisc(QdiscType::Netem)
            .with_sudo()
            .build()
            .unwrap();

        let args = cmd.to_args();
        assert!(args.contains(&"sudo".to_string()));
        assert!(args.contains(&"ip".to_string()));
        assert!(args.contains(&"netns".to_string()));
        assert!(args.contains(&"exec".to_string()));
        assert!(args.contains(&"test-ns".to_string()));
    }

    #[test]
    fn test_validation_errors() {
        // Missing operation
        let result = TcCommandBuilder::new().device("eth0").build();
        assert!(result.is_err());

        // Invalid loss percentage
        let netem_params = NetemParams {
            loss_percent: Some(150.0), // Invalid: > 100%
            ..Default::default()
        };

        let result = TcCommandBuilder::new()
            .operation(TcOperation::Add)
            .device("eth0")
            .qdisc(QdiscType::Netem)
            .params(QdiscParams::Netem(netem_params))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_qdisc() {
        let cmd = TcCommandBuilder::new()
            .operation(TcOperation::Add)
            .device("eth0")
            .qdisc(QdiscType::Custom("myqdisc".to_string()))
            .params(QdiscParams::Custom({
                let mut params = HashMap::new();
                params.insert("param1".to_string(), "value1".to_string());
                params.insert("param2".to_string(), "value2".to_string());
                params
            }))
            .build()
            .unwrap();

        let args = cmd.to_args();
        assert!(args.contains(&"myqdisc".to_string()));
        assert!(args.contains(&"param1".to_string()));
        assert!(args.contains(&"value1".to_string()));
    }

    #[test]
    fn test_skip_validation() {
        // This should work even with invalid parameters when validation is disabled
        let netem_params = NetemParams {
            loss_percent: Some(150.0), // Invalid: > 100%
            ..Default::default()
        };

        let result = TcCommandBuilder::new()
            .operation(TcOperation::Add)
            .device("eth0")
            .qdisc(QdiscType::Netem)
            .params(QdiscParams::Netem(netem_params))
            .skip_validation()
            .build();

        assert!(result.is_ok());
    }

    #[test]
    fn test_qdisc_type_display() {
        assert_eq!(QdiscType::Netem.to_string(), "netem");
        assert_eq!(QdiscType::Tbf.to_string(), "tbf");
        assert_eq!(QdiscType::Htb.to_string(), "htb");
        assert_eq!(QdiscType::Custom("test".to_string()).to_string(), "test");
    }

    #[test]
    fn test_target_display() {
        assert_eq!(TcTarget::Root.to_string(), "root");
        assert_eq!(TcTarget::Ingress.to_string(), "ingress");
        assert_eq!(
            TcTarget::Handle("1:0".to_string()).to_string(),
            "handle 1:0"
        );
        assert_eq!(
            TcTarget::Parent("1:1".to_string()).to_string(),
            "parent 1:1"
        );
    }
}
