//! In-memory scenario store.
//!
//! Holds user-created scenarios for the life of the backend process. It
//! deliberately does **not** persist across restarts: the previous
//! implementation published to a `tcgui/storage/{backend}/scenarios/{id}` Zenoh
//! key, but nothing in the deployment configures a Zenoh storage plugin, so
//! every put went to the void and every get returned nothing — a silent
//! data-loss path sitting off-grammar in the identity chunk position (RFC
//! keyspace-v2 03 §1.2, the Sparkplug mistake). Durable, on-grammar persistence
//! moves to `state/tc/scenario/{id}` in the keyspace-v2 cutover; until then an
//! in-memory map is honest about its lifetime and keeps scenario CRUD working
//! within a session instead of silently discarding writes.

use anyhow::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{debug, info};

use tcgui_shared::scenario::NetworkScenario;

/// Process-lifetime store for network scenarios, keyed by scenario id.
#[derive(Default)]
pub struct ScenarioStore {
    scenarios: RwLock<HashMap<String, NetworkScenario>>,
}

impl ScenarioStore {
    /// Create an empty scenario store.
    pub fn new() -> Self {
        info!("Initializing in-memory scenario store");
        Self::default()
    }

    /// Store (insert or replace) a scenario.
    pub async fn put_scenario(&self, scenario: &NetworkScenario) -> Result<()> {
        debug!("Storing scenario '{}'", scenario.id);
        self.scenarios
            .write()
            .await
            .insert(scenario.id.clone(), scenario.clone());
        info!("Successfully stored scenario '{}'", scenario.id);
        Ok(())
    }

    /// Retrieve a scenario by id.
    pub async fn get_scenario(&self, id: &str) -> Result<Option<NetworkScenario>> {
        Ok(self.scenarios.read().await.get(id).cloned())
    }

    /// List all stored scenarios.
    pub async fn list_scenarios(&self) -> Result<Vec<NetworkScenario>> {
        Ok(self.scenarios.read().await.values().cloned().collect())
    }

    /// Delete a scenario by id. Returns `true` if it existed.
    pub async fn delete_scenario(&self, id: &str) -> Result<bool> {
        let existed = self.scenarios.write().await.remove(id).is_some();
        if existed {
            info!("Successfully deleted scenario '{}'", id);
        } else {
            debug!("Scenario '{}' does not exist, nothing to delete", id);
        }
        Ok(existed)
    }

    /// Check if a scenario exists.
    pub async fn scenario_exists(&self, id: &str) -> Result<bool> {
        Ok(self.scenarios.read().await.contains_key(id))
    }

    /// Compute aggregate statistics over the stored scenarios.
    pub async fn get_storage_stats(&self) -> Result<ScenarioStorageStats> {
        let scenarios = self.scenarios.read().await;
        let count = scenarios.len();
        let total_steps = scenarios.values().map(|s| s.steps.len()).sum();
        let average_steps_per_scenario = if count > 0 {
            total_steps as f64 / count as f64
        } else {
            0.0
        };
        let total_duration_ms = scenarios
            .values()
            .map(|s| s.estimated_total_duration_ms())
            .sum();

        Ok(ScenarioStorageStats {
            total_scenarios: count,
            total_steps,
            average_steps_per_scenario,
            total_duration_ms,
        })
    }
}

/// Storage statistics for monitoring and debugging.
#[derive(Debug, Clone)]
pub struct ScenarioStorageStats {
    /// Total number of scenarios in the store.
    pub total_scenarios: usize,
    /// Total number of steps across all scenarios.
    pub total_steps: usize,
    /// Average number of steps per scenario.
    pub average_steps_per_scenario: f64,
    /// Total duration of all scenarios combined (milliseconds).
    pub total_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::TcNetemConfig;
    use tcgui_shared::scenario::ScenarioStep;

    fn create_test_scenario(id: &str, name: &str) -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            id.to_string(),
            name.to_string(),
            format!("Test scenario: {name}"),
        );
        let mut tc_config = TcNetemConfig::new();
        tc_config.loss.enabled = true;
        tc_config.loss.percentage = 5.0;
        scenario.add_step(ScenarioStep::new(1000, "Test step".to_string(), tc_config));
        scenario
    }

    #[tokio::test]
    async fn put_get_list_delete_roundtrip() {
        let store = ScenarioStore::new();
        assert!(store.list_scenarios().await.unwrap().is_empty());

        let s = create_test_scenario("test", "Test Scenario");
        store.put_scenario(&s).await.unwrap();

        assert!(store.scenario_exists("test").await.unwrap());
        assert_eq!(
            store.get_scenario("test").await.unwrap().unwrap().id,
            "test"
        );
        assert_eq!(store.list_scenarios().await.unwrap().len(), 1);

        assert!(store.delete_scenario("test").await.unwrap());
        assert!(!store.delete_scenario("test").await.unwrap());
        assert!(store.get_scenario("test").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn stats_reflect_contents() {
        let store = ScenarioStore::new();
        store
            .put_scenario(&create_test_scenario("test1", "Scenario 1"))
            .await
            .unwrap();
        store
            .put_scenario(&create_test_scenario("test2", "Scenario 2"))
            .await
            .unwrap();

        let stats = store.get_storage_stats().await.unwrap();
        assert_eq!(stats.total_scenarios, 2);
        assert_eq!(stats.total_steps, 2);
        assert_eq!(stats.average_steps_per_scenario, 1.0);
    }
}
