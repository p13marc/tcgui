//! Integration tests for tcgui-backend
//!
//! These tests verify the interaction between different components
//! and test the backend behavior with mocked system interfaces.

use tcgui_shared::errors::TcguiError;

// Mock TC command manager for testing
pub struct MockTcCommandManager {
    pub executed_commands: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    pub should_fail: bool,
}

impl Default for MockTcCommandManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MockTcCommandManager {
    pub fn new() -> Self {
        Self {
            executed_commands: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            should_fail: false,
        }
    }

    pub fn new_failing() -> Self {
        Self {
            executed_commands: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            should_fail: true,
        }
    }

    #[allow(clippy::too_many_arguments)] // Test method - matches production API
    pub async fn apply_tc_config_in_namespace(
        &self,
        namespace: &str,
        interface: &str,
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
    ) -> Result<String, TcguiError> {
        if self.should_fail {
            return Err(TcguiError::TcCommandError {
                message: "Mock TC command failed".to_string(),
            });
        }

        let mut commands = self.executed_commands.lock().unwrap();
        let command_desc = format!(
            "tc apply {}/{}: loss={:.1}% delay={:?}ms dup={:?}% reorder={:?}% corrupt={:?}% rate={:?}kbps",
            namespace,
            interface,
            loss,
            delay_ms,
            duplicate_percent,
            reorder_percent,
            corrupt_percent,
            rate_limit_kbps
        );
        commands.push(command_desc.clone());

        Ok(format!("Applied TC config: {}", command_desc))
    }

    pub async fn remove_tc_config_from_namespace(
        &self,
        _namespace: &str,
        interface: &str,
    ) -> Result<String, TcguiError> {
        if self.should_fail {
            return Err(TcguiError::TcCommandError {
                message: "Mock TC remove failed".to_string(),
            });
        }

        let mut commands = self.executed_commands.lock().unwrap();
        let command_desc = format!("tc remove {}/{}", _namespace, interface);
        commands.push(command_desc.clone());

        Ok(format!("Removed TC config: {}", command_desc))
    }

