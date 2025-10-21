//! Traffic Control (TC) command execution and management.
//!
//! This module provides comprehensive traffic control command execution across
//! multiple network namespaces. It handles netem packet loss simulation with
//! correlation support and provides robust error handling and feedback.
//!
//! # Key Features
//!
//! * **Multi-namespace support**: Execute TC commands in default and named namespaces
//! * **Netem simulation**: Packet loss with optional correlation patterns
//! * **Smart command handling**: Automatic fallback from "add" to "replace" operations
//! * **Comprehensive feedback**: Detailed success/error reporting to frontend
//! * **Robust error handling**: Graceful handling of common TC command failures
//!
//! # TC Commands Generated
//!
//! * **Apply**: `tc qdisc {add|replace} dev <interface> root netem loss <loss>% [correlation <corr>%]`
//! * **Remove**: `tc qdisc del dev <interface> root`
//! * **Namespaced**: `ip netns exec <namespace> tc ...` for named namespaces
//!
//! # Examples
//!
//! ```rust,no_run
//! use tcgui_backend::tc_commands::TcCommandManager;
//!
//! async fn apply_tc_config() -> anyhow::Result<()> {
//!     let tc_manager = TcCommandManager::new();
//!     let result = tc_manager.apply_tc_config_in_namespace(
//!         "test-ns", "eth0", 5.0, Some(10.0), None, None, None, None, None, None, None, None, None, None, None
//!     ).await?;
//!     println!("TC result: {}", result);
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use tokio::process::Command;
use tracing::{error, info, instrument, warn};

use tcgui_shared::{errors::TcguiError, TcNetemConfig, TcValidate};

/// Traffic Control command manager for network emulation.
///
/// This struct manages the execution of Linux `tc` (traffic control) commands
/// across multiple network namespaces. It provides netem-based network emulation
/// with support for packet loss and correlation patterns.
///
/// # Architecture
///
/// * **Default namespace**: Direct `sudo tc` command execution
/// * **Named namespaces**: Uses `sudo ip netns exec <namespace> tc` for isolation
/// * **Smart retry logic**: Automatically tries "replace" if "add" fails
/// * **Result reporting**: Sends comprehensive feedback via Zenoh messaging
///
/// # Supported Parameters
///
/// * **Packet loss**: 0.0 to 100.0 percent packet loss simulation
/// * **Correlation**: Optional consecutive packet loss correlation (0.0 to 100.0)
/// * **Interface targeting**: Operates on specific network interfaces
/// * **Namespace isolation**: Full support for network namespace operations
#[derive(Clone)]
pub struct TcCommandManager {
    // All functionality handled via query servers and publishers now
}

impl Default for TcCommandManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TcCommandManager {
    /// Creates a new TcCommandManager instance.
    ///
    /// # Returns
    ///
    /// A new `TcCommandManager` ready to execute traffic control commands
    /// across multiple network namespaces. Communication is handled via
    /// the query server infrastructure.
    pub fn new() -> Self {
        Self {}
    }

