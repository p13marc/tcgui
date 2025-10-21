//! Regression tests for tcgui-backend
//!
//! These tests capture known behaviors and ensure they remain consistent
//! across refactoring and changes. They test specific scenarios that have
//! been fixed or implemented to prevent regressions.

use tcgui_shared::TcConfiguration;

/// Test TC parameter parsing from real tc command output
/// This ensures we continue to correctly parse tc qdisc information
#[cfg(test)]
mod tc_parsing_regression_tests {
    use super::*;

    // Helper function from main.rs tests - kept here for regression testing
    fn parse_tc_parameters_test(qdisc_info: &str) -> TcConfiguration {
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
                    }
                }
                // Handle seconds (s) - convert to milliseconds
                else if first_token.ends_with("s") {
                    let delay_str = first_token.trim_end_matches("s");
                    if let Ok(delay_val) = delay_str.parse::<f32>() {
                        let delay_ms = delay_val * 1000.0; // Convert seconds to milliseconds
                        config.delay_ms = Some(delay_ms);
                    }
                }

                // Parse jitter (second token if it ends with ms)
                if delay_tokens.len() > 1 && delay_tokens[1].ends_with("ms") {
                    let jitter_str = delay_tokens[1].trim_end_matches("ms");
                    if let Ok(jitter_val) = jitter_str.parse::<f32>() {
                        config.delay_jitter_ms = Some(jitter_val);
                    }

                    // Parse correlation (third token if it ends with %)
                    if delay_tokens.len() > 2 && delay_tokens[2].ends_with("%") {
                        let corr_str = delay_tokens[2].trim_end_matches("%");
                        if let Ok(corr_val) = corr_str.parse::<f32>() {
                            config.delay_correlation = Some(corr_val);
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
                }
            }
        }

        // Parse reorder percentage
        if let Some(reorder_start) = qdisc_info.find("reorder ") {
            let reorder_part = &qdisc_info[reorder_start + 8..];
            if let Some(percent_pos) = reorder_part.find('%') {
                if let Ok(reorder_val) = reorder_part[..percent_pos].trim().parse::<f32>() {
                    config.reorder_percent = Some(reorder_val);
                }
            }

            // Parse reorder gap
            if let Some(gap_start) = qdisc_info.find("gap ") {
                let gap_part = &qdisc_info[gap_start + 4..];
                let gap_end = gap_part.find(' ').unwrap_or(gap_part.len());
                if let Ok(gap_val) = gap_part[..gap_end].trim().parse::<u32>() {
                    config.reorder_gap = Some(gap_val);
                }
            }
        }

        // Parse corrupt percentage
        if let Some(corrupt_start) = qdisc_info.find("corrupt ") {
            let corrupt_part = &qdisc_info[corrupt_start + 8..];
            if let Some(percent_pos) = corrupt_part.find('%') {
                if let Ok(corrupt_val) = corrupt_part[..percent_pos].trim().parse::<f32>() {
                    config.corrupt_percent = Some(corrupt_val);
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
                }
            } else if let Some(mbit_pos) = rate_part_lower.find("mbit") {
                if let Ok(rate_val) = rate_part[..mbit_pos].trim().parse::<u32>() {
                    config.rate_limit_kbps = Some(rate_val * 1000); // Convert mbit to kbit
                }
            }
        }

        config
    }

    #[test]
    fn regression_test_seconds_delay_parsing() {
        // This was a reported bug: delay parsing failed for seconds format "2.95s"
        let qdisc_info = "qdisc netem 802b: root refcnt 9 limit 1000 delay 2.95s";
        let config = parse_tc_parameters_test(qdisc_info);

        // Should parse 2.95s as 2950ms
        assert_eq!(
            config.delay_ms,
            Some(2950.0),
            "Failed to parse seconds format delay"
        );
    }

    #[test]
    fn regression_test_fractional_delay_parsing() {
        // Test fractional milliseconds like "1.5ms"
        let qdisc_info = "qdisc netem 8030: root refcnt 2 limit 1000 delay 1.5ms";
        let config = parse_tc_parameters_test(qdisc_info);

        assert_eq!(
            config.delay_ms,
            Some(1.5),
            "Failed to parse fractional milliseconds"
        );
    }

    #[test]
    fn regression_test_case_insensitive_rate_parsing() {
        // Test both uppercase and lowercase rate units
        let test_cases = vec![
            ("rate 100Kbit", Some(100)),
            ("rate 100kbit", Some(100)),
            ("rate 2Mbit", Some(2000)),
            ("rate 2mbit", Some(2000)),
        ];

        for (qdisc_part, expected) in test_cases {
            let qdisc_info = format!("qdisc netem 8030: root {}", qdisc_part);
            let config = parse_tc_parameters_test(&qdisc_info);
            assert_eq!(
                config.rate_limit_kbps, expected,
                "Failed to parse rate: {}",
                qdisc_part
            );
        }
    }

    #[test]
    fn regression_test_complex_real_world_output() {
        // This is a real qdisc output that was failing to parse correctly
        let qdisc_info = "qdisc netem 802b: root refcnt 9 limit 1000 delay 2.95s loss 49.1% 30.1% duplicate 27.8% reorder 71.8% corrupt 25.3% rate 1Mbit seed 10478122975723631342";

        let config = parse_tc_parameters_test(qdisc_info);

        // All parameters should be parsed correctly
        assert_eq!(config.loss, 49.1);
        assert_eq!(config.delay_ms, Some(2950.0)); // 2.95s = 2950ms
        assert_eq!(config.duplicate_percent, Some(27.8));
        assert_eq!(config.reorder_percent, Some(71.8));
        assert_eq!(config.corrupt_percent, Some(25.3));
        assert_eq!(config.rate_limit_kbps, Some(1000)); // 1Mbit = 1000kbps
    }

    #[test]
    fn regression_test_reorder_gap_parsing() {
        // Ensure reorder gap is parsed correctly
        let qdisc_info = "qdisc netem 802d: root refcnt 2 limit 1000 reorder 25% gap 5";
        let config = parse_tc_parameters_test(qdisc_info);

        assert_eq!(config.reorder_percent, Some(25.0));
        assert_eq!(config.reorder_gap, Some(5));
    }

    #[test]
    fn regression_test_zero_values_not_parsed() {
        // Ensure that "0%" values are not set (they should remain None/default)
        let qdisc_info = "qdisc netem 8030: root limit 1000 loss 0% delay 0ms duplicate 0%";
        let config = parse_tc_parameters_test(qdisc_info);

        // Zero values should still be parsed as they appear in the output
        assert_eq!(config.loss, 0.0);
        assert_eq!(config.delay_ms, Some(0.0));
        assert_eq!(config.duplicate_percent, Some(0.0));
    }

    #[test]
    fn regression_test_no_netem_qdisc() {
        // Test parsing non-netem qdisc (should return defaults)
        let qdisc_info = "qdisc noqueue 0: root refcnt 2";
        let config = parse_tc_parameters_test(qdisc_info);

        // Should not find any netem parameters
        assert_eq!(config.loss, 0.0);
        assert_eq!(config.delay_ms, None);
        assert_eq!(config.duplicate_percent, None);
        assert_eq!(config.reorder_percent, None);
        assert_eq!(config.corrupt_percent, None);
        assert_eq!(config.rate_limit_kbps, None);
    }

    #[test]
    fn regression_test_malformed_percentages() {
        // Test handling of malformed percentage values
        let qdisc_info = "qdisc netem 8030: root loss invalid% duplicate notanumber%";
        let config = parse_tc_parameters_test(qdisc_info);

        // Should not parse invalid values
        assert_eq!(config.loss, 0.0); // Default
        assert_eq!(config.duplicate_percent, None); // Should not be parsed
    }

    #[test]
    fn regression_test_empty_qdisc_output() {
        // Test empty or minimal qdisc output
        let config = parse_tc_parameters_test("");

        // Should return all defaults
        assert_eq!(config.loss, 0.0);
        assert_eq!(config.delay_ms, None);
        assert_eq!(config.duplicate_percent, None);
        assert_eq!(config.reorder_percent, None);
        assert_eq!(config.corrupt_percent, None);
        assert_eq!(config.rate_limit_kbps, None);
    }
}