    pub fn get_executed_commands(&self) -> Vec<String> {
        self.executed_commands.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tc_apply_integration() {
        let mock_tc = MockTcCommandManager::new();

        // Test applying basic loss configuration
        let result = mock_tc
            .apply_tc_config_in_namespace(
                "default",
                "eth0",
                5.0,
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
            )
            .await;

        assert!(result.is_ok());
        let commands = mock_tc.get_executed_commands();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].contains("loss=5.0%"));
        assert!(commands[0].contains("default/eth0"));
    }

    #[tokio::test]
    async fn test_tc_apply_complex_config() {
        let mock_tc = MockTcCommandManager::new();

        // Test applying complex configuration with all parameters
        let result = mock_tc
            .apply_tc_config_in_namespace(
                "test-ns",
                "veth0",
                2.5,
                Some(15.0),
                Some(100.0),
                Some(20.0),
                Some(30.0),
                Some(5.0),
                Some(10.0),
                Some(25.0),
                Some(50.0),
                Some(3),
                Some(1.0),
                Some(5.0),
                Some(1000),
            )
            .await;

        assert!(result.is_ok());
        let commands = mock_tc.get_executed_commands();
        assert_eq!(commands.len(), 1);

        let command = &commands[0];
        assert!(command.contains("loss=2.5%"));
        assert!(command.contains("delay=Some(100.0)ms"));
        assert!(command.contains("dup=Some(5.0)%"));
        assert!(command.contains("reorder=Some(25.0)%"));
        assert!(command.contains("corrupt=Some(1.0)%"));
        assert!(command.contains("rate=Some(1000)kbps"));
        assert!(command.contains("test-ns/veth0"));
    }

    #[tokio::test]
    async fn test_tc_remove_integration() {
        let mock_tc = MockTcCommandManager::new();

        let result = mock_tc
            .remove_tc_config_from_namespace("default", "eth0")
            .await;

        assert!(result.is_ok());
        let commands = mock_tc.get_executed_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0], "tc remove default/eth0");
    }

    #[tokio::test]
    async fn test_tc_command_failure_handling() {
        let mock_tc = MockTcCommandManager::new_failing();

        let result = mock_tc
            .apply_tc_config_in_namespace(
                "default", "eth0", 5.0, None, None, None, None, None, None, None, None, None, None,
                None, None,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            TcguiError::TcCommandError { message } => {
                assert_eq!(message, "Mock TC command failed");
            }
            _ => panic!("Expected TcCommandError"),
        }
    }

    #[tokio::test]
    async fn test_multiple_tc_operations_sequence() {
        let mock_tc = MockTcCommandManager::new();

        // Apply initial configuration
        let _result1 = mock_tc
            .apply_tc_config_in_namespace(
                "default", "eth0", 5.0, None, None, None, None, None, None, None, None, None, None,
                None, None,
            )
            .await
            .unwrap();

        // Apply updated configuration
        let _result2 = mock_tc
            .apply_tc_config_in_namespace(
                "default",
                "eth0",
                10.0,
                Some(25.0),
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
                Some(512),
            )
            .await
            .unwrap();

        // Remove configuration
        let _result3 = mock_tc
            .remove_tc_config_from_namespace("default", "eth0")
            .await
            .unwrap();

        let commands = mock_tc.get_executed_commands();
        assert_eq!(commands.len(), 3);

        // Verify command sequence
        assert!(commands[0].contains("loss=5.0%"));
        assert!(commands[1].contains("loss=10.0%"));
        assert!(commands[1].contains("delay=Some(50.0)ms"));
        assert!(commands[1].contains("rate=Some(512)kbps"));
        assert!(commands[2].contains("tc remove"));
    }

    #[tokio::test]
    async fn test_namespace_isolation() {
        let mock_tc = MockTcCommandManager::new();

        // Apply TC to default namespace
        let _result1 = mock_tc
            .apply_tc_config_in_namespace(
                "default", "eth0", 5.0, None, None, None, None, None, None, None, None, None, None,
                None, None,
            )
            .await
            .unwrap();

        // Apply TC to custom namespace
        let _result2 = mock_tc
            .apply_tc_config_in_namespace(
                "custom-ns",
                "veth0",
                10.0,
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
                None,
            )
            .await
            .unwrap();

        let commands = mock_tc.get_executed_commands();
        assert_eq!(commands.len(), 2);

        // Verify namespace isolation
        assert!(commands[0].contains("default/eth0"));
        assert!(commands[1].contains("custom-ns/veth0"));
        assert!(commands[0].contains("loss=5.0%"));
        assert!(commands[1].contains("loss=10.0%"));
    }

    #[tokio::test]
    async fn test_interface_specific_operations() {
        let mock_tc = MockTcCommandManager::new();

        // Apply different configs to different interfaces
        let _result1 = mock_tc
            .apply_tc_config_in_namespace(
                "default", "eth0", 5.0, None, None, None, None, None, None, None, None, None, None,
                None, None,
            )
            .await
            .unwrap();

        let _result2 = mock_tc
            .apply_tc_config_in_namespace(
                "default",
                "eth1",
                15.0,
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
            )
            .await
            .unwrap();

        let commands = mock_tc.get_executed_commands();
        assert_eq!(commands.len(), 2);

        // Verify interface-specific configurations
        assert!(commands[0].contains("default/eth0"));
        assert!(commands[0].contains("loss=5.0%"));
        assert!(commands[0].contains("delay=None"));

        assert!(commands[1].contains("default/eth1"));
        assert!(commands[1].contains("loss=15.0%"));
        assert!(commands[1].contains("delay=Some(100.0)ms"));
    }
}

/// Tests for TC parameter decision making (replace vs recreate logic)
#[cfg(test)]
mod tc_decision_tests {
    // Mock current TC config detection
    struct MockCurrentTcConfig {
        pub has_loss: bool,
        pub has_delay: bool,
        pub has_duplicate: bool,
        pub has_reorder: bool,
        pub has_corrupt: bool,
        pub has_rate: bool,
    }

    impl MockCurrentTcConfig {
        fn new() -> Self {
            Self {
                has_loss: false,
                has_delay: false,
                has_duplicate: false,
                has_reorder: false,
                has_corrupt: false,
                has_rate: false,
            }
        }

        fn with_all_features() -> Self {
            Self {
                has_loss: true,
                has_delay: true,
                has_duplicate: true,
                has_reorder: true,
                has_corrupt: true,
                has_rate: true,
            }
        }
    }

