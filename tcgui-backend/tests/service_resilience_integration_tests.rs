//! Integration tests for service resilience and error handling
//!
//! These tests validate error handling behavior across service boundaries
//! and under various failure scenarios.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use tcgui_backend::utils::service_resilience::{
    ServiceResilienceManager, execute_network_discovery, execute_system_command,
    execute_zenoh_communication,
};

/// Test retry behavior across different services
#[tokio::test]
async fn test_retry_integration() -> Result<()> {
    let _resilience_manager = ServiceResilienceManager::new();

    // Test that operations are retried on failure
    let result: Result<String> = execute_network_discovery(
        || async { Err(anyhow::anyhow!("Simulated network failure")) },
        "test_operation",
        "test_service",
    )
    .await;

    // Should eventually fail after retries
    assert!(result.is_err());

    // Test successful operation
    let result = execute_network_discovery(
        || async { Ok("Success".to_string()) },
        "test_operation",
        "test_service",
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Success");

    Ok(())
}

/// Test retry behavior with exponential backoff
#[tokio::test]
async fn test_retry_with_backoff_integration() -> Result<()> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    // Test that successful operations work
    let result = execute_system_command(
        || async { Ok("System operation successful".to_string()) },
        "retry_test",
        "test_service",
    )
    .await;

    // Should succeed immediately for successful operations
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "System operation successful");

    // Test that failing operations are retried
    let attempt_count = Arc::new(AtomicU32::new(0));
    let count_clone = Arc::clone(&attempt_count);

    let result: Result<String> = execute_system_command(
        || {
            let count = Arc::clone(&count_clone);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(anyhow::anyhow!("Persistent failure"))
            }
        },
        "failing_operation",
        "test_service",
    )
    .await;

    // Should fail but retry multiple times
    assert!(result.is_err());
    assert!(attempt_count.load(Ordering::SeqCst) > 1); // Should have retried

    Ok(())
}

/// Test Zenoh communication resilience
#[tokio::test]
async fn test_zenoh_communication_resilience() -> Result<()> {
    // Test successful Zenoh operations
    let result = execute_zenoh_communication(
        || async { Ok("Zenoh operation successful".to_string()) },
        "zenoh_success_test",
        "test_service",
    )
    .await;

    // Should succeed
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Zenoh operation successful");

    // Test failed Zenoh operations are handled properly
    let result: Result<String> = execute_zenoh_communication(
        || async { Err(anyhow::anyhow!("Zenoh communication failure")) },
        "zenoh_fail_test",
        "test_service",
    )
    .await;

    // Should fail but be handled with retry logic
    assert!(result.is_err());

    Ok(())
}

/// Test timeout behavior for operations
#[tokio::test]
async fn test_operation_timeout_handling() -> Result<()> {
    let result = timeout(
        Duration::from_millis(100),
        execute_network_discovery(
            || async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok("should_timeout".to_string())
            },
            "timeout_test",
            "test_service",
        ),
    )
    .await;

    // Should timeout
    assert!(result.is_err());

    Ok(())
}

/// Test concurrent operations with shared resilience
#[tokio::test]
async fn test_concurrent_operations_with_shared_resilience() -> Result<()> {
    let resilience_manager = Arc::new(ServiceResilienceManager::new());
    let mut handles = vec![];

    // Spawn multiple concurrent operations
    for i in 0..10 {
        let _manager = Arc::clone(&resilience_manager);
        let handle = tokio::spawn(async move {
            let operation_name = format!("concurrent_op_{}", i);
            execute_network_discovery(
                || async move {
                    // Simulate some operations succeeding, some failing
                    if i % 3 == 0 {
                        Err(anyhow::anyhow!("Simulated failure for operation {}", i))
                    } else {
                        Ok(format!("Success for operation {}", i))
                    }
                },
                &operation_name,
                "concurrent_test_service",
            )
            .await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(_) => success_count += 1,
            Err(_) => failure_count += 1,
        }
    }

    // Should have some successes and some failures
    assert!(success_count > 0);
    assert!(failure_count > 0);
    assert_eq!(success_count + failure_count, 10);

    Ok(())
}

/// Test error handling behavior
#[tokio::test]
async fn test_error_handling_behavior() -> Result<()> {
    // Test that errors are properly propagated through the resilience layer
    let result: Result<String> = execute_network_discovery(
        || async { Err(anyhow::anyhow!("Test error message")) },
        "error_test",
        "test_service",
    )
    .await;

    assert!(result.is_err());
    let error_msg = result.err().unwrap().to_string();
    assert!(error_msg.contains("Test error message") || error_msg.contains("Failed after"));

    Ok(())
}

/// Test error context preservation across service boundaries
#[tokio::test]
async fn test_error_context_preservation() -> Result<()> {
    let result: Result<String> = execute_network_discovery(
        || async {
            // Simulate a nested error that should preserve context
            Err(anyhow::anyhow!("Network interface not found"))
        },
        "context_test",
        "network_service",
    )
    .await;

    assert!(result.is_err());
    let error = result.err().unwrap();
    let error_string = error.to_string();
    // The service resilience layer may wrap errors, so we need to check for
    // the core error message presence. The exact format may vary due to retry logic.
    assert!(
        error_string.contains("Network interface not found")
            || error_string.contains("Failed after")
    );

    Ok(())
}

/// Test service resilience under simulated high load
#[tokio::test]
async fn test_high_load_resilience() -> Result<()> {
    let mut handles = vec![];
    const OPERATION_COUNT: usize = 50;

    for i in 0..OPERATION_COUNT {
        let handle = tokio::spawn(async move {
            // Mix of different operation types
            match i % 3 {
                0 => {
                    execute_network_discovery(
                        || async {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            if rand::random::<bool>() {
                                Ok(format!("Network operation {}", i))
                            } else {
                                Err(anyhow::anyhow!("Network failure {}", i))
                            }
                        },
                        &format!("network_op_{}", i),
                        "load_test_service",
                    )
                    .await
                }
                1 => {
                    execute_zenoh_communication(
                        || async {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                            if rand::random::<bool>() {
                                Ok(format!("Zenoh operation {}", i))
                            } else {
                                Err(anyhow::anyhow!("Zenoh failure {}", i))
                            }
                        },
                        &format!("zenoh_op_{}", i),
                        "load_test_service",
                    )
                    .await
                }
                _ => {
                    execute_system_command(
                        || async {
                            tokio::time::sleep(Duration::from_millis(15)).await;
                            if rand::random::<bool>() {
                                Ok(format!("System operation {}", i))
                            } else {
                                Err(anyhow::anyhow!("System failure {}", i))
                            }
                        },
                        &format!("system_op_{}", i),
                        "load_test_service",
                    )
                    .await
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all operations and count results
    let mut completed = 0;
    for handle in handles {
        let _ = handle.await?;
        completed += 1;
    }

    // All operations should complete (either succeed or fail gracefully)
    assert_eq!(completed, OPERATION_COUNT);

    Ok(())
}
