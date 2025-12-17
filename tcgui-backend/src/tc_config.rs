//! TC configuration parsing and command building utilities.
//!
//! This module provides functions for:
//! - Parsing TC parameters from qdisc output strings
//! - Building TC command strings for display
//! - Converting between configuration formats

use tcgui_shared::TcConfiguration;
use tracing::info;

/// Parse TC parameters from a qdisc info string (e.g., from `tc qdisc show`).
///
/// # Arguments
/// * `qdisc_info` - Raw output from tc qdisc show command
///
/// # Returns
/// A `TcConfiguration` with parsed values. Unparseable fields default to 0/None.
pub fn parse_tc_parameters(qdisc_info: &str) -> TcConfiguration {
    let mut config = TcConfiguration {
        loss: 0.0,
        correlation: None,
        delay_ms: None,
        delay_jitter_ms: None,
        delay_correlation: None,
        duplicate_percent: None,
        duplicate_correlation: None,
        reorder_percent: None,
        reorder_correlation: None,
        reorder_gap: None,
        corrupt_percent: None,
        corrupt_correlation: None,
        rate_limit_kbps: None,
        command: format!("# Detected: {}", qdisc_info.trim()),
    };

    // Parse loss percentage
    if let Some(loss_start) = qdisc_info.find("loss ") {
        let loss_part = &qdisc_info[loss_start + 5..];
        if let Some(percent_pos) = loss_part.find('%') {
            if let Ok(loss_val) = loss_part[..percent_pos].trim().parse::<f32>() {
                config.loss = loss_val;
                info!("Parsed loss: {}%", loss_val);
            }
        }
    }

    // Parse delay (format: "delay 100ms 10ms 25%" for delay, jitter, correlation)
    // Also handle seconds format: "delay 2.95s"
    if let Some(delay_start) = qdisc_info.find("delay ") {
        let delay_part = &qdisc_info[delay_start + 6..];
        let delay_tokens: Vec<&str> = delay_part.split_whitespace().collect();

        // Parse base delay (first token)
        if !delay_tokens.is_empty() {
            let first_token = delay_tokens[0];

            // Handle milliseconds (ms)
            if first_token.ends_with("ms") {
                let delay_str = first_token.trim_end_matches("ms");
                if let Ok(delay_val) = delay_str.parse::<f32>() {
                    config.delay_ms = Some(delay_val);
                    info!("Parsed delay: {}ms", delay_val);
                }
            }
            // Handle seconds (s) - convert to milliseconds
            else if first_token.ends_with('s') && !first_token.ends_with("ms") {
                let delay_str = first_token.trim_end_matches('s');
                if let Ok(delay_val) = delay_str.parse::<f32>() {
                    let delay_ms = delay_val * 1000.0;
                    config.delay_ms = Some(delay_ms);
                    info!("Parsed delay: {}s ({}ms)", delay_val, delay_ms);
                }
            }

            // Parse jitter (second token if it ends with ms)
            if delay_tokens.len() > 1 && delay_tokens[1].ends_with("ms") {
                let jitter_str = delay_tokens[1].trim_end_matches("ms");
                if let Ok(jitter_val) = jitter_str.parse::<f32>() {
                    config.delay_jitter_ms = Some(jitter_val);
                    info!("Parsed delay jitter: {}ms", jitter_val);
                }

                // Parse correlation (third token if it ends with %)
                if delay_tokens.len() > 2 && delay_tokens[2].ends_with('%') {
                    let corr_str = delay_tokens[2].trim_end_matches('%');
                    if let Ok(corr_val) = corr_str.parse::<f32>() {
                        config.delay_correlation = Some(corr_val);
                        info!("Parsed delay correlation: {}%", corr_val);
                    }
                }
            }
        }
    }

    // Parse duplicate percentage
    if let Some(dup_start) = qdisc_info.find("duplicate ") {
        let dup_part = &qdisc_info[dup_start + 10..];
        if let Some(percent_pos) = dup_part.find('%') {
            if let Ok(dup_val) = dup_part[..percent_pos].trim().parse::<f32>() {
                config.duplicate_percent = Some(dup_val);
                info!("Parsed duplicate: {}%", dup_val);
            }
        }
    }

    // Parse reorder percentage
    if let Some(reorder_start) = qdisc_info.find("reorder ") {
        let reorder_part = &qdisc_info[reorder_start + 8..];
        if let Some(percent_pos) = reorder_part.find('%') {
            if let Ok(reorder_val) = reorder_part[..percent_pos].trim().parse::<f32>() {
                config.reorder_percent = Some(reorder_val);
                info!("Parsed reorder: {}%", reorder_val);
            }
        }

        // Parse reorder gap
        if let Some(gap_start) = qdisc_info.find("gap ") {
            let gap_part = &qdisc_info[gap_start + 4..];
            let gap_end = gap_part.find(' ').unwrap_or(gap_part.len());
            if let Ok(gap_val) = gap_part[..gap_end].trim().parse::<u32>() {
                config.reorder_gap = Some(gap_val);
                info!("Parsed reorder gap: {}", gap_val);
            }
        }
    }

    // Parse corrupt percentage
    if let Some(corrupt_start) = qdisc_info.find("corrupt ") {
        let corrupt_part = &qdisc_info[corrupt_start + 8..];
        if let Some(percent_pos) = corrupt_part.find('%') {
            if let Ok(corrupt_val) = corrupt_part[..percent_pos].trim().parse::<f32>() {
                config.corrupt_percent = Some(corrupt_val);
                info!("Parsed corrupt: {}%", corrupt_val);
            }
        }
    }

    // Parse rate limiting (can be in kbit, Kbit, mbit, Mbit)
    if let Some(rate_start) = qdisc_info.find("rate ") {
        let rate_part = &qdisc_info[rate_start + 5..];
        let rate_part_lower = rate_part.to_lowercase();

        if let Some(kbit_pos) = rate_part_lower.find("kbit") {
            if let Ok(rate_val) = rate_part[..kbit_pos].trim().parse::<u32>() {
                config.rate_limit_kbps = Some(rate_val);
                info!("Parsed rate limit: {}kbps", rate_val);
            }
        } else if let Some(mbit_pos) = rate_part_lower.find("mbit") {
            if let Ok(rate_val) = rate_part[..mbit_pos].trim().parse::<u32>() {
                config.rate_limit_kbps = Some(rate_val * 1000);
                info!(
                    "Parsed rate limit: {}mbit ({}kbps)",
                    rate_val,
                    rate_val * 1000
                );
            }
        }
    }

    config
}