    fn should_recreate_qdisc(
        current: &MockCurrentTcConfig,
        loss: f32,
        delay_ms: Option<f32>,
        duplicate_percent: Option<f32>,
        reorder_percent: Option<f32>,
        corrupt_percent: Option<f32>,
        rate_limit_kbps: Option<u32>,
    ) -> bool {
        let will_remove_loss = current.has_loss && loss <= 0.0;
        let will_remove_delay = current.has_delay && delay_ms.is_none_or(|d| d <= 0.0);
        let will_remove_duplicate =
            current.has_duplicate && duplicate_percent.is_none_or(|d| d <= 0.0);
        let will_remove_reorder = current.has_reorder && reorder_percent.is_none_or(|r| r <= 0.0);
        let will_remove_corrupt = current.has_corrupt && corrupt_percent.is_none_or(|c| c <= 0.0);
        let will_remove_rate = current.has_rate && rate_limit_kbps.is_none_or(|r| r == 0);

        will_remove_loss
            || will_remove_delay
            || will_remove_duplicate
            || will_remove_reorder
            || will_remove_corrupt
            || will_remove_rate
    }

    #[test]
    fn test_replace_when_only_modifying_parameters() {
        let current = MockCurrentTcConfig::with_all_features();

        // Only changing values, not removing features
        let should_recreate = should_recreate_qdisc(
            &current,
            10.0,
            Some(200.0),
            Some(5.0),
            Some(30.0),
            Some(2.0),
            Some(1000),
        );

        assert!(
            !should_recreate,
            "Should use replace when only modifying parameters"
        );
    }

    #[test]
    fn test_recreate_when_removing_loss() {
        let mut current = MockCurrentTcConfig::with_all_features();
        current.has_loss = true;

        // Removing loss (setting to 0)
        let should_recreate = should_recreate_qdisc(
            &current,
            0.0,
            Some(100.0),
            Some(5.0),
            Some(25.0),
            Some(1.0),
            Some(512),
        );

        assert!(should_recreate, "Should recreate when removing loss");
    }

    #[test]
    fn test_recreate_when_removing_delay() {
        let current = MockCurrentTcConfig::with_all_features();

        // Removing delay (setting to None)
        let should_recreate = should_recreate_qdisc(
            &current,
            5.0,
            None,
            Some(5.0),
            Some(25.0),
            Some(1.0),
            Some(512),
        );

        assert!(should_recreate, "Should recreate when removing delay");
    }

    #[test]
    fn test_recreate_when_removing_reorder() {
        let current = MockCurrentTcConfig::with_all_features();

        // Removing reorder (setting to None)
        let should_recreate = should_recreate_qdisc(
            &current,
            5.0,
            Some(100.0),
            Some(5.0),
            None,
            Some(1.0),
            Some(512),
        );

        assert!(should_recreate, "Should recreate when removing reorder");
    }

    #[test]
    fn test_replace_when_adding_new_features() {
        let current = MockCurrentTcConfig::new();

        // Adding features to empty qdisc should use replace
        let should_recreate = should_recreate_qdisc(
            &current,
            5.0,
            Some(100.0),
            Some(5.0),
            Some(25.0),
            Some(1.0),
            Some(512),
        );

        assert!(
            !should_recreate,
            "Should use replace when adding features to empty qdisc"
        );
    }

    #[test]
    fn test_recreate_when_removing_multiple_features() {
        let current = MockCurrentTcConfig::with_all_features();

        // Removing multiple features
        let should_recreate =
            should_recreate_qdisc(&current, 0.0, None, None, Some(25.0), Some(1.0), Some(512));

        assert!(
            should_recreate,
            "Should recreate when removing multiple features"
        );
    }

    #[test]
    fn test_edge_case_rate_limit_zero() {
        let mut current = MockCurrentTcConfig::new();
        current.has_rate = true;

        // Rate limit 0 should trigger recreation
        let should_recreate =
            should_recreate_qdisc(&current, 5.0, Some(100.0), None, None, None, Some(0));

        assert!(
            should_recreate,
            "Should recreate when setting rate limit to 0"
        );
    }

