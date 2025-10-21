//! Simplified service resilience utilities for the tcgui backend.
//!
//! This module provides basic retry mechanisms for reliable operation.

use crate::utils::error_handling::retry_async;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

/// Simple retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub backoff_multiplier: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        }
    }
}

/// Simple service resilience manager for retry policies
pub struct ServiceResilienceManager {
    /// Default retry configurations for different operation types
    retry_configs: Arc<Mutex<HashMap<String, RetryConfig>>>,
}

impl ServiceResilienceManager {
    /// Create a new service resilience manager
    pub fn new() -> Self {
        let mut retry_configs = HashMap::new();

        // Default retry configurations for common operations
        retry_configs.insert(
            "network".to_string(),
            RetryConfig {
                max_attempts: 3,
                initial_delay: Duration::from_millis(100),
                backoff_multiplier: 2.0,
            },
        );

        retry_configs.insert(
            "zenoh".to_string(),
            RetryConfig {
                max_attempts: 2,
                initial_delay: Duration::from_millis(50),
                backoff_multiplier: 2.0,
            },
        );

        retry_configs.insert(
            "tc_command".to_string(),
            RetryConfig {
                max_attempts: 2,
                initial_delay: Duration::from_millis(200),
                backoff_multiplier: 1.5,
            },
        );

        Self {
            retry_configs: Arc::new(Mutex::new(retry_configs)),
        }
    }

    /// Initialize the resilience manager
    pub fn initialize(&self) -> Result<()> {
        info!("Initialized service resilience manager");
        Ok(())
    }

    /// Get retry configuration for an operation type
    pub fn get_retry_config(&self, operation_type: &str) -> RetryConfig {
        let configs = self.retry_configs.lock().unwrap();
        configs
            .get(operation_type)
            .cloned()
            .unwrap_or_else(RetryConfig::default)
    }

    /// Execute operation with retry logic
    pub async fn execute_with_retry<F, Fut, T, E>(
        &self,
        operation: F,
        operation_type: &str,
        operation_name: &str,
    ) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<T, E>>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let config = self.get_retry_config(operation_type);
        retry_async(
            operation,
            config.max_attempts,
            config.initial_delay,
            config.backoff_multiplier,
            operation_name,
        )
        .await
    }
}

impl Default for ServiceResilienceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Global resilience manager instance
static RESILIENCE_MANAGER: std::sync::OnceLock<ServiceResilienceManager> =
    std::sync::OnceLock::new();

/// Get the global resilience manager instance
pub fn get_resilience_manager() -> &'static ServiceResilienceManager {
    RESILIENCE_MANAGER.get_or_init(|| {
        let manager = ServiceResilienceManager::new();
        if let Err(e) = manager.initialize() {
            tracing::error!("Failed to initialize resilience manager: {}", e);
        }
        manager
    })
}

/// Execute system command with resilience
pub async fn execute_system_command<F, Fut, T>(
    operation: F,
    operation_name: &str,
    _component: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let manager = get_resilience_manager();
    let wrapped_operation = || async {
        operation()
            .await
            .map_err(|e: anyhow::Error| std::io::Error::other(e.to_string()))
    };

    manager
        .execute_with_retry(wrapped_operation, "tc_command", operation_name)
        .await
}

/// Execute zenoh communication with resilience
pub async fn execute_zenoh_communication<F, Fut, T>(
    operation: F,
    operation_name: &str,
    _component: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let manager = get_resilience_manager();
    let wrapped_operation = || async {
        operation()
            .await
            .map_err(|e: anyhow::Error| std::io::Error::other(e.to_string()))
    };

    manager
        .execute_with_retry(wrapped_operation, "zenoh", operation_name)
        .await
}

/// Execute network discovery with resilience
pub async fn execute_network_discovery<F, Fut, T>(
    operation: F,
    operation_name: &str,
    _component: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let manager = get_resilience_manager();
    let wrapped_operation = || async {
        operation()
            .await
            .map_err(|e: anyhow::Error| std::io::Error::other(e.to_string()))
    };

    manager
        .execute_with_retry(wrapped_operation, "network", operation_name)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_resilience_manager_creation() {
        let manager = ServiceResilienceManager::new();
        assert!(manager.initialize().is_ok());
    }

    #[tokio::test]
    async fn test_retry_configuration() {
        let manager = ServiceResilienceManager::new();
        let config = manager.get_retry_config("network");
        assert_eq!(config.max_attempts, 3);

        // Test default configuration for unknown types
        let unknown_config = manager.get_retry_config("unknown");
        assert_eq!(unknown_config.max_attempts, 3);
    }

    #[tokio::test]
    async fn test_execute_with_retry_success() {
        let manager = ServiceResilienceManager::new();
        let attempts = std::sync::Arc::new(std::sync::Mutex::new(0));

        let result = manager
            .execute_with_retry(
                || {
                    let attempts = attempts.clone();
                    async move {
                        let mut count = attempts.lock().unwrap();
                        *count += 1;
                        let current_attempts = *count;
                        drop(count); // Release the lock

                        if current_attempts < 2 {
                            Result::<i32, std::io::Error>::Err(std::io::Error::other(
                                "Simulated failure",
                            ))
                        } else {
                            Ok(42)
                        }
                    }
                },
                "network",
                "test_operation",
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(*attempts.lock().unwrap(), 2);
    }
}