    /// Check if there's an existing qdisc on the interface and return its details.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Target namespace ("default" for host namespace)
    /// * `interface` - Network interface name (e.g., "eth0", "fo")
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Qdisc information (empty string if no qdisc)
    /// * `Err` - On command execution failures
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn check_existing_qdisc(&self, namespace: &str, interface: &str) -> Result<String> {
        let mut cmd = if namespace == "default" {
            let mut cmd = Command::new("tc");
            cmd.args(["qdisc", "show", "dev", interface]);
            cmd
        } else {
            let mut cmd = Command::new("ip");
            cmd.args([
                "netns", "exec", namespace, "tc", "qdisc", "show", "dev", interface,
            ]);
            cmd
        };

        let output = cmd.output().await.map_err(|e| TcguiError::TcCommandError {
            message: format!("Failed to execute tc qdisc show command: {}", e),
        })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Filter out the "root" qdisc line, which is what we're interested in
            for line in stdout.lines() {
                if line.contains("root") {
                    return Ok(line.to_string());
                }
            }
            Ok(String::new()) // No root qdisc found
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(TcguiError::TcCommandError {
                message: format!("tc qdisc show command failed: {}", stderr),
            }
            .into())
        }
    }

    /// Apply TC config using structured configuration (recommended)
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn apply_tc_config_structured(
        &self,
        namespace: &str,
        interface: &str,
        config: &TcNetemConfig,
    ) -> Result<String> {
        // Validate configuration first
        config.validate().map_err(|e| TcguiError::TcCommandError {
            message: format!("TC configuration validation failed: {}", e),
        })?;

        info!(
            "Applying structured TC config: namespace={}, interface={}, config={:?}",
            namespace, interface, config
        );

        // Convert to legacy parameters for now (will be replaced with direct command building later)
        let (
            loss,
            correlation,
            delay_ms,
            delay_jitter_ms,
            delay_correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps,
        ) = config.to_legacy_params();

        // Use existing implementation for now
        self.apply_tc_config_in_namespace(
            namespace,
            interface,
            loss,
            correlation,
            delay_ms,
            delay_jitter_ms,
            delay_correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps,
        )
        .await
    }

    /// Apply TC config in default namespace (legacy method)
    #[allow(dead_code)] // Keep for backward compatibility
    pub async fn apply_tc_config(
        &self,
        interface: &str,
        loss: f32,
        correlation: Option<f32>,
    ) -> Result<String> {
        self.apply_tc_config_in_namespace(
            "default",
            interface,
            loss,
            correlation,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Applies traffic control configuration to an interface in a specific namespace.
    ///
    /// This method configures netem packet loss simulation, delay emulation, and packet duplication on the specified interface
    /// within the given namespace. It uses smart retry logic to handle existing
    /// qdisc configurations gracefully.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Target namespace ("default" for host namespace)
    /// * `interface` - Network interface name (e.g., "eth0", "fo")
    /// * `loss` - Packet loss percentage (0.0 to 100.0)
    /// * `correlation` - Optional correlation for consecutive packet loss (0.0 to 100.0)
    /// * `delay_ms` - Optional base delay in milliseconds (0.0 to 5000.0)
    /// * `delay_jitter_ms` - Optional delay jitter/variation in milliseconds (0.0 to 1000.0)
    /// * `delay_correlation` - Optional delay correlation (0.0 to 100.0)
    /// * `duplicate_percent` - Optional packet duplication percentage (0.0 to 100.0)
    /// * `duplicate_correlation` - Optional duplication correlation (0.0 to 100.0)
    /// * `reorder_percent` - Optional packet reordering percentage (0.0 to 100.0)
    /// * `reorder_correlation` - Optional reordering correlation (0.0 to 100.0)
    /// * `reorder_gap` - Optional reordering gap parameter (1 to 10)
    /// * `corrupt_percent` - Optional packet corruption percentage (0.0 to 100.0)
    /// * `corrupt_correlation` - Optional corruption correlation (0.0 to 100.0)
    /// * `rate_limit_kbps` - Optional rate limiting in kbps (1 to 1000000)
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Success message with operation details
    /// * `Err` - On command execution failures or system errors
    ///
    /// # Commands Generated
    ///
    /// * **Default namespace**: `sudo tc qdisc {add|replace} dev <interface> root netem [loss <loss>%] [delay <delay>ms [<jitter>ms [<corr>%]]] [duplicate <dup>% [<corr>%]] [reorder <reorder>% [<corr>%] [gap <gap>]] [corrupt <corrupt>% [<corr>%]] [rate <rate>kbit|<rate>mbit]`
    /// * **Named namespace**: `sudo ip netns exec <namespace> tc qdisc {add|replace} dev <interface> root netem [loss <loss>%] [delay <delay>ms [<jitter>ms [<corr>%]]] [duplicate <dup>% [<corr>%]] [reorder <reorder>% [<corr>%] [gap <gap>]] [corrupt <corrupt>% [<corr>%]] [rate <rate>kbit|<rate>mbit]`
    ///
    /// # Smart Retry Logic
    ///
    /// 1. **First attempt**: Tries `tc qdisc add` to create new qdisc
    /// 2. **Fallback**: If add fails (qdisc exists), automatically tries `tc qdisc replace`
    /// 3. **Error handling**: Reports detailed failure information if both attempts fail
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tcgui_backend::tc_commands::TcCommandManager;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let tc_manager = TcCommandManager::new();
    /// // Apply 5% packet loss in default namespace
    /// let result = tc_manager.apply_tc_config_in_namespace(
    ///     "default", "eth0", 5.0, None, None, None, None, None, None, None, None, None, None, None, None
    /// ).await?;
    ///
    /// // Apply 10% packet loss with 25% correlation in named namespace
    /// let result = tc_manager.apply_tc_config_in_namespace(
    ///     "test-ns", "eth0", 10.0, Some(25.0), None, None, None, None, None, None, None, None, None, None, None
    /// ).await?;
    ///
    /// // Apply 100ms delay with 10ms jitter and 25% correlation
    /// let result = tc_manager.apply_tc_config_in_namespace(
    ///     "default", "eth0", 0.0, None, Some(100.0), Some(10.0), Some(25.0), None, None, None, None, None, None, None, None
    /// ).await?;
    ///
    /// // Apply 5% packet duplication with 10% correlation
    /// let result = tc_manager.apply_tc_config_in_namespace(
    ///     "default", "eth0", 0.0, None, None, None, None, Some(5.0), Some(10.0), None, None, None, None, None, None
    /// ).await?;
    ///
    /// // Apply full combination: loss, delay, and duplication
    /// let result = tc_manager.apply_tc_config_in_namespace(
    ///     "test-ns", "eth0", 5.0, Some(10.0), Some(50.0), Some(5.0), Some(20.0), Some(3.0), Some(15.0), None, None, None, None, None, None
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_arguments)] // Legacy method maintained for backward compatibility
    #[instrument(
        skip(self),
        fields(
            namespace,
            interface,
            loss,
            correlation,
            delay_ms,
            delay_jitter_ms,
            delay_correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps
        )
    )]
    pub async fn apply_tc_config_in_namespace(
        &self,
        namespace: &str,
        interface: &str,
        loss: f32,
        correlation: Option<f32>,
        delay_ms: Option<f32>,
        delay_jitter_ms: Option<f32>,
        delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        reorder_correlation: Option<f32>,
        reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> Result<String> {
        info!(
            "Applying TC config: namespace={}, interface={}, loss={}%, correlation={:?}, delay={}ms, jitter={}ms, delay_corr={:?}, duplicate={}%, dup_corr={:?}, reorder={}%, reorder_corr={:?}, gap={:?}, corrupt={}%, corrupt_corr={:?}, rate={}kbps",
            namespace, interface, loss, correlation,
            delay_ms.unwrap_or(0.0), delay_jitter_ms.unwrap_or(0.0), delay_correlation,
            duplicate_percent.unwrap_or(0.0), duplicate_correlation,
            reorder_percent.unwrap_or(0.0), reorder_correlation, reorder_gap,
            corrupt_percent.unwrap_or(0.0), corrupt_correlation,
            rate_limit_kbps.unwrap_or(0)
        );

        // First check if there's already a qdisc on this interface
        match self.check_existing_qdisc(namespace, interface).await {
            Ok(qdisc_info) => {
                if qdisc_info.is_empty() {
                    // No existing qdisc, safe to add
                    info!(
                        "No existing qdisc found on {}/{}, adding new netem qdisc",
                        namespace, interface
                    );
                    self.execute_tc_command(
                        namespace,
                        interface,
                        "add",
                        loss,
                        correlation,
                        delay_ms,
                        delay_jitter_ms,
                        delay_correlation,
                        duplicate_percent,
                        duplicate_correlation,
                        reorder_percent,
                        reorder_correlation,
                        reorder_gap,
                        corrupt_percent,
                        corrupt_correlation,
                        rate_limit_kbps,
                    )
                    .await
                } else {
                    // Existing qdisc found, check if it's netem or something else
                    if qdisc_info.contains("netem") {
                        info!("Existing netem qdisc found on {}/{}, need to determine if we can replace or need fresh recreation", namespace, interface);
                        // Check if we need to remove parameters by comparing current vs requested
                        let current_config = self.parse_current_tc_config(&qdisc_info);
                        let needs_recreation = self.needs_qdisc_recreation(
                            &current_config,
                            loss,
                            correlation,
                            delay_ms,
                            delay_jitter_ms,
                            delay_correlation,
                            duplicate_percent,
                            duplicate_correlation,
                            reorder_percent,
                            reorder_correlation,
                            reorder_gap,
                            corrupt_percent,
                            corrupt_correlation,
                            rate_limit_kbps,
                        );

                        if needs_recreation {
                            info!("Parameters need to be removed, deleting and recreating netem qdisc");
                            match self
                                .remove_tc_config_in_namespace(namespace, interface)
                                .await
                            {
                                Ok(_) => {
                                    self.execute_tc_command(
                                        namespace,
                                        interface,
                                        "add",
                                        loss,
                                        correlation,
                                        delay_ms,
                                        delay_jitter_ms,
                                        delay_correlation,
                                        duplicate_percent,
                                        duplicate_correlation,
                                        reorder_percent,
                                        reorder_correlation,
                                        reorder_gap,
                                        corrupt_percent,
                                        corrupt_correlation,
                                        rate_limit_kbps,
                                    )
                                    .await
                                }
                                Err(e) => {
                                    warn!("Failed to delete existing qdisc: {}, trying replace anyway", e);
                                    self.execute_tc_command(
                                        namespace,
                                        interface,
                                        "replace",
                                        loss,
                                        correlation,
                                        delay_ms,
                                        delay_jitter_ms,
                                        delay_correlation,
                                        duplicate_percent,
                                        duplicate_correlation,
                                        reorder_percent,
                                        reorder_correlation,
                                        reorder_gap,
                                        corrupt_percent,
                                        corrupt_correlation,
                                        rate_limit_kbps,
                                    )
                                    .await
                                }
                            }
                        } else {
                            info!("No parameter removal needed, using replace");
                            self.execute_tc_command(
                                namespace,
                                interface,
                                "replace",
                                loss,
                                correlation,
                                delay_ms,
                                delay_jitter_ms,
                                delay_correlation,
                                duplicate_percent,
                                duplicate_correlation,
                                reorder_percent,
                                reorder_correlation,
                                reorder_gap,
                                corrupt_percent,
                                corrupt_correlation,
                                rate_limit_kbps,
                            )
                            .await
                        }
                    } else if qdisc_info.contains("noqueue") {
                        // noqueue qdisc can be directly replaced with add command
                        info!("Existing noqueue qdisc found on {}/{}, adding netem qdisc (will replace noqueue)", namespace, interface);
                        self.execute_tc_command(
                            namespace,
                            interface,
                            "add",
                            loss,
                            correlation,
                            delay_ms,
                            delay_jitter_ms,
                            delay_correlation,
                            duplicate_percent,
                            duplicate_correlation,
                            reorder_percent,
                            reorder_correlation,
                            reorder_gap,
                            corrupt_percent,
                            corrupt_correlation,
                            rate_limit_kbps,
                        )
                        .await
                    } else {
                        // Other types of qdiscs that might need removal
                        info!("Existing qdisc found on {}/{} ({}), attempting to remove and add netem", namespace, interface, qdisc_info.trim());

                        match self
                            .remove_tc_config_in_namespace(namespace, interface)
                            .await
                        {
                            Ok(_) => {
                                info!("Existing qdisc removed, adding netem qdisc");
                                self.execute_tc_command(
                                    namespace,
                                    interface,
                                    "add",
                                    loss,
                                    correlation,
                                    delay_ms,
                                    delay_jitter_ms,
                                    delay_correlation,
                                    duplicate_percent,
                                    duplicate_correlation,
                                    reorder_percent,
                                    reorder_correlation,
                                    reorder_gap,
                                    corrupt_percent,
                                    corrupt_correlation,
                                    rate_limit_kbps,
                                )
                                .await
                            }
                            Err(remove_error) => {
                                // If removal failed, try add anyway (might work for some qdiscs)
                                warn!("Failed to remove existing qdisc from {}/{}: {}, trying add anyway", namespace, interface, remove_error);
                                info!("Attempting to add netem qdisc despite removal failure");
                                self.execute_tc_command(
                                    namespace,
                                    interface,
                                    "add",
                                    loss,
                                    correlation,
                                    delay_ms,
                                    delay_jitter_ms,
                                    delay_correlation,
                                    duplicate_percent,
                                    duplicate_correlation,
                                    reorder_percent,
                                    reorder_correlation,
                                    reorder_gap,
                                    corrupt_percent,
                                    corrupt_correlation,
                                    rate_limit_kbps,
                                )
                                .await
                            }
                        }
                    }
                }
            }
            Err(check_error) => {
                warn!("Could not check existing qdisc on {}/{}: {}, falling back to add/replace logic", namespace, interface, check_error);

                // Fallback to the old try-add-then-replace logic
                let result = self
                    .execute_tc_command(
                        namespace,
                        interface,
                        "add",
                        loss,
                        correlation,
                        delay_ms,
                        delay_jitter_ms,
                        delay_correlation,
                        duplicate_percent,
                        duplicate_correlation,
                        reorder_percent,
                        reorder_correlation,
                        reorder_gap,
                        corrupt_percent,
                        corrupt_correlation,
                        rate_limit_kbps,
                    )
                    .await;

                match result {
                    Ok(message) => Ok(message),
                    Err(_) => {
                        info!("Add failed, trying replace");
                        self.execute_tc_command(
                            namespace,
                            interface,
                            "replace",
                            loss,
                            correlation,
                            delay_ms,
                            delay_jitter_ms,
                            delay_correlation,
                            duplicate_percent,
                            duplicate_correlation,
                            reorder_percent,
                            reorder_correlation,
                            reorder_gap,
                            corrupt_percent,
                            corrupt_correlation,
                            rate_limit_kbps,
                        )
                        .await
                    }
                }
            }
        }
    }

    /// Execute TC command with specified action (add or replace)
    #[allow(clippy::too_many_arguments)] // Legacy method maintained for backward compatibility
    #[instrument(
        skip(self),
        fields(
            namespace,
            interface,
            action,
            loss,
            correlation,
            delay_ms,
            delay_jitter_ms,
            delay_correlation,
            duplicate_percent,
            duplicate_correlation,
            reorder_percent,
            reorder_correlation,
            reorder_gap,
            corrupt_percent,
            corrupt_correlation,
            rate_limit_kbps
        )
    )]
    async fn execute_tc_command(
        &self,
        namespace: &str,
        interface: &str,
        action: &str, // "add" or "replace"
        loss: f32,
        correlation: Option<f32>,
        delay_ms: Option<f32>,
        delay_jitter_ms: Option<f32>,
        delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        reorder_correlation: Option<f32>,
        reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> Result<String> {
        // Log which TC parameters will be included in the command
        let mut active_params = Vec::new();
        if loss > 0.0 {
            active_params.push(format!("loss={}%", loss));
            if let Some(corr) = correlation {
                if corr > 0.0 {
                    active_params.push(format!("loss_correlation={}%", corr));
                }
            }
        }
        if let Some(delay) = delay_ms {
            if delay > 0.0 {
                active_params.push(format!("delay={}ms", delay));
                if let Some(jitter) = delay_jitter_ms {
                    if jitter > 0.0 {
                        active_params.push(format!("jitter={}ms", jitter));
                        if let Some(delay_corr) = delay_correlation {
                            if delay_corr > 0.0 {
                                active_params.push(format!("delay_correlation={}%", delay_corr));
                            }
                        }
                    }
                }
            }
        }
        if let Some(duplicate) = duplicate_percent {
            if duplicate > 0.0 {
                active_params.push(format!("duplicate={}%", duplicate));
                if let Some(dup_corr) = duplicate_correlation {
                    if dup_corr > 0.0 {
                        active_params.push(format!("duplicate_correlation={}%", dup_corr));
                    }
                }
            }
        }
        if let Some(reorder) = reorder_percent {
            if reorder > 0.0 {
                active_params.push(format!("reorder={}%", reorder));
                if let Some(reorder_corr) = reorder_correlation {
                    if reorder_corr > 0.0 {
                        active_params.push(format!("reorder_correlation={}%", reorder_corr));
                    }
                }
                if let Some(gap) = reorder_gap {
                    if gap > 0 {
                        active_params.push(format!("gap={}", gap));
                    }
                }
            }
        }
        if let Some(corrupt) = corrupt_percent {
            if corrupt > 0.0 {
                active_params.push(format!("corrupt={}%", corrupt));
                if let Some(corrupt_corr) = corrupt_correlation {
                    if corrupt_corr > 0.0 {
                        active_params.push(format!("corrupt_correlation={}%", corrupt_corr));
                    }
                }
            }
        }
        if let Some(rate) = rate_limit_kbps {
            if rate > 0 {
                let rate_display = if rate >= 1000 {
                    format!("{}mbit", rate / 1000)
                } else {
                    format!("{}kbit", rate)
                };
                active_params.push(format!("rate={}", rate_display));
            }
        }

        // If reordering is requested but no delay specified, netem requires a delay. Add a small automatic delay.
        let reorder_requested = reorder_percent.unwrap_or(0.0) > 0.0;
        let delay_specified = delay_ms.unwrap_or(0.0) > 0.0;
        let mut auto_add_delay = false;
        if reorder_requested && !delay_specified {
            active_params.push("delay=1ms(auto)".to_string());
            auto_add_delay = true;
        }

        info!(
            "Active TC parameters for {}/{}: [{}]",
            namespace,
            interface,
            active_params.join(", ")
        );

        // Build base command depending on namespace
        let mut cmd = if namespace == "default" {
            let mut cmd = Command::new("tc");
            cmd.args(["qdisc", action, "dev", interface, "root", "netem"]);
            cmd
        } else {
            let mut cmd = Command::new("ip");
            cmd.args([
                "netns", "exec", namespace, "tc", "qdisc", action, "dev", interface, "root",
                "netem",
            ]);
            cmd
        };

        // Add loss parameters if loss > 0
        if loss > 0.0 {
            // Use "loss random PERCENT [CORRELATION]" syntax
            cmd.args(["loss", "random", &format!("{}%", loss)]);

            // Add loss correlation if specified (directly after the percentage)
            if let Some(corr) = correlation {
                if corr > 0.0 {
                    cmd.args([&format!("{}%", corr)]);
                }
            }
        }

        // Track whether we've already added a delay (required for reordering)
        let mut has_delay = false;

        // Add delay parameters if delay is specified
        if let Some(delay) = delay_ms {
            if delay > 0.0 {
                cmd.args(["delay", &format!("{}ms", delay)]);
                has_delay = true;

                // Add delay jitter if specified
                if let Some(jitter) = delay_jitter_ms {
                    if jitter > 0.0 {
                        cmd.args([&format!("{}ms", jitter)]);

                        // Add delay correlation if specified (only valid with jitter)
                        if let Some(delay_corr) = delay_correlation {
                            if delay_corr > 0.0 {
                                cmd.args([&format!("{}%", delay_corr)]);
                            }
                        }
                    }
                }
            }
        }

        // If we need reordering but no delay was provided, add a minimal delay automatically
        if auto_add_delay && !has_delay {
            cmd.args(["delay", "1ms"]);
            has_delay = true;
        }

        // Add duplication parameters if duplication is specified
        if let Some(duplicate) = duplicate_percent {
            if duplicate > 0.0 {
                cmd.args(["duplicate", &format!("{}%", duplicate)]);

                // Add duplication correlation if specified
                if let Some(dup_corr) = duplicate_correlation {
                    if dup_corr > 0.0 {
                        cmd.args([&format!("{}%", dup_corr)]);
                    }
                }
            }
        }

        // Add reordering parameters if reordering is specified
        if let Some(reorder) = reorder_percent {
            if reorder > 0.0 {
                // Ensure delay is present (netem requires some delay for reorder)
                if !has_delay {
                    cmd.args(["delay", "1ms"]);
                }
                cmd.args(["reorder", &format!("{}%", reorder)]);

                // Add reordering correlation if specified
                if let Some(reorder_corr) = reorder_correlation {
                    if reorder_corr > 0.0 {
                        cmd.args([&format!("{}%", reorder_corr)]);
                    }
                }

                // Add reordering gap if specified
                if let Some(gap) = reorder_gap {
                    if gap > 0 {
                        cmd.args(["gap", &format!("{}", gap)]);
                    }
                }
            }
        }

        // Add corruption parameters if corruption is specified
        if let Some(corrupt) = corrupt_percent {
            if corrupt > 0.0 {
                cmd.args(["corrupt", &format!("{}%", corrupt)]);

                // Add corruption correlation if specified
                if let Some(corrupt_corr) = corrupt_correlation {
                    if corrupt_corr > 0.0 {
                        cmd.args([&format!("{}%", corrupt_corr)]);
                    }
                }
            }
        }

        // Add rate limiting parameters if rate limiting is specified
        if let Some(rate) = rate_limit_kbps {
            if rate > 0 {
                // Convert kbps to appropriate unit for tc netem rate parameter
                if rate >= 1000 {
                    cmd.args(["rate", &format!("{}mbit", rate / 1000)]);
                } else {
                    cmd.args(["rate", &format!("{}kbit", rate)]);
                }
            }
        }

        info!("Executing TC {} command: {:?}", action, cmd);

        let output = cmd.output().await.map_err(|e| TcguiError::TcCommandError {
            message: format!("Failed to execute TC {} command: {}", action, e),
        })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            info!(
                "TC {} command completed successfully. stdout: '{}'",
                action,
                stdout.trim()
            );
            Ok(format!("TC config {} successfully: {}", action, stdout))
        } else {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            error!("TC {} command failed with exit code {}", action, exit_code);
            error!("  stdout: '{}'", stdout.trim());
            error!("  stderr: '{}'", stderr.trim());

            Err(TcguiError::TcCommandError {
                message: format!(
                    "TC {} command failed (exit {}): stderr='{}', stdout='{}'",
                    action,
                    exit_code,
                    stderr.trim(),
                    stdout.trim()
                ),
            }
            .into())
        }
    }

    /// Remove TC config in default namespace (legacy method)
    #[allow(dead_code)] // Keep for backward compatibility
    pub async fn remove_tc_config(&self, interface: &str) -> Result<String> {
        self.remove_tc_config_in_namespace("default", interface)
            .await
    }

    /// Removes traffic control configuration from an interface in a specific namespace.
    ///
    /// This method removes all netem qdisc configurations from the specified interface
    /// within the given namespace. It gracefully handles cases where no TC configuration
    /// exists on the interface.
    ///
    /// # Arguments
    ///
    /// * `namespace` - Target namespace ("default" for host namespace)
    /// * `interface` - Network interface name (e.g., "eth0", "fo")
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - Success message (including "no config to remove" cases)
    /// * `Err` - On command execution failures or system errors
    ///
    /// # Commands Generated
    ///
    /// * **Default namespace**: `tc qdisc del dev <interface> root`
    /// * **Named namespace**: `ip netns exec <namespace> tc qdisc del dev <interface> root`
    ///
    /// # Error Handling
    ///
    /// * **No qdisc present**: Returns success message "No TC config to remove"
    /// * **System errors**: Reports detailed error information for debugging
    /// * **Permission issues**: Captures and reports stderr output
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tcgui_backend::tc_commands::TcCommandManager;
    /// # async fn example() -> anyhow::Result<()> {
    /// # let tc_manager = TcCommandManager::new();
    /// // Remove TC config from default namespace interface
    /// let result = tc_manager.remove_tc_config_in_namespace(
    ///     "default", "eth0"
    /// ).await?;
    ///
    /// // Remove TC config from named namespace interface  
    /// let result = tc_manager.remove_tc_config_in_namespace(
    ///     "test-ns", "eth0"
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn remove_tc_config_in_namespace(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<String> {
        info!(
            "Removing TC config for interface: {} in namespace: {}",
            interface, namespace
        );

        let mut cmd = if namespace == "default" {
            let mut cmd = Command::new("tc");
            cmd.args(["qdisc", "del", "dev", interface, "root"]);
            cmd
        } else {
            let mut cmd = Command::new("ip");
            cmd.args([
                "netns", "exec", namespace, "tc", "qdisc", "del", "dev", interface, "root",
            ]);
            cmd
        };

        info!("Executing command: {:?}", cmd);

        let output = cmd.output().await.map_err(|e| TcguiError::TcCommandError {
            message: format!("Failed to execute TC command: {}", e),
        })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(format!("TC config removed successfully: {}", stdout))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // It's normal for this to fail if there's no qdisc
            if stderr.contains("RTNETLINK answers: No such file or directory") {
                Ok("No TC config to remove".to_string())
            } else {
                Err(TcguiError::TcCommandError {
                    message: format!("TC command failed: {}", stderr),
                }
                .into())
            }
        }
    }

    // Note: TC results are now handled via query/reply pattern in main.rs
    // These methods are no longer needed as TC operations use direct query responses

    /// Parse current TC configuration from qdisc info string
    fn parse_current_tc_config(&self, qdisc_info: &str) -> CurrentTcConfig {
        CurrentTcConfig {
            has_loss: qdisc_info.contains("loss"),
            has_delay: qdisc_info.contains("delay"),
            has_duplicate: qdisc_info.contains("duplicate"),
            has_reorder: qdisc_info.contains("reorder"),
            has_corrupt: qdisc_info.contains("corrupt"),
            has_rate: qdisc_info.contains("rate"),
        }
    }

    /// Determine if we need to recreate the qdisc (delete + add) vs just replace
    /// This is needed when we want to remove parameters that TC netem preserves on replace
    #[allow(clippy::too_many_arguments)]
    fn needs_qdisc_recreation(
        &self,
        current: &CurrentTcConfig,
        loss: f32,
        _correlation: Option<f32>,
        delay_ms: Option<f32>,
        _delay_jitter_ms: Option<f32>,
        _delay_correlation: Option<f32>,
        duplicate_percent: Option<f32>,
        _duplicate_correlation: Option<f32>,
        reorder_percent: Option<f32>,
        _reorder_correlation: Option<f32>,
        _reorder_gap: Option<u32>,
        corrupt_percent: Option<f32>,
        _corrupt_correlation: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> bool {
        // Check if any currently active parameters need to be removed
        let will_remove_loss = current.has_loss && loss <= 0.0;
        let will_remove_delay = current.has_delay && delay_ms.is_none_or(|d| d <= 0.0);
        let will_remove_duplicate =
            current.has_duplicate && duplicate_percent.is_none_or(|d| d <= 0.0);
        let will_remove_reorder = current.has_reorder && reorder_percent.is_none_or(|r| r <= 0.0);
        let will_remove_corrupt = current.has_corrupt && corrupt_percent.is_none_or(|c| c <= 0.0);
        let will_remove_rate = current.has_rate && rate_limit_kbps.is_none_or(|r| r == 0);

        let needs_recreation = will_remove_loss
            || will_remove_delay
            || will_remove_duplicate
            || will_remove_reorder
            || will_remove_corrupt
            || will_remove_rate;

        if needs_recreation {
            info!("Qdisc recreation needed: loss_removal={}, delay_removal={}, duplicate_removal={}, reorder_removal={}, corrupt_removal={}, rate_removal={}", 
                will_remove_loss, will_remove_delay, will_remove_duplicate, will_remove_reorder, will_remove_corrupt, will_remove_rate);
        }

        needs_recreation
    }
}

/// Simple structure to track which TC parameters are currently active
#[derive(Debug)]
struct CurrentTcConfig {
    has_loss: bool,
    has_delay: bool,
    has_duplicate: bool,
    has_reorder: bool,
    has_corrupt: bool,
    has_rate: bool,
}