    #[test]
    fn test_edge_case_very_small_values() {
        let current = MockCurrentTcConfig::with_all_features();

        // Very small but non-zero values should use replace
        let should_recreate = should_recreate_qdisc(
            &current,
            0.01,
            Some(0.1),
            Some(0.1),
            Some(0.1),
            Some(0.1),
            Some(1),
        );

        assert!(
            !should_recreate,
            "Should use replace for very small but non-zero values"
        );
    }
}

/// Test message serialization and deserialization for Zenoh communication
#[cfg(test)]
mod zenoh_message_tests {

    use tcgui_shared::{TcConfiguration, TcResponse, *};

    #[test]
    fn test_tc_request_serialization() {
        let request = TcRequest {
            namespace: "default".to_string(),
            interface: "eth0".to_string(),
            operation: TcOperation::Apply {
                loss: 5.0,
                correlation: Some(25.0),
                delay_ms: Some(100.0),
                delay_jitter_ms: None,
                delay_correlation: None,
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: None,
                reorder_correlation: None,
                reorder_gap: None,
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: Some(1000),
            },
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: TcRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.namespace, "default");
        assert_eq!(deserialized.interface, "eth0");

        match deserialized.operation {
            TcOperation::Apply {
                loss,
                rate_limit_kbps,
                ..
            } => {
                assert_eq!(loss, 5.0);
                assert_eq!(rate_limit_kbps, Some(1000));
            }
            _ => panic!("Expected Apply operation"),
        }
    }

    #[test]
    fn test_tc_response_serialization() {
        let response = TcResponse {
            success: true,
            message: "TC configuration applied successfully".to_string(),
            applied_config: Some(TcConfiguration {
                loss: 5.0,
                correlation: Some(25.0),
                delay_ms: Some(100.0),
                delay_jitter_ms: None,
                delay_correlation: None,
                duplicate_percent: None,
                duplicate_correlation: None,
                reorder_percent: None,
                reorder_correlation: None,
                reorder_gap: None,
                corrupt_percent: None,
                corrupt_correlation: None,
                rate_limit_kbps: Some(1000),
                command: "tc qdisc replace dev eth0 root netem loss 5% delay 100ms rate 1mbit"
                    .to_string(),
            }),
            error_code: None,
        };

        let serialized = serde_json::to_string(&response).unwrap();
        let deserialized: TcResponse = serde_json::from_str(&serialized).unwrap();

        assert!(deserialized.success);
        assert!(deserialized.message.contains("successfully"));
        assert!(deserialized.applied_config.is_some());
        let config = deserialized.applied_config.unwrap();
        assert_eq!(config.loss, 5.0);
        assert_eq!(config.rate_limit_kbps, Some(1000));
    }

    #[test]
    fn test_interface_control_request_serialization() {
        let request = InterfaceControlRequest {
            namespace: "test-ns".to_string(),
            interface: "veth0".to_string(),
            operation: InterfaceControlOperation::Enable,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: InterfaceControlRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.namespace, "test-ns");
        assert_eq!(deserialized.interface, "veth0");
        assert!(matches!(
            deserialized.operation,
            InterfaceControlOperation::Enable
        ));
    }

    #[test]
    fn test_malformed_json_handling() {
        // Test malformed JSON
        let malformed_json = r#"{"namespace": "test", "interface": "eth0"}"#; // Missing operation field
        let result: Result<TcRequest, _> = serde_json::from_str(malformed_json);

        assert!(result.is_err(), "Should fail on malformed JSON");
    }

    #[test]
    fn test_empty_optional_fields() {
        let request = TcRequest {
            namespace: "default".to_string(),
            interface: "eth0".to_string(),
            operation: TcOperation::Apply {
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
            },
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let deserialized: TcRequest = serde_json::from_str(&serialized).unwrap();

        match deserialized.operation {
            TcOperation::Apply {
                correlation,
                delay_ms,
                rate_limit_kbps,
                duplicate_percent,
                reorder_percent,
                corrupt_percent,
                ..
            } => {
                assert_eq!(correlation, None);
                assert_eq!(delay_ms, None);
                assert_eq!(rate_limit_kbps, None);
                assert_eq!(duplicate_percent, None);
                assert_eq!(reorder_percent, None);
                assert_eq!(corrupt_percent, None);
            }
            _ => panic!("Expected Apply operation"),
        }
    }
}
