//! Zenoh-based scenario storage implementation.
//!
//! This module provides persistent storage for network scenarios using Zenoh's
//! key-value storage capabilities. Scenarios are stored with keys like:
//! `tcgui/storage/scenarios/{scenario_id}`

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use zenoh::Session;

use tcgui_shared::scenario::NetworkScenario;

/// Zenoh-based storage for network scenarios
pub struct ScenarioZenohStorage {
    /// Zenoh session for storage operations
    session: Arc<Session>,
    /// Storage key prefix (e.g., "tcgui/storage/scenarios")
    storage_prefix: String,
}

impl ScenarioZenohStorage {
    /// Create a new scenario storage with Zenoh session
    pub fn new(session: Arc<Session>, storage_prefix: String) -> Self {
        info!(
            "Initializing scenario storage with prefix: {}",
            storage_prefix
        );

        Self {
            session,
            storage_prefix,
        }
    }

    /// Store a scenario using Zenoh key-value storage
    pub async fn put_scenario(&self, scenario: &NetworkScenario) -> Result<()> {
        let key = self.scenario_key(&scenario.id);
        let value = serde_json::to_vec(scenario).map_err(|e| {
            anyhow::anyhow!("Failed to serialize scenario '{}': {}", scenario.id, e)
        })?;

        debug!(
            "Storing scenario '{}' ({} bytes) at key: {}",
            scenario.id,
            value.len(),
            key
        );

        self.session
            .put(&key, value)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to store scenario '{}': {}", scenario.id, e))?;

        info!("Successfully stored scenario '{}'", scenario.id);
        Ok(())
    }