/// Build a TC command string for display from configuration parameters.
///
/// This generates a human-readable command string showing what TC configuration
/// would be applied (useful for logging and UI display).
#[allow(dead_code)] // Will be used when handlers are refactored
#[allow(clippy::too_many_arguments)]
pub fn build_tc_command_string(
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
) -> String {
    let mut cmd_parts = vec![format!("tc qdisc replace dev {} root netem", interface)];

    if loss > 0.0 {
        let loss_part = if let Some(corr) = correlation {
            if corr > 0.0 {
                format!("loss {}% correlation {}%", loss, corr)
            } else {
                format!("loss {}%", loss)
            }
        } else {
            format!("loss {}%", loss)
        };
        cmd_parts.push(loss_part);
    }

    if let Some(delay) = delay_ms {
        if delay > 0.0 {
            let mut delay_part = format!("delay {}ms", delay);
            if let Some(jitter) = delay_jitter_ms {
                if jitter > 0.0 {
                    delay_part.push_str(&format!(" {}ms", jitter));
                    if let Some(delay_corr) = delay_correlation {
                        if delay_corr > 0.0 {
                            delay_part.push_str(&format!(" {}%", delay_corr));
                        }
                    }
                }
            }
            cmd_parts.push(delay_part);
        }
    }

    if let Some(duplicate) = duplicate_percent {
        if duplicate > 0.0 {
            let mut duplicate_part = format!("duplicate {}%", duplicate);
            if let Some(dup_corr) = duplicate_correlation {
                if dup_corr > 0.0 {
                    duplicate_part.push_str(&format!(" {}%", dup_corr));
                }
            }
            cmd_parts.push(duplicate_part);
        }
    }

    if let Some(reorder) = reorder_percent {
        if reorder > 0.0 {
            let mut reorder_part = format!("reorder {}%", reorder);
            if let Some(reorder_corr) = reorder_correlation {
                if reorder_corr > 0.0 {
                    reorder_part.push_str(&format!(" {}%", reorder_corr));
                }
            }
            if let Some(gap) = reorder_gap {
                if gap > 0 {
                    reorder_part.push_str(&format!(" gap {}", gap));
                }
            }
            cmd_parts.push(reorder_part);
        }
    }

    if let Some(corrupt) = corrupt_percent {
        if corrupt > 0.0 {
            let mut corrupt_part = format!("corrupt {}%", corrupt);
            if let Some(corrupt_corr) = corrupt_correlation {
                if corrupt_corr > 0.0 {
                    corrupt_part.push_str(&format!(" {}%", corrupt_corr));
                }
            }
            cmd_parts.push(corrupt_part);
        }
    }

    if let Some(rate) = rate_limit_kbps {
        if rate > 0 {
            let rate_part = if rate >= 1000 {
                format!("rate {}mbit", rate / 1000)
            } else {
                format!("rate {}kbit", rate)
            };
            cmd_parts.push(rate_part);
        }
    }

    cmd_parts.join(" ")
}

