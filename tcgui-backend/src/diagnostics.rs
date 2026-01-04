//! Network diagnostics service for TC GUI.
//!
//! This module provides network diagnostic capabilities including:
//! - Link status checking (up/down, carrier, MTU)
//! - Connectivity testing via ping
//! - Latency measurement
//! - Current TC configuration retrieval

use crate::network::NetworkManager;
use crate::tc_commands::TcCommandManager;
use nlink::netlink::namespace;
use nlink::netlink::{Connection, Route};
use std::process::Stdio;

use std::time::Duration;
use tcgui_shared::{
    ConnectivityResult, DiagnosticsRequest, DiagnosticsResponse, DiagnosticsResults, LatencyResult,
    LinkStatus, TcCorruptConfig, TcDelayConfig, TcDiagnosticStats, TcDuplicateConfig, TcLossConfig,
    TcNetemConfig, TcRateLimitConfig, TcReorderConfig,
};
use tokio::process::Command;
use tracing::{debug, info, instrument};

/// Service for running network diagnostics on interfaces.
pub struct DiagnosticsService<'a> {
    #[allow(dead_code)]
    network_manager: &'a NetworkManager,
    tc_manager: &'a TcCommandManager,
}

impl<'a> DiagnosticsService<'a> {
    /// Create a new diagnostics service.
    pub fn new(network_manager: &'a NetworkManager, tc_manager: &'a TcCommandManager) -> Self {
        Self {
            network_manager,
            tc_manager,
        }
    }

    /// Run diagnostics on a network interface.
    #[instrument(skip(self), fields(namespace, interface))]
    pub async fn run_diagnostics(
        &self,
        request: &DiagnosticsRequest,
    ) -> Result<DiagnosticsResponse, String> {
        info!(
            "Running diagnostics on {}/{}",
            request.namespace, request.interface
        );

        let mut results = DiagnosticsResults::default();

        // Step 1: Check link status
        match self
            .check_link_status(&request.namespace, &request.interface)
            .await
        {
            Ok(status) => {
                results.link_status = status;
            }
            Err(e) => {
                return Ok(DiagnosticsResponse {
                    success: false,
                    message: format!("Failed to check link status: {}", e),
                    results,
                    error_code: Some(-1),
                });
            }
        }

        // Step 2: Get current TC configuration
        results.configured_tc = self
            .get_current_tc_config(&request.namespace, &request.interface)
            .await
            .ok()
            .flatten();

        // Step 2b: Get TC statistics if netem is configured
        results.tc_stats = self
            .get_tc_diagnostic_stats(&request.namespace, &request.interface)
            .await;

        // Step 3: Detect target for connectivity tests
        let target = match &request.target {
            Some(t) => t.clone(),
            None => self
                .detect_target(&request.namespace, &request.interface)
                .await
                .unwrap_or_else(|| "8.8.8.8".to_string()),
        };

        // Step 4: Run connectivity and latency tests (only if link is up)
        if results.link_status.is_up && results.link_status.has_carrier {
            let timeout_secs = (request.timeout_ms / 1000).max(1);

            // Run ping test for connectivity and latency
            match self
                .run_ping_test(
                    &request.namespace,
                    &request.interface,
                    &target,
                    5, // 5 samples
                    timeout_secs,
                )
                .await
            {
                Ok((connectivity, latency)) => {
                    results.connectivity = Some(connectivity);
                    results.latency = latency;
                }
                Err(e) => {
                    debug!("Ping test failed: {}", e);
                    results.connectivity = Some(ConnectivityResult {
                        target: target.clone(),
                        reachable: false,
                        method: "ping".to_string(),
                    });
                }
            }
        } else {
            results.connectivity = Some(ConnectivityResult {
                target: target.clone(),
                reachable: false,
                method: "skipped (link down)".to_string(),
            });
        }

        // Build response message
        let message = self.build_summary_message(&results);

        Ok(DiagnosticsResponse {
            success: true,
            message,
            results,
            error_code: None,
        })
    }

