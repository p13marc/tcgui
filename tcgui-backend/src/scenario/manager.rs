//! Scenario Manager - High-level interface for scenario operations.
//!
//! This module provides the main ScenarioManager that coordinates between
//! storage, execution engine, and file-based scenario loading.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, instrument, warn};
use zenoh::Session;

use tcgui_shared::identity::LocalOrigin;
use tcgui_shared::registry::tc;
use tcgui_shared::scenario::{NetworkScenario, ScenarioLoadError};

use super::{ScenarioExecutionEngine, ScenarioLoader, ScenarioStore};
use crate::tc_commands::TcCommandManager;

/// High-level scenario manager that coordinates all scenario operations
pub struct ScenarioManager {
    /// In-memory store for user-created scenarios (process lifetime)
    storage: ScenarioStore,
    /// Execution engine for running scenarios
    execution_engine: ScenarioExecutionEngine,
    /// File-based scenario loader
    loader: ScenarioLoader,
    /// Cached templates loaded from files
    cached_templates: Vec<NetworkScenario>,
    /// Cached load errors from last template load
    cached_load_errors: Vec<ScenarioLoadError>,
    /// Backend name for identification
    backend_name: String,
    /// Session used to publish the scenario library onto the state plane.
    session: Arc<Session>,
    /// This host's origin — every published key is built from it.
    local_origin: LocalOrigin,
}

impl ScenarioManager {
    /// Create a new scenario manager with default scenario directories
    #[instrument(skip(session, tc_manager))]
    pub fn new(
        session: Arc<Session>,
        local_origin: LocalOrigin,
        backend_name: String,
        tc_manager: TcCommandManager,
    ) -> Self {
        Self::with_options(
            session,
            local_origin,
            backend_name,
            tc_manager,
            vec![],
            false,
        )
    }

    /// Create a new scenario manager with additional scenario directories
    #[instrument(skip(session, tc_manager, extra_dirs))]
    pub fn with_scenario_dirs(
        session: Arc<Session>,
        local_origin: LocalOrigin,
        backend_name: String,
        tc_manager: TcCommandManager,
        extra_dirs: Vec<PathBuf>,
    ) -> Self {
        Self::with_options(
            session,
            local_origin,
            backend_name,
            tc_manager,
            extra_dirs,
            false,
        )
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
        local_origin: LocalOrigin,
        backend_name: String,
        tc_manager: TcCommandManager,
        extra_dirs: Vec<PathBuf>,
        no_default_scenarios: bool,
    ) -> Self {
        info!("Initializing ScenarioManager for backend: {}", backend_name);

        let storage = ScenarioStore::new();

        let execution_engine = ScenarioExecutionEngine::new(
            session.clone(),
            local_origin.clone(),
            backend_name.clone(),
            tc_manager,
        );

        // Create loader, optionally skipping default directories
        let mut loader = ScenarioLoader::with_defaults(!no_default_scenarios);
        loader.add_directories(extra_dirs);

        // Load templates from files
        let (cached_templates, cached_load_errors) = loader.load_all_with_errors();
        info!(
            "Loaded {} scenario templates from files ({} errors)",
            cached_templates.len(),
            cached_load_errors.len()
        );

        Self {
            storage,
            execution_engine,
            loader,
            cached_templates,
            cached_load_errors,
            backend_name,
            session,
            local_origin,
        }
    }

    /// Reload templates from disk
    pub fn reload_templates(&mut self) {
        let (templates, errors) = self.loader.load_all_with_errors();
        self.cached_templates = templates;
        self.cached_load_errors = errors;
        info!(
            "Reloaded {} scenario templates from files ({} errors)",
            self.cached_templates.len(),
            self.cached_load_errors.len()
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

    /// List all scenarios with any load errors that occurred
    pub async fn list_all_scenarios_with_errors(
        &self,
    ) -> Result<(Vec<NetworkScenario>, Vec<ScenarioLoadError>)> {
        let mut scenarios = self.storage.list_scenarios().await?;
        scenarios.extend(self.cached_templates.clone());
        Ok((scenarios, self.cached_load_errors.clone()))
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
        // Validate before persisting — rejects empty fields, over-long runs, and
        // oversized step counts from untrusted callers (#17).
        use tcgui_shared::TcValidate;
        scenario
            .validate()
            .map_err(|e| anyhow::anyhow!("invalid scenario: {e}"))?;

        // The id becomes a key chunk. Requiring it to be chunk-clean means
        // slugging is the identity, so the Put path (which keys on the raw id)
        // and the Delete path (which reads the id back off the key) cannot
        // disagree — see #66.
        if zenkey::Chunk::parse(&scenario.id).is_err() {
            return Err(anyhow::anyhow!(
                "invalid scenario id {:?}: must be a plain key chunk ([a-z0-9] with . _ - inside)",
                scenario.id
            ));
        }

        self.storage.put_scenario(&scenario).await?;
        self.publish_scenario(&scenario).await;
        Ok(())
    }

    /// Publish one scenario onto `state/tc/scenario/{id}`.
    ///
    /// A plain `put` rather than a declared publisher: every method here takes
    /// `&self` behind an `Arc`, so a publisher map would need a lock, and the
    /// late-joiner cache it would buy is already covered by the
    /// `ScenarioRequest::List` query the GUI issues on connect.
    async fn publish_scenario(&self, scenario: &NetworkScenario) {
        let key = tc::key(&self.local_origin, &tc::Subject::scenario(&scenario.id));
        match serde_json::to_string(scenario) {
            Ok(payload) => {
                if let Err(e) = self
                    .session
                    .put(key.as_keyexpr(), payload)
                    .encoding(zenoh::bytes::Encoding::APPLICATION_JSON)
                    .await
                {
                    warn!("Failed to publish scenario {} state: {e}", scenario.id);
                }
            }
            Err(e) => warn!("Failed to serialize scenario {}: {e}", scenario.id),
        }
    }

    /// Publish every scenario the backend knows about — file templates included.
    ///
    /// Publishing only user-created scenarios would be worse than publishing
    /// none: a GUI would then see a partial library on the state plane and a
    /// full one from `ScenarioRequest::List`, and which it showed would depend
    /// on which arrived last.
    pub async fn publish_all_scenarios(&self) {
        for scenario in &self.cached_templates {
            self.publish_scenario(scenario).await;
        }
        match self.storage.list_scenarios().await {
            Ok(scenarios) => {
                for scenario in &scenarios {
                    self.publish_scenario(scenario).await;
                }
            }
            Err(e) => warn!("Failed to list scenarios for publishing: {e}"),
        }
    }

    /// Delete a scenario
    pub async fn delete_scenario(&self, id: &str) -> Result<bool> {
        let removed = self.storage.delete_scenario(id).await?;
        if removed {
            // A removal is a Delete tombstone, never a None-payload Put: the
            // registry says "delete = removed", the GUI branches on is_delete,
            // and RFC 04 §1.2 forbids the payload encoding.
            let key = tc::key(&self.local_origin, &tc::Subject::scenario(id));
            if let Err(e) = self.session.delete(key.as_keyexpr()).await {
                warn!("Failed to publish scenario {id} tombstone: {e}");
            }
        }
        Ok(removed)
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
