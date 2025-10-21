//! Simplified error handling utilities for the tcgui backend.
//!
//! This module provides basic error handling patterns including context enrichment
//! and simple retry mechanisms.

use anyhow::Result;
use std::time::Duration;
use tracing::{debug, error, warn};

/// Simple retry with exponential backoff
pub async fn retry_async<F, Fut, T, E>(
    operation: F,
    max_attempts: u32,
    initial_delay: Duration,
    backoff_multiplier: f32,
    operation_name: &str,
) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<T, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut delay = initial_delay;
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!(
                        "Operation '{}' succeeded after {} attempts",
                        operation_name, attempt
                    );
                }
                return Ok(result);
            }
            Err(err) => {
                warn!(
                    "Operation '{}' failed on attempt {}/{}: {}",
                    operation_name, attempt, max_attempts, err
                );

                last_error = Some(err);

                if attempt < max_attempts {
                    debug!("Retrying '{}' in {:?}", operation_name, delay);
                    tokio::time::sleep(delay).await;
                    delay = Duration::from_secs_f32(delay.as_secs_f32() * backoff_multiplier);
                }
            }
        }
    }

    error!(
        "Operation '{}' failed after {} attempts",
        operation_name, max_attempts
    );

    Err(anyhow::Error::from(last_error.unwrap())
        .context(format!("Failed after {} attempts", max_attempts)))
}
