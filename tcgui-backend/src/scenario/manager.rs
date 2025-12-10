//! Scenario Manager - High-level interface for scenario operations.
//!
//! This module provides the main ScenarioManager that coordinates between
//! storage, execution engine, and file-based scenario loading.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, instrument};
use zenoh::Session;

use tcgui_shared::scenario::NetworkScenario;

use super::{ScenarioExecutionEngine, ScenarioLoader, ScenarioZenohStorage};
use crate::tc_commands::TcCommandManager;

/// High-level scenario manager that coordinates all scenario operations
pub struct ScenarioManager {
    /// Storage backend for persisting scenarios
    storage: ScenarioZenohStorage,
    /// Execution engine for running scenarios
    execution_engine: ScenarioExecutionEngine,
    /// File-based scenario loader
    loader: ScenarioLoader,
    /// Cached templates loaded from files
    cached_templates: Vec<NetworkScenario>,
    /// Backend name for identification
    backend_name: String,
}

impl ScenarioManager {
    /// Create a new scenario manager with default scenario directories
    #[instrument(skip(session, tc_manager))]
    pub fn new(session: Arc<Session>, backend_name: String, tc_manager: TcCommandManager) -> Self {
        Self::with_options(session, backend_name, tc_manager, vec![], false)
    }

    /// Create a new scenario manager with additional scenario directories
    #[instrument(skip(session, tc_manager, extra_dirs))]
    pub fn with_scenario_dirs(
        session: Arc<Session>,
        backend_name: String,
        tc_manager: TcCommandManager,
        extra_dirs: Vec<PathBuf>,
    ) -> Self {
        Self::with_options(session, backend_name, tc_manager, extra_dirs, false)
    }

    /// Create a new scenario manager with full configuration options
    ///
    /// # Arguments
    /// * `session` - Zenoh session for storage and communication
    /// * `backend_name` - Name of this backend instance
    /// * `tc_manager` - TC command manager for executing network changes
    /// * `extra_dirs` - Additional directories to load scenarios from
    /// * `no_default_scenarios` - If true, skip default scenario directories
    #[instrument(skip(session, tc_manager, extra_dirs))]
    pub fn with_options(
        session: Arc<Session>,
        backend_name: String,
        tc_manager: TcCommandManager,
        extra_dirs: Vec<PathBuf>,
        no_default_scenarios: bool,
    ) -> Self {
        info!("Initializing ScenarioManager for backend: {}", backend_name);

        let storage_prefix = format!("tcgui/storage/{}/scenarios", backend_name);
        let storage = ScenarioZenohStorage::new(session.clone(), storage_prefix);

        let execution_engine =
            ScenarioExecutionEngine::new(session.clone(), backend_name.clone(), tc_manager);

        // Create loader, optionally skipping default directories
        let mut loader = ScenarioLoader::with_defaults(!no_default_scenarios);
        loader.add_directories(extra_dirs);

        // Load templates from files
        let cached_templates = loader.load_all();
        info!(
            "Loaded {} scenario templates from files",
            cached_templates.len()
        );

        Self {
            storage,
            execution_engine,
            loader,
            cached_templates,
            backend_name,
        }
    }

    /// Reload templates from disk
    pub fn reload_templates(&mut self) {
        self.cached_templates = self.loader.load_all();
        info!(
            "Reloaded {} scenario templates from files",
            self.cached_templates.len()
        );
    }

    /// Get storage statistics
    pub async fn get_storage_stats(
        &self,
    ) -> Result<crate::scenario::storage::ScenarioStorageStats> {
        self.storage.get_storage_stats().await
    }

    /// List all scenarios (both user and templates)
    pub async fn list_all_scenarios(&self) -> Result<Vec<NetworkScenario>> {
        let mut scenarios = self.storage.list_scenarios().await?;
        scenarios.extend(self.cached_templates.clone());
        Ok(scenarios)
    }

    /// Get a specific scenario by ID
    pub async fn get_scenario(&self, id: &str) -> Result<Option<NetworkScenario>> {
        // First check storage
        if let Some(scenario) = self.storage.get_scenario(id).await? {
            return Ok(Some(scenario));
        }

        // Then check cached templates
        Ok(self.cached_templates.iter().find(|s| s.id == id).cloned())
    }

    /// Store a new scenario
    pub async fn store_scenario(&self, scenario: NetworkScenario) -> Result<()> {
        self.storage.put_scenario(&scenario).await
    }

    /// Delete a scenario
    pub async fn delete_scenario(&self, id: &str) -> Result<bool> {
        self.storage.delete_scenario(id).await
    }

    /// Start executing a scenario on specified interface
    pub async fn start_scenario_execution(
        &self,
        scenario_id: &str,
        namespace: String,
        interface: String,
        loop_execution: bool,
    ) -> Result<String> {
        // Get the scenario from storage or templates
        let scenario = self
            .get_scenario(scenario_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Scenario '{}' not found", scenario_id))?;

        self.execution_engine
            .start_scenario(scenario, namespace, interface, loop_execution)
            .await
    }

    /// Stop scenario execution
    pub async fn stop_scenario_execution(&self, namespace: &str, interface: &str) -> Result<bool> {
        self.execution_engine
            .stop_scenario(namespace, interface)
            .await
    }

    /// Pause scenario execution
    pub async fn pause_scenario_execution(&self, namespace: &str, interface: &str) -> Result<bool> {
        self.execution_engine
            .pause_scenario(namespace, interface)
            .await
    }

    /// Resume scenario execution
    pub async fn resume_scenario_execution(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Result<bool> {
        self.execution_engine
            .resume_scenario(namespace, interface)
            .await
    }

    /// Get execution status
    pub async fn get_execution_status(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Option<tcgui_shared::scenario::ScenarioExecution> {
        self.execution_engine
            .get_execution_status(namespace, interface)
            .await
    }

    /// List all active executions
    pub async fn list_active_executions(&self) -> Vec<tcgui_shared::scenario::ScenarioExecution> {
        self.execution_engine.list_active_executions().await
    }

    /// Get backend name
    pub fn backend_name(&self) -> &str {
        &self.backend_name
    }

    /// Get the scenario loader (for accessing directory info)
    pub fn loader(&self) -> &ScenarioLoader {
        &self.loader
    }
}