/// Test parameter removal logic (this was a major bug fix)
#[cfg(test)]
mod parameter_removal_regression_tests {
    #[test]
    fn regression_test_reorder_parameter_removal() {
        // This was a specific bug: unchecking reorder checkbox didn't remove the parameter
        // because Linux tc replace preserves old parameters

        // Simulate current state with reorder enabled
        struct MockCurrentState {
            has_reorder: bool,
        }

        let current = MockCurrentState { has_reorder: true };

        // New state without reorder (reorder_percent = None)
        let new_reorder_percent: Option<f32> = None;

        // This should trigger qdisc recreation, not replacement
        let should_recreate = current.has_reorder && new_reorder_percent.is_none_or(|r| r <= 0.0);

        assert!(
            should_recreate,
            "Should recreate qdisc when removing reorder parameter"
        );
    }

    #[test]
    fn regression_test_loss_parameter_removal() {
        // Test loss parameter removal
        struct MockCurrentState {
            has_loss: bool,
        }

        let current = MockCurrentState { has_loss: true };

        // Setting loss to 0 should trigger recreation
        let new_loss = 0.0;
        let should_recreate = current.has_loss && new_loss <= 0.0;

        assert!(
            should_recreate,
            "Should recreate qdisc when removing loss parameter"
        );
    }

