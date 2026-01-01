//! TC configuration command building utilities.
//!
//! This module provides functions for:
//! - Building TC command strings for display
//! - Converting between configuration formats
//!
//! Note: TC configuration parsing is now done via the nlink crate's
//! `NetemOptions` which directly parses netlink messages from the kernel.

use tcgui_shared::TcConfiguration;

/// Build a TC command string for display from configuration parameters.
///
/// This generates a human-readable command string showing what TC configuration
/// would be applied (useful for logging and UI display).
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

    if let Some(delay) = delay_ms
        && delay > 0.0
    {
        let mut delay_part = format!("delay {}ms", delay);
        if let Some(jitter) = delay_jitter_ms
            && jitter > 0.0
        {
            delay_part.push_str(&format!(" {}ms", jitter));
            if let Some(delay_corr) = delay_correlation
                && delay_corr > 0.0
            {
                delay_part.push_str(&format!(" {}%", delay_corr));
            }
        }
        cmd_parts.push(delay_part);
    }

    if let Some(duplicate) = duplicate_percent
        && duplicate > 0.0
    {
        let mut duplicate_part = format!("duplicate {}%", duplicate);
        if let Some(dup_corr) = duplicate_correlation
            && dup_corr > 0.0
        {
            duplicate_part.push_str(&format!(" {}%", dup_corr));
        }
        cmd_parts.push(duplicate_part);
    }

    if let Some(reorder) = reorder_percent
        && reorder > 0.0
    {
        let mut reorder_part = format!("reorder {}%", reorder);
        if let Some(reorder_corr) = reorder_correlation
            && reorder_corr > 0.0
        {
            reorder_part.push_str(&format!(" {}%", reorder_corr));
        }
        if let Some(gap) = reorder_gap
            && gap > 0
        {
            reorder_part.push_str(&format!(" gap {}", gap));
        }
        cmd_parts.push(reorder_part);
    }

    if let Some(corrupt) = corrupt_percent
        && corrupt > 0.0
    {
        let mut corrupt_part = format!("corrupt {}%", corrupt);
        if let Some(corrupt_corr) = corrupt_correlation
            && corrupt_corr > 0.0
        {
            corrupt_part.push_str(&format!(" {}%", corrupt_corr));
        }
        cmd_parts.push(corrupt_part);
    }

    if let Some(rate) = rate_limit_kbps
        && rate > 0
    {
        let rate_part = if rate >= 1000 {
            format!("rate {}mbit", rate / 1000)
        } else {
            format!("rate {}kbit", rate)
        };
        cmd_parts.push(rate_part);
    }

    cmd_parts.join(" ")
}

/// Build a TcConfiguration from individual parameters.
///
/// This is a convenience function that combines parameter values with a
/// generated command string.
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