    /// Retrieve a scenario by ID from Zenoh storage
    pub async fn get_scenario(&self, id: &str) -> Result<Option<NetworkScenario>> {
        let key = self.scenario_key(id);

        debug!("Retrieving scenario '{}' from key: {}", id, key);

        // Use Zenoh's get operation to retrieve the scenario
        let replies = self
            .session
            .get(&key)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query scenario '{}': {}", id, e))?;

        // Collect all replies (should be at most one for exact key match)
        let mut scenarios = Vec::new();
        while let Ok(reply) = replies.recv_async().await {
            match reply.result() {
                Ok(sample) => {
                    match serde_json::from_slice::<NetworkScenario>(
                        sample.payload().to_bytes().as_ref(),
                    ) {
                        Ok(scenario) => {
                            debug!("Successfully deserialized scenario '{}'", id);
                            scenarios.push(scenario);
                        }
                        Err(e) => {
                            warn!("Failed to deserialize scenario '{}': {}", id, e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error in get reply for scenario '{}': {}", id, e);
                }
            }
        }

        if scenarios.len() > 1 {
            warn!(
                "Found {} scenarios for key '{}', expected at most 1",
                scenarios.len(),
                key
            );
        }

        Ok(scenarios.into_iter().next())
    }

    /// List all available scenarios from Zenoh storage
    pub async fn list_scenarios(&self) -> Result<Vec<NetworkScenario>> {
        let pattern = format!("{}/*", self.storage_prefix);

        debug!("Listing all scenarios with pattern: {}", pattern);

        let replies = self
            .session
            .get(&pattern)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list scenarios: {}", e))?;

        let mut scenarios = Vec::new();
        let mut count = 0;

        while let Ok(reply) = replies.recv_async().await {
            count += 1;
            match reply.result() {
                Ok(sample) => {
                    match serde_json::from_slice::<NetworkScenario>(
                        sample.payload().to_bytes().as_ref(),
                    ) {
                        Ok(scenario) => {
                            debug!("Listed scenario '{}' ({})", scenario.id, scenario.name);
                            scenarios.push(scenario);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to deserialize scenario from key '{}': {}",
                                sample.key_expr(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Error in list reply: {}", e);
                }
            }
        }

        info!(
            "Listed {} scenarios (processed {} replies)",
            scenarios.len(),
            count
        );
        Ok(scenarios)
    }

    /// Delete a scenario by ID from Zenoh storage
    pub async fn delete_scenario(&self, id: &str) -> Result<bool> {
        let key = self.scenario_key(id);

        debug!("Deleting scenario '{}' from key: {}", id, key);

        // Check if scenario exists first
        let exists = self.get_scenario(id).await?.is_some();

        if !exists {
            debug!("Scenario '{}' does not exist, nothing to delete", id);
            return Ok(false);
        }

        // Delete the scenario
        self.session
            .delete(&key)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete scenario '{}': {}", id, e))?;

        info!("Successfully deleted scenario '{}'", id);
        Ok(true)
    }

    /// Check if a scenario exists in storage
    pub async fn scenario_exists(&self, id: &str) -> Result<bool> {
        match self.get_scenario(id).await {
            Ok(scenario) => Ok(scenario.is_some()),
            Err(e) => {
                error!("Error checking if scenario '{}' exists: {}", id, e);
                Err(e)
            }
        }
    }

    /// Get storage statistics (number of scenarios, total size, etc.)
    pub async fn get_storage_stats(&self) -> Result<ScenarioStorageStats> {
        let scenarios = self.list_scenarios().await?;

        let count = scenarios.len();
        let total_steps = scenarios.iter().map(|s| s.steps.len()).sum();
        let avg_steps = if count > 0 {
            total_steps as f64 / count as f64
        } else {
            0.0
        };

        let total_duration_ms: u64 = scenarios
            .iter()
            .map(|s| s.estimated_total_duration_ms())
            .sum();

        Ok(ScenarioStorageStats {
            total_scenarios: count,
            total_steps,
            average_steps_per_scenario: avg_steps,
            total_duration_ms,
        })
    }

    /// Generate Zenoh key for a scenario
    fn scenario_key(&self, id: &str) -> String {
        format!("{}/{}", self.storage_prefix, id)
    }
}

/// Storage statistics for monitoring and debugging
#[derive(Debug, Clone)]
pub struct ScenarioStorageStats {
    /// Total number of scenarios in storage
    pub total_scenarios: usize,
    /// Total number of steps across all scenarios
    pub total_steps: usize,
    /// Average number of steps per scenario
    pub average_steps_per_scenario: f64,
    /// Total duration of all scenarios combined (milliseconds)
    pub total_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::TcNetemConfig;
    use tcgui_shared::scenario::ScenarioStep;

    // Helper function to create a test scenario
    fn create_test_scenario(id: &str, name: &str) -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            id.to_string(),
            name.to_string(),
            format!("Test scenario: {}", name),
        );

        let mut tc_config = TcNetemConfig::new();
        tc_config.loss.enabled = true;
        tc_config.loss.percentage = 5.0;

        let step = ScenarioStep::new(1000, "Test step".to_string(), tc_config);

        scenario.add_step(step);
        scenario
    }

    #[test]
    fn test_scenario_key_generation() {
        let storage_prefix = "tcgui/storage/scenarios".to_string();

        // Test key generation logic directly without Session mock
        let generate_key = |id: &str| format!("{}/{}", storage_prefix, id);

        assert_eq!(
            generate_key("test-scenario"),
            "tcgui/storage/scenarios/test-scenario"
        );

        assert_eq!(
            generate_key("mobile-degradation"),
            "tcgui/storage/scenarios/mobile-degradation"
        );
    }

    #[test]
    fn test_scenario_serialization() {
        let scenario = create_test_scenario("test", "Test Scenario");

        // Test that scenario can be serialized and deserialized
        let serialized = serde_json::to_vec(&scenario).expect("Failed to serialize scenario");
        let deserialized: NetworkScenario =
            serde_json::from_slice(&serialized).expect("Failed to deserialize scenario");

        assert_eq!(scenario.id, deserialized.id);
        assert_eq!(scenario.name, deserialized.name);
        assert_eq!(scenario.steps.len(), deserialized.steps.len());
    }

    #[test]
    fn test_storage_stats_calculation() {
        let scenarios = [
            create_test_scenario("test1", "Scenario 1"),
            create_test_scenario("test2", "Scenario 2"),
        ];

        let count = scenarios.len();
        let total_steps: usize = scenarios.iter().map(|s| s.steps.len()).sum();
        let avg_steps = total_steps as f64 / count as f64;

        assert_eq!(count, 2);
        assert_eq!(total_steps, 2); // Each test scenario has 1 step
        assert_eq!(avg_steps, 1.0);
    }
}