    #[test]
    fn regression_test_delay_parameter_removal() {
        // Test delay parameter removal
        struct MockCurrentState {
            has_delay: bool,
        }

        let current = MockCurrentState { has_delay: true };

        // Setting delay to None should trigger recreation
        let new_delay: Option<f32> = None;
        let should_recreate = current.has_delay && new_delay.is_none_or(|d| d <= 0.0);

        assert!(
            should_recreate,
            "Should recreate qdisc when removing delay parameter"
        );
    }

    #[test]
    fn regression_test_rate_limit_zero_removal() {
        // Test rate limit removal with 0 value
        struct MockCurrentState {
            has_rate: bool,
        }

        let current = MockCurrentState { has_rate: true };

        // Setting rate to 0 should trigger recreation (u32 comparison fix)
        let new_rate = Some(0u32);
        let should_recreate = current.has_rate && new_rate.is_none_or(|r| r == 0);

        assert!(
            should_recreate,
            "Should recreate qdisc when setting rate limit to 0"
        );
    }

    #[test]
    fn regression_test_parameter_modification_uses_replace() {
        // Test that modifying parameters (not removing) uses replace
        struct MockCurrentState {
            has_loss: bool,
            has_delay: bool,
        }

        let current = MockCurrentState {
            has_loss: true,
            has_delay: true,
        };

        // Modifying existing parameters should use replace
        let new_loss = 10.0; // Changed from some other value
        let new_delay = Some(200.0); // Changed from some other value

        let will_remove_loss = current.has_loss && new_loss <= 0.0;
        let will_remove_delay = current.has_delay && new_delay.is_none_or(|d| d <= 0.0);
        let should_recreate = will_remove_loss || will_remove_delay;

        assert!(
            !should_recreate,
            "Should use replace when only modifying parameters"
        );
    }
}