/// Build a TcConfiguration from individual parameters.
///
/// This is a convenience function that combines parameter values with a
/// generated command string.
#[allow(dead_code)] // Will be used when handlers are refactored
#[allow(clippy::too_many_arguments)]
pub fn build_tc_configuration(
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
) -> TcConfiguration {
    let command = build_tc_command_string(
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
    );

    TcConfiguration {
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
        command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tc_basic_parameters() {
        let qdisc_info = "qdisc netem 802d: root refcnt 2 limit 1000 delay 1ms reorder 25% 50% corrupt 15% rate 100Kbit seed 6860218008241482725 gap 1";

        let config = parse_tc_parameters(qdisc_info);

        assert_eq!(config.delay_ms, Some(1.0), "Should parse delay 1ms");
        assert_eq!(
            config.reorder_percent,
            Some(25.0),
            "Should parse reorder 25%"
        );
        assert_eq!(config.reorder_gap, Some(1), "Should parse gap 1");
        assert_eq!(
            config.corrupt_percent,
            Some(15.0),
            "Should parse corrupt 15%"
        );
        assert_eq!(
            config.rate_limit_kbps,
            Some(100),
            "Should parse rate 100Kbit as 100 kbps"
        );
        assert_eq!(config.loss, 0.0, "Should not have loss");
        assert_eq!(config.duplicate_percent, None, "Should not have duplicate");
    }

    #[test]
    fn test_parse_tc_loss_and_duplicate() {
        let qdisc_info = "qdisc netem 8030: root refcnt 2 limit 1000 loss 5.5% duplicate 10.2%";

        let config = parse_tc_parameters(qdisc_info);

        assert_eq!(config.loss, 5.5, "Should parse loss 5.5%");
        assert_eq!(
            config.duplicate_percent,
            Some(10.2),
            "Should parse duplicate 10.2%"
        );
        assert_eq!(config.delay_ms, None, "Should not have delay");
    }

    #[test]
    fn test_parse_tc_complex_delay() {
        let qdisc_info = "qdisc netem 8031: root refcnt 2 limit 1000 delay 100ms 10ms 25%";

        let config = parse_tc_parameters(qdisc_info);

        assert_eq!(config.delay_ms, Some(100.0), "Should parse delay 100ms");
        assert_eq!(
            config.delay_jitter_ms,
            Some(10.0),
            "Should parse jitter 10ms"
        );
        assert_eq!(
            config.delay_correlation,
            Some(25.0),
            "Should parse delay correlation 25%"
        );
    }

    #[test]
    fn test_parse_tc_seconds_delay() {
        let qdisc_info =
            "qdisc netem 802b: root refcnt 9 limit 1000 delay 2.95s loss 49.1% rate 1Mbit";

        let config = parse_tc_parameters(qdisc_info);

        assert_eq!(
            config.delay_ms,
            Some(2950.0),
            "Should parse delay 2.95s as 2950ms"
        );
        assert_eq!(config.loss, 49.1, "Should parse loss 49.1%");
        assert_eq!(
            config.rate_limit_kbps,
            Some(1000),
            "Should parse rate 1Mbit as 1000 kbps"
        );
    }

    #[test]
    fn test_parse_tc_rate_limit_variations() {
        let config_kbit = parse_tc_parameters("qdisc netem root rate 500Kbit");
        assert_eq!(config_kbit.rate_limit_kbps, Some(500));

        let config_mbit = parse_tc_parameters("qdisc netem root rate 2Mbit");
        assert_eq!(config_mbit.rate_limit_kbps, Some(2000));

        let config_lower = parse_tc_parameters("qdisc netem root rate 1000kbit");
        assert_eq!(config_lower.rate_limit_kbps, Some(1000));
    }

    #[test]
    fn test_parse_tc_empty_and_invalid() {
        let config_empty = parse_tc_parameters("");
        assert_eq!(config_empty.loss, 0.0);
        assert_eq!(config_empty.delay_ms, None);

        let config_noqueue = parse_tc_parameters("qdisc noqueue 0: dev lo root refcnt 2");
        assert_eq!(config_noqueue.loss, 0.0);
        assert_eq!(config_noqueue.delay_ms, None);
    }

    #[test]
    fn test_build_tc_command_string_basic() {
        let cmd = build_tc_command_string(
            "eth0", 5.0, None, None, None, None, None, None, None, None, None, None, None, None,
        );
        assert!(cmd.contains("tc qdisc replace dev eth0 root netem"));
        assert!(cmd.contains("loss 5%"));
    }

    #[test]
    fn test_build_tc_command_string_full() {
        let cmd = build_tc_command_string(
            "eth0",
            10.0,
            Some(25.0),
            Some(100.0),
            Some(10.0),
            Some(50.0),
            Some(5.0),
            Some(10.0),
            Some(20.0),
            Some(30.0),
            Some(3),
            Some(1.0),
            Some(5.0),
            Some(1000),
        );
        assert!(cmd.contains("loss 10% correlation 25%"));
        assert!(cmd.contains("delay 100ms 10ms 50%"));
        assert!(cmd.contains("duplicate 5% 10%"));
        assert!(cmd.contains("reorder 20% 30% gap 3"));
        assert!(cmd.contains("corrupt 1% 5%"));
        assert!(cmd.contains("rate 1mbit"));
    }

    #[test]
    fn test_build_tc_configuration() {
        let config = build_tc_configuration(
            "eth0",
            5.0,
            None,
            Some(50.0),
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
        );
        assert_eq!(config.loss, 5.0);
        assert_eq!(config.delay_ms, Some(50.0));
        assert!(config.command.contains("loss 5%"));
        assert!(config.command.contains("delay 50ms"));
    }
}
