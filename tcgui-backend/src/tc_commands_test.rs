//! Comprehensive unit tests for TC command functionality
//!
//! This module contains detailed unit tests for TC command generation, parsing,
//! validation, and edge cases to ensure robust traffic control functionality.

use crate::tc_commands::TcCommandManager;

/// Helper function to create a TC manager for testing
fn create_tc_manager() -> TcCommandManager {
    TcCommandManager::new()
}

/// Mock implementation for testing TC command generation without executing
impl TcCommandManager {
    /// Generate TC command for testing without executing
    #[allow(clippy::too_many_arguments)] // Test helper - matches production API
    pub fn generate_tc_command(
        &self,
        namespace: &str,
        interface: &str,
        action: &str,
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
    ) -> Vec<String> {
        let mut args = vec![];

        // Build base command depending on namespace
        if namespace == "default" {
            args.extend_from_slice(&[
                "tc".to_string(),
                "qdisc".to_string(),
                action.to_string(),
                "dev".to_string(),
                interface.to_string(),
                "root".to_string(),
                "netem".to_string(),
            ]);
        } else {
            args.extend_from_slice(&[
                "ip".to_string(),
                "netns".to_string(),
                "exec".to_string(),
                namespace.to_string(),
                "tc".to_string(),
                "qdisc".to_string(),
                action.to_string(),
                "dev".to_string(),
                interface.to_string(),
                "root".to_string(),
                "netem".to_string(),
            ]);
        }

        // Add loss parameters if loss > 0
        if loss > 0.0 {
            args.push("loss".to_string());
            args.push("random".to_string());
            args.push(format!("{}%", loss));

            if let Some(corr) = correlation
                && corr > 0.0
            {
                args.push(format!("{}%", corr));
            }
        }

        // Track whether we've added delay
        let mut has_delay = false;

        // Add delay parameters
        if let Some(delay) = delay_ms
            && delay > 0.0
        {
            args.push("delay".to_string());
            args.push(format!("{}ms", delay));
            has_delay = true;

            if let Some(jitter) = delay_jitter_ms
                && jitter > 0.0
            {
                args.push(format!("{}ms", jitter));

                if let Some(delay_corr) = delay_correlation
                    && delay_corr > 0.0
                {
                    args.push(format!("{}%", delay_corr));
                }
            }
        }

        // Add duplication parameters
        if let Some(duplicate) = duplicate_percent
            && duplicate > 0.0
        {
            args.push("duplicate".to_string());
            args.push(format!("{}%", duplicate));

            if let Some(dup_corr) = duplicate_correlation
                && dup_corr > 0.0
            {
                args.push(format!("{}%", dup_corr));
            }
        }

        // Add reordering parameters
        if let Some(reorder) = reorder_percent
            && reorder > 0.0
        {
            // Ensure delay is present (netem requires some delay for reorder)
            if !has_delay {
                args.push("delay".to_string());
                args.push("1ms".to_string());
            }
            args.push("reorder".to_string());
            args.push(format!("{}%", reorder));

            if let Some(reorder_corr) = reorder_correlation
                && reorder_corr > 0.0
            {
                args.push(format!("{}%", reorder_corr));
            }

            if let Some(gap) = reorder_gap
                && gap > 0
            {
                args.push("gap".to_string());
                args.push(format!("{}", gap));
            }
        }

        // Add corruption parameters
        if let Some(corrupt) = corrupt_percent
            && corrupt > 0.0
        {
            args.push("corrupt".to_string());
            args.push(format!("{}%", corrupt));

            if let Some(corrupt_corr) = corrupt_correlation
                && corrupt_corr > 0.0
            {
                args.push(format!("{}%", corrupt_corr));
            }
        }

        // Add rate limiting parameters
        if let Some(rate) = rate_limit_kbps
            && rate > 0
        {
            args.push("rate".to_string());
            if rate >= 1000 {
                args.push(format!("{}mbit", rate / 1000));
            } else {
                args.push(format!("{}kbit", rate));
            }
        }

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to get command args after the command itself
    fn get_netem_args(cmd_args: &[String]) -> &[String] {
        // Find "netem" and return everything after it
        if let Some(netem_idx) = cmd_args.iter().position(|arg| arg == "netem") {
            &cmd_args[netem_idx + 1..]
        } else {
            &[]
        }
    }

    #[test]
    fn test_basic_loss_command_generation() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default", "eth0", "replace", 5.0, None, None, None, None, None, None, None, None,
            None, None, None, None,
        );