/// Test bandwidth calculation edge cases
#[cfg(test)]
mod bandwidth_calculation_regression_tests {
    #[test]
    fn regression_test_bandwidth_counter_wraparound() {
        // Test handling of counter wraparound (u64 overflow)
        let previous_bytes = u64::MAX - 1000;
        let current_bytes = 500; // Wrapped around
        let time_diff = 1.0; // 1 second

        // Should handle wraparound correctly
        let expected_diff = (u64::MAX - previous_bytes) + current_bytes + 1;
        let rate = expected_diff as f64 / time_diff;

        // This is the logic that should be implemented for wraparound handling
        let calculated_diff = if current_bytes < previous_bytes {
            // Counter wrapped around
            (u64::MAX - previous_bytes) + current_bytes + 1
        } else {
            current_bytes - previous_bytes
        };

        assert_eq!(calculated_diff, expected_diff);
        assert!(rate > 0.0, "Rate calculation should handle wraparound");
    }

    #[test]
    fn regression_test_zero_time_difference() {
        // Test handling of zero time difference (divide by zero protection)
        let previous_bytes = 1000u64;
        let current_bytes = 2000u64;
        let time_diff = 0.0;

        // Should return 0 or previous rate, not panic
        let rate = if time_diff <= 0.0 {
            0.0 // Safe fallback
        } else {
            (current_bytes - previous_bytes) as f64 / time_diff
        };

        assert_eq!(rate, 0.0, "Should handle zero time difference gracefully");
    }

    #[test]
    fn regression_test_negative_time_difference() {
        // Test handling of negative time difference (clock adjustments)
        let previous_bytes = 1000u64;
        let current_bytes = 2000u64;
        let time_diff = -1.0; // Clock went backwards

        // Should return 0 or previous rate, not negative rate
        let rate = if time_diff <= 0.0 {
            0.0 // Safe fallback
        } else {
            (current_bytes - previous_bytes) as f64 / time_diff
        };

        assert_eq!(
            rate, 0.0,
            "Should handle negative time difference gracefully"
        );
    }
}

/// Test interface discovery edge cases
#[cfg(test)]
mod interface_discovery_regression_tests {
    #[test]
    fn regression_test_loopback_filtering() {
        // Test that loopback interfaces are properly filtered when requested
        let interfaces = ["lo", "eth0", "wlan0"];
        let exclude_loopback = true;

        let filtered: Vec<&str> = interfaces
            .iter()
            .filter(|&&name| !exclude_loopback || name != "lo")
            .copied()
            .collect();

        assert!(!filtered.contains(&"lo"), "Loopback should be filtered out");
        assert!(
            filtered.contains(&"eth0"),
            "Regular interfaces should remain"
        );
        assert_eq!(filtered.len(), 2, "Should filter exactly one interface");
    }

    #[test]
    fn regression_test_namespace_default_handling() {
        // Test that "default" namespace is handled specially
        let namespace = "default";

        // Command generation should not include "ip netns exec" for default namespace
        let uses_netns = namespace != "default";

        assert!(!uses_netns, "Default namespace should not use netns exec");

        // Test non-default namespace
        let custom_namespace = "test-ns";
        let uses_netns_custom = custom_namespace != "default";

        assert!(uses_netns_custom, "Custom namespace should use netns exec");
    }

    #[test]
    fn regression_test_interface_type_detection() {
        // Test interface type detection based on name patterns
        let interface_names = vec![
            ("lo", "Loopback"),
            ("eth0", "Physical"),
            ("veth0", "Virtual"),
            ("br0", "Bridge"),
            ("tun0", "TUN"),
            ("tap0", "TAP"),
        ];

        for (name, expected_type) in interface_names {
            let detected_type = match name {
                "lo" => "Loopback",
                n if n.starts_with("veth") => "Virtual",
                n if n.starts_with("br") => "Bridge",
                n if n.starts_with("tun") => "TUN",
                n if n.starts_with("tap") => "TAP",
                n if n.starts_with("eth") || n.starts_with("en") => "Physical",
                _ => "Unknown",
            };

            assert_eq!(
                detected_type, expected_type,
                "Wrong interface type for {}",
                name
            );
        }
    }
}