    /// Check link status for an interface.
    async fn check_link_status(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<LinkStatus, String> {
        use nlink::Connection;
        use nlink::netlink::Route;

        let conn = if namespace == "default" {
            Connection::<Route>::new()
                .map_err(|e| format!("Failed to create netlink connection: {}", e))?
        } else {
            namespace::connection_for(namespace)
                .map_err(|e| format!("Failed to connect to namespace {}: {}", namespace, e))?
        };

        let links = conn
            .get_links()
            .await
            .map_err(|e| format!("Failed to get links: {}", e))?;

        for link in &links {
            let name = link.name_or("").to_string();
            if name == interface {
                return Ok(LinkStatus {
                    is_up: link.is_up(),
                    has_carrier: link.has_carrier(),
                    mtu: link.mtu().unwrap_or(1500),
                });
            }
        }

        Err(format!("Interface {} not found", interface))
    }

    /// Get the current TC netem configuration for an interface.
    async fn get_current_tc_config(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<Option<TcNetemConfig>, String> {
        let netem_opts = self
            .tc_manager
            .get_netem_options(namespace, interface)
            .await
            .map_err(|e| format!("Failed to get TC config: {}", e))?;

        match netem_opts {
            Some(opts) => {
                // Convert nlink NetemOptions to TcNetemConfig
                let config = TcNetemConfig {
                    loss: TcLossConfig {
                        enabled: opts.loss().unwrap_or(0.0) > 0.0,
                        percentage: opts.loss().unwrap_or(0.0) as f32,
                        correlation: opts.loss_correlation().unwrap_or(0.0) as f32,
                    },
                    delay: TcDelayConfig {
                        enabled: opts.delay().map(|d| d.as_millis() > 0).unwrap_or(false),
                        base_ms: opts.delay().map(|d| d.as_millis() as f32).unwrap_or(0.0),
                        jitter_ms: opts.jitter().map(|d| d.as_millis() as f32).unwrap_or(0.0),
                        correlation: opts.delay_correlation().unwrap_or(0.0) as f32,
                    },
                    duplicate: TcDuplicateConfig {
                        enabled: opts.duplicate().unwrap_or(0.0) > 0.0,
                        percentage: opts.duplicate().unwrap_or(0.0) as f32,
                        correlation: opts.duplicate_correlation().unwrap_or(0.0) as f32,
                    },
                    reorder: TcReorderConfig {
                        enabled: opts.reorder().unwrap_or(0.0) > 0.0,
                        percentage: opts.reorder().unwrap_or(0.0) as f32,
                        correlation: opts.reorder_correlation().unwrap_or(0.0) as f32,
                        gap: opts.gap().unwrap_or(5),
                    },
                    corrupt: TcCorruptConfig {
                        enabled: opts.corrupt().unwrap_or(0.0) > 0.0,
                        percentage: opts.corrupt().unwrap_or(0.0) as f32,
                        correlation: opts.corrupt_correlation().unwrap_or(0.0) as f32,
                    },
                    rate_limit: TcRateLimitConfig {
                        enabled: opts.rate_bps().map(|r| r > 0).unwrap_or(false),
                        rate_kbps: opts.rate_bps().map(|r| (r / 1000) as u32).unwrap_or(0),
                    },
                };
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    /// Get TC diagnostic statistics for an interface (drops, overlimits, etc.).
    async fn get_tc_diagnostic_stats(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Option<TcDiagnosticStats> {
        match self
            .tc_manager
            .get_tc_statistics(namespace, interface)
            .await
        {
            Ok(Some(stats)) => Some(TcDiagnosticStats {
                drops: stats.queue.drops,
                overlimits: stats.queue.overlimits,
                qlen: stats.queue.qlen,
                backlog: stats.queue.backlog,
                bps: stats.rate_est.map(|r| r.bps),
                pps: stats.rate_est.map(|r| r.pps),
            }),
            Ok(None) => None, // No netem configured
            Err(e) => {
                debug!(
                    "Failed to get TC stats for {}/{}: {}",
                    namespace, interface, e
                );
                None
            }
        }
    }

    /// Detect a reasonable target for ping tests.
    async fn detect_target(&self, namespace: &str, _interface: &str) -> Option<String> {
        // Try to get the default gateway
        if let Some(gateway) = self.get_default_gateway(namespace).await {
            return Some(gateway);
        }

        // Fallback to a well-known DNS server
        Some("8.8.8.8".to_string())
    }

    /// Get the default gateway for a namespace using nlink's route query API.
    async fn get_default_gateway(&self, ns: &str) -> Option<String> {
        let conn = if ns == "default" {
            Connection::<Route>::new().ok()?
        } else {
            namespace::connection_for(ns).ok()?
        };

        let routes = conn.get_routes().await.ok()?;

        // Find IPv4 default route (dst_len == 0 means 0.0.0.0/0)
        routes
            .iter()
            .find(|r| r.dst_len() == 0 && r.is_ipv4())
            .and_then(|r| r.gateway())
            .map(|ip| ip.to_string())
    }

    /// Run a ping test and parse results.
    async fn run_ping_test(
        &self,
        namespace: &str,
        interface: &str,
        target: &str,
        count: u32,
        timeout_secs: u32,
    ) -> Result<(ConnectivityResult, Option<LatencyResult>), String> {
        let timeout = Duration::from_secs(timeout_secs as u64 + count as u64);

        let output = if namespace == "default" {
            tokio::time::timeout(
                timeout,
                Command::new("ping")
                    .args([
                        "-c",
                        &count.to_string(),
                        "-W",
                        &timeout_secs.to_string(),
                        "-I",
                        interface,
                        target,
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output(),
            )
            .await
            .map_err(|_| "Ping timed out".to_string())?
            .map_err(|e| format!("Failed to run ping: {}", e))?
        } else {
            tokio::time::timeout(
                timeout,
                Command::new("ip")
                    .args([
                        "netns",
                        "exec",
                        namespace,
                        "ping",
                        "-c",
                        &count.to_string(),
                        "-W",
                        &timeout_secs.to_string(),
                        "-I",
                        interface,
                        target,
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output(),
            )
            .await
            .map_err(|_| "Ping timed out".to_string())?
            .map_err(|e| format!("Failed to run ping: {}", e))?
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let reachable = output.status.success();

        let connectivity = ConnectivityResult {
            target: target.to_string(),
            reachable,
            method: "ping".to_string(),
        };

        // Parse latency statistics if ping succeeded
        let latency = if reachable {
            self.parse_ping_output(&stdout, target, count)
        } else {
            None
        };

        Ok((connectivity, latency))
    }

    /// Parse ping output to extract latency statistics.
    fn parse_ping_output(&self, output: &str, target: &str, samples: u32) -> Option<LatencyResult> {
        // Look for the statistics line: "rtt min/avg/max/mdev = 0.123/0.456/0.789/0.111 ms"
        for line in output.lines() {
            if line.contains("rtt min/avg/max") || line.contains("round-trip min/avg/max") {
                // Parse "rtt min/avg/max/mdev = 0.123/0.456/0.789/0.111 ms"
                if let Some(stats_part) = line.split('=').nth(1) {
                    let stats_str = stats_part.split_whitespace().next()?;
                    let parts: Vec<&str> = stats_str.split('/').collect();
                    if parts.len() >= 3 {
                        let min_ms = parts[0].parse::<f32>().ok()?;
                        let avg_ms = parts[1].parse::<f32>().ok()?;
                        let max_ms = parts[2].parse::<f32>().ok()?;

                        // Parse packet loss from another line
                        let packet_loss = self.parse_packet_loss(output);

                        return Some(LatencyResult {
                            target: target.to_string(),
                            min_ms,
                            avg_ms,
                            max_ms,
                            packet_loss_percent: packet_loss,
                            samples,
                        });
                    }
                }
            }
        }

        None
    }

    /// Parse packet loss percentage from ping output.
    fn parse_packet_loss(&self, output: &str) -> f32 {
        // Look for "X% packet loss" or "X.X% packet loss"
        for line in output.lines() {
            if line.contains("packet loss") {
                // Find the percentage
                for word in line.split_whitespace() {
                    if let Some(pct_str) = word.strip_suffix('%')
                        && let Ok(pct) = pct_str.parse::<f32>()
                    {
                        return pct;
                    }
                }
            }
        }
        0.0
    }

    /// Build a summary message for the diagnostics results.
    fn build_summary_message(&self, results: &DiagnosticsResults) -> String {
        let mut parts = Vec::new();

        // Link status
        let link_status = if results.link_status.is_up && results.link_status.has_carrier {
            "Link UP"
        } else if results.link_status.is_up {
            "Link UP (no carrier)"
        } else {
            "Link DOWN"
        };
        parts.push(link_status.to_string());

        // Connectivity
        if let Some(ref conn) = results.connectivity {
            if conn.reachable {
                parts.push(format!("{} reachable", conn.target));
            } else {
                parts.push(format!("{} unreachable", conn.target));
            }
        }

        // Latency summary
        if let Some(ref lat) = results.latency {
            parts.push(format!("avg latency: {:.1}ms", lat.avg_ms));
            if lat.packet_loss_percent > 0.0 {
                parts.push(format!("{:.1}% loss", lat.packet_loss_percent));
            }
        }

        // TC status
        if results.configured_tc.is_some() {
            parts.push("TC active".to_string());
        }

        // TC stats summary (drops and overlimits)
        if let Some(ref tc_stats) = results.tc_stats
            && (tc_stats.drops > 0 || tc_stats.overlimits > 0)
        {
            parts.push(format!(
                "TC: {} drops, {} overlimits",
                tc_stats.drops, tc_stats.overlimits
            ));
        }

        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to parse ping output without needing full service
    fn parse_ping_stats(output: &str, target: &str, samples: u32) -> Option<LatencyResult> {
        // Look for the statistics line: "rtt min/avg/max/mdev = 0.123/0.456/0.789/0.111 ms"
        for line in output.lines() {
            let is_stats_line =
                line.contains("rtt min/avg/max") || line.contains("round-trip min/avg/max");
            if !is_stats_line {
                continue;
            }

            let Some(stats_part) = line.split('=').nth(1) else {
                continue;
            };

            let stats_str = stats_part.split_whitespace().next()?;
            let parts: Vec<&str> = stats_str.split('/').collect();
            if parts.len() >= 3 {
                let min_ms = parts[0].parse::<f32>().ok()?;
                let avg_ms = parts[1].parse::<f32>().ok()?;
                let max_ms = parts[2].parse::<f32>().ok()?;

                // Parse packet loss
                let packet_loss = parse_loss_from_output(output);

                return Some(LatencyResult {
                    target: target.to_string(),
                    min_ms,
                    avg_ms,
                    max_ms,
                    packet_loss_percent: packet_loss,
                    samples,
                });
            }
        }
        None
    }

    fn parse_loss_from_output(output: &str) -> f32 {
        for line in output.lines() {
            if line.contains("packet loss") {
                for word in line.split_whitespace() {
                    if let Some(pct_str) = word.strip_suffix('%')
                        && let Ok(pct) = pct_str.parse::<f32>()
                    {
                        return pct;
                    }
                }
            }
        }
        0.0
    }

    #[test]
    fn test_parse_ping_output() {
        let output = r#"PING 8.8.8.8 (8.8.8.8) 56(84) bytes of data.
64 bytes from 8.8.8.8: icmp_seq=1 ttl=117 time=12.3 ms
64 bytes from 8.8.8.8: icmp_seq=2 ttl=117 time=11.8 ms
64 bytes from 8.8.8.8: icmp_seq=3 ttl=117 time=12.1 ms

--- 8.8.8.8 ping statistics ---
3 packets transmitted, 3 received, 0% packet loss, time 2003ms
rtt min/avg/max/mdev = 11.800/12.067/12.300/0.205 ms"#;

        let result = parse_ping_stats(output, "8.8.8.8", 3);
        assert!(result.is_some());
        let lat = result.unwrap();
        assert!((lat.min_ms - 11.8).abs() < 0.01);
        assert!((lat.avg_ms - 12.067).abs() < 0.01);
        assert!((lat.max_ms - 12.3).abs() < 0.01);
        assert!((lat.packet_loss_percent - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_packet_loss() {
        let output = "3 packets transmitted, 2 received, 33.33% packet loss, time 2003ms";
        let loss = parse_loss_from_output(output);
        assert!((loss - 33.33).abs() < 0.01);
    }

    #[test]
    fn test_parse_packet_loss_integer() {
        let output = "5 packets transmitted, 4 received, 20% packet loss, time 4000ms";
        let loss = parse_loss_from_output(output);
        assert!((loss - 20.0).abs() < 0.01);
    }
}