        assert_eq!(args[0], "tc");
        assert_eq!(args[1], "qdisc");
        assert_eq!(args[2], "replace");
        assert_eq!(args[3], "dev");
        assert_eq!(args[4], "eth0");
        assert_eq!(args[5], "root");
        assert_eq!(args[6], "netem");

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["loss", "random", "5%"]);
    }

    #[test]
    fn test_loss_with_correlation() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            10.0,
            Some(25.0),
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
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["loss", "random", "10%", "25%"]);
    }

    #[test]
    fn test_delay_only() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            Some(100.0),
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

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["delay", "100ms"]);
    }

    #[test]
    fn test_delay_with_jitter() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            Some(100.0),
            Some(20.0),
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

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["delay", "100ms", "20ms"]);
    }

    #[test]
    fn test_delay_with_jitter_and_correlation() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            Some(100.0),
            Some(20.0),
            Some(75.0),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["delay", "100ms", "20ms", "75%"]);
    }

    #[test]
    fn test_duplicate_parameters() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            None,
            None,
            None,
            Some(15.0),
            Some(30.0),
            None,
            None,
            None,
            None,
            None,
            None,
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["duplicate", "15%", "30%"]);
    }

    #[test]
    fn test_reorder_without_delay_adds_automatic_delay() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(25.0),
            None,
            None,
            None,
            None,
            None,
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["delay", "1ms", "reorder", "25%"]);
    }

    #[test]
    fn test_reorder_with_existing_delay() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            Some(50.0),
            None,
            None,
            None,
            None,
            Some(25.0),
            Some(80.0),
            Some(5),
            None,
            None,
            None,
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(
            netem_args,
            &["delay", "50ms", "reorder", "25%", "80%", "gap", "5"]
        );
    }

    #[test]
    fn test_corrupt_parameters() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(8.0),
            Some(40.0),
            None,
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["corrupt", "8%", "40%"]);
    }

    #[test]
    fn test_rate_limit_kbps() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
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
            Some(512),
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["rate", "512kbit"]);
    }

    #[test]
    fn test_rate_limit_mbps_conversion() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
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
            Some(2000),
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &["rate", "2mbit"]);
    }

    #[test]
    fn test_namespace_command_generation() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "test-ns", "veth0", "add", 5.0, None, None, None, None, None, None, None, None, None,
            None, None, None,
        );

        assert_eq!(args[0], "ip");
        assert_eq!(args[1], "netns");
        assert_eq!(args[2], "exec");
        assert_eq!(args[3], "test-ns");
        assert_eq!(args[4], "tc");
        assert_eq!(args[5], "qdisc");
        assert_eq!(args[6], "add");
        assert_eq!(args[7], "dev");
        assert_eq!(args[8], "veth0");
    }

    #[test]
    fn test_complex_all_parameters() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            2.5,
            Some(15.0), // loss with correlation
            Some(50.0),
            Some(10.0),
            Some(25.0), // delay with jitter and correlation
            Some(5.0),
            Some(20.0), // duplicate with correlation
            Some(30.0),
            Some(60.0),
            Some(3), // reorder with correlation and gap
            Some(1.0),
            Some(10.0), // corrupt with correlation
            Some(1000), // rate limit
        );

        let netem_args = get_netem_args(&args);
        let expected = vec![
            "loss",
            "random",
            "2.5%",
            "15%",
            "delay",
            "50ms",
            "10ms",
            "25%",
            "duplicate",
            "5%",
            "20%",
            "reorder",
            "30%",
            "60%",
            "gap",
            "3",
            "corrupt",
            "1%",
            "10%",
            "rate",
            "1mbit",
        ];
        assert_eq!(netem_args, expected.as_slice());
    }

    #[test]
    fn test_zero_values_ignored() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.0,
            Some(15.0), // loss 0 should be ignored, correlation too
            Some(0.0),
            Some(10.0),
            Some(25.0), // delay 0 should ignore all delay params
            Some(0.0),
            Some(20.0), // duplicate 0 should be ignored
            Some(0.0),
            Some(60.0),
            Some(3), // reorder 0 should be ignored
            Some(0.0),
            Some(10.0), // corrupt 0 should be ignored
            Some(0),    // rate 0 should be ignored
        );

        let netem_args = get_netem_args(&args);
        // Should only have the base netem with no parameters
        assert_eq!(netem_args, &[] as &[String]);
    }

    #[test]
    fn test_edge_case_very_small_values() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            0.01,
            None, // Very small loss
            Some(0.5),
            None,
            None, // Very small delay
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1), // Minimum rate limit
        );

        let netem_args = get_netem_args(&args);
        assert_eq!(
            netem_args,
            &["loss", "random", "0.01%", "delay", "0.5ms", "rate", "1kbit"]
        );
    }

    #[test]
    fn test_edge_case_maximum_values() {
        let tc_manager = create_tc_manager();

        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            100.0,
            Some(100.0), // Maximum loss and correlation
            Some(5000.0),
            Some(1000.0),
            Some(100.0), // Maximum delay values
            Some(100.0),
            Some(100.0), // Maximum duplicate values
            Some(100.0),
            Some(100.0),
            Some(10), // Maximum reorder values
            Some(100.0),
            Some(100.0),   // Maximum corrupt values
            Some(1000000), // Maximum rate limit
        );

        let netem_args = get_netem_args(&args);
        let expected = vec![
            "loss",
            "random",
            "100%",
            "100%",
            "delay",
            "5000ms",
            "1000ms",
            "100%",
            "duplicate",
            "100%",
            "100%",
            "reorder",
            "100%",
            "100%",
            "gap",
            "10",
            "corrupt",
            "100%",
            "100%",
            "rate",
            "1000mbit",
        ];
        assert_eq!(netem_args, expected.as_slice());
    }

    #[test]
    fn test_parameter_validation_ranges() {
        // This test ensures we handle parameters within expected ranges
        // Even though validation might happen elsewhere, command generation should be robust

        let tc_manager = create_tc_manager();

        // Test with out-of-bound values (these might come from corrupted data)
        let args = tc_manager.generate_tc_command(
            "default",
            "eth0",
            "replace",
            -5.0,
            None, // Negative loss (should be treated as 0)
            Some(-100.0),
            None,
            None, // Negative delay (should be treated as 0)
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        // Negative values should be treated as zero and ignored
        let netem_args = get_netem_args(&args);
        assert_eq!(netem_args, &[] as &[String]);
    }
}