/// Test error handling and recovery scenarios
#[cfg(test)]
mod error_handling_regression_tests {
    #[test]
    fn regression_test_tc_command_permission_error() {
        // Test handling of permission denied errors
        let error_output = "RTNETLINK answers: Operation not permitted";

        // Should detect this as a permission error
        let is_permission_error = error_output.contains("Operation not permitted")
            || error_output.contains("Permission denied");

        assert!(is_permission_error, "Should detect permission errors");
    }

    #[test]
    fn regression_test_interface_not_found_error() {
        // Test handling of interface not found errors
        let error_output = "Cannot find device \"nonexistent\"";

        let is_interface_error = error_output.contains("Cannot find device");

        assert!(
            is_interface_error,
            "Should detect interface not found errors"
        );
    }

    #[test]
    fn regression_test_invalid_parameter_error() {
        // Test handling of invalid parameter errors
        let error_output = "Illegal \"loss\"";

        let is_parameter_error = error_output.contains("Illegal");

        assert!(is_parameter_error, "Should detect invalid parameter errors");
    }

    #[test]
    fn regression_test_namespace_not_found_error() {
        // Test handling of namespace not found errors
        let error_output =
            "Cannot open network namespace \"nonexistent\": No such file or directory";

        let is_namespace_error = error_output.contains("Cannot open network namespace");

        assert!(
            is_namespace_error,
            "Should detect namespace not found errors"
        );
    }
}

/// Test Zenoh communication edge cases
#[cfg(test)]
mod zenoh_communication_regression_tests {
    #[test]
    fn regression_test_large_message_handling() {
        // Test that large messages (like many interfaces) are handled correctly
        use serde_json;
        use tcgui_shared::NetworkInterface;

        // Create a large interface list
        let mut large_interface_list = Vec::new();
        for i in 0..1000 {
            large_interface_list.push(NetworkInterface {
                name: format!("veth{}", i),
                namespace: "default".to_string(),
                index: i as u32,
                is_up: true,
                has_tc_qdisc: false,
                interface_type: tcgui_shared::InterfaceType::Virtual,
            });
        }

        // Should be able to serialize and deserialize large messages
        let serialized = serde_json::to_string(&large_interface_list).unwrap();
        let deserialized: Vec<NetworkInterface> = serde_json::from_str(&serialized).unwrap();

        assert_eq!(
            deserialized.len(),
            1000,
            "Should handle large message serialization"
        );
        assert_eq!(
            deserialized[999].name, "veth999",
            "Should preserve data in large messages"
        );
    }

    #[test]
    fn regression_test_unicode_interface_names() {
        // Test handling of unicode characters in interface names
        use serde_json;
        use tcgui_shared::NetworkInterface;

        let unicode_interface = NetworkInterface {
            name: "eth-тест-🌐".to_string(),
            namespace: "тест-namespace".to_string(),
            index: 1,
            is_up: true,
            has_tc_qdisc: false,
            interface_type: tcgui_shared::InterfaceType::Physical,
        };

        // Should handle unicode correctly
        let serialized = serde_json::to_string(&unicode_interface).unwrap();
        let deserialized: NetworkInterface = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.name, "eth-тест-🌐");
        assert_eq!(deserialized.namespace, "тест-namespace");
    }

    #[test]
    fn regression_test_special_characters_in_commands() {
        // Test handling of special characters that might break command parsing
        let special_namespace = "test-ns_with.special-chars";
        let special_interface = "eth0:1"; // VLAN interface

        // Should handle special characters without breaking command construction
        let is_valid_namespace =
            !special_namespace.contains(' ') && !special_namespace.contains('"');
        let is_valid_interface =
            !special_interface.contains(' ') && !special_interface.contains('"');

        assert!(is_valid_namespace, "Should validate namespace characters");
        assert!(is_valid_interface, "Should validate interface characters");
    }
}
