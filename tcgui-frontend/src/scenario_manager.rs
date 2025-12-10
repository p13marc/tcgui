//! Scenario management for TC GUI frontend.
//!
//! This module provides the ScenarioManager that handles scenario-related state,
//! Zenoh queries, and coordination between the UI and backend scenario services.

use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use tcgui_shared::scenario::{
    NetworkScenario, ScenarioExecution, ScenarioExecutionRequest, ScenarioRequest,
};

use crate::messages::{ScenarioExecutionQueryMessage, ScenarioQueryMessage};

/// Manages scenario state and operations for the frontend
#[derive(Clone, Default)]
pub struct ScenarioManager {
    /// Available scenarios from backend
    available_scenarios: HashMap<String, Vec<NetworkScenario>>, // backend_name -> scenarios
    /// Currently active scenario executions
    active_executions: HashMap<String, HashMap<String, ScenarioExecution>>, // backend -> (namespace/interface -> execution)
    /// Currently selected scenario for details view
    selected_scenario: Option<NetworkScenario>,
    /// Whether scenario details are visible
    show_scenario_details: bool,
    /// Query channels for scenario operations
    scenario_query_sender: Option<mpsc::UnboundedSender<ScenarioQueryMessage>>,
    execution_query_sender: Option<mpsc::UnboundedSender<ScenarioExecutionQueryMessage>>,
}

impl ScenarioManager {
    /// Create a new scenario manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Set up the scenario query channel
    pub fn setup_scenario_query_channel(
        &mut self,
        sender: mpsc::UnboundedSender<ScenarioQueryMessage>,
    ) {
        info!("Setting up scenario query channel");
        self.scenario_query_sender = Some(sender);
    }

    /// Set up the scenario execution query channel
    pub fn setup_execution_query_channel(
        &mut self,
        sender: mpsc::UnboundedSender<ScenarioExecutionQueryMessage>,
    ) {
        info!("Setting up scenario execution query channel");
        self.execution_query_sender = Some(sender);
    }

    /// Get all available scenarios for a backend
    pub fn get_available_scenarios(&self, backend_name: &str) -> Vec<NetworkScenario> {
        self.available_scenarios
            .get(backend_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Get active executions for a backend
    pub fn get_active_executions(&self, backend_name: &str) -> Vec<ScenarioExecution> {
        self.active_executions
            .get(backend_name)
            .map(|executions| executions.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get execution status for a specific interface
    #[allow(dead_code)]
    pub fn get_execution_status(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> Option<ScenarioExecution> {
        let execution_key = format!("{}/{}", namespace, interface);
        self.active_executions
            .get(backend_name)
            .and_then(|executions| executions.get(&execution_key).cloned())
    }

    /// Check if there's an execution running on an interface
    #[allow(dead_code)] // TODO: Wire up scenario UI
    pub fn is_execution_active(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> bool {
        self.get_execution_status(backend_name, namespace, interface)
            .is_some()
    }

    /// Get currently selected scenario
    pub fn get_selected_scenario(&self) -> Option<&NetworkScenario> {
        self.selected_scenario.as_ref()
    }

    /// Check if scenario details are shown
    pub fn is_showing_details(&self) -> bool {
        self.show_scenario_details
    }

    /// Show scenario details
    pub fn show_scenario_details(&mut self, scenario: NetworkScenario) {
        info!("Showing details for scenario: {}", scenario.id);
        self.selected_scenario = Some(scenario);
        self.show_scenario_details = true;
    }

    /// Hide scenario details
    pub fn hide_scenario_details(&mut self) {
        debug!("Hiding scenario details");
        self.show_scenario_details = false;
        self.selected_scenario = None;
    }

    /// Request list of all scenarios from backend
    pub fn request_scenarios(&self, backend_name: &str) -> Result<(), String> {
        if let Some(sender) = &self.scenario_query_sender {
            let request = ScenarioRequest::List;
            let message = ScenarioQueryMessage {
                backend_name: backend_name.to_string(),
                request,
                response_sender: None, // Responses will be handled by ZenohManager
            };

            if let Err(e) = sender.send(message) {
                error!("Failed to send scenario list request: {}", e);
                return Err(format!("Failed to request scenarios: {}", e));
            }

            info!("Requested scenarios from backend: {}", backend_name);
            Ok(())
        } else {
            warn!("Scenario query channel not available");
            Err("Scenario query channel not available".to_string())
        }
    }

    /// Start scenario execution
    pub fn start_execution(
        &self,
        backend_name: &str,
        scenario_id: &str,
        namespace: &str,
        interface: &str,
    ) -> Result<(), String> {
        if let Some(sender) = &self.execution_query_sender {
            let request = ScenarioExecutionRequest::Start {
                scenario_id: scenario_id.to_string(),
                namespace: namespace.to_string(),
                interface: interface.to_string(),
            };

            let message = ScenarioExecutionQueryMessage {
                backend_name: backend_name.to_string(),
                request,
                response_sender: None,
            };

            if let Err(e) = sender.send(message) {
                error!("Failed to send execution start request: {}", e);
                return Err(format!("Failed to start execution: {}", e));
            }

            info!(
                "Started scenario '{}' execution on {}:{}",
                scenario_id, namespace, interface
            );
            Ok(())
        } else {
            warn!("Execution query channel not available");
            Err("Execution query channel not available".to_string())
        }
    }

    /// Stop scenario execution
    pub fn stop_execution(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> Result<(), String> {
        if let Some(sender) = &self.execution_query_sender {
            let request = ScenarioExecutionRequest::Stop {
                namespace: namespace.to_string(),
                interface: interface.to_string(),
            };

            let message = ScenarioExecutionQueryMessage {
                backend_name: backend_name.to_string(),
                request,
                response_sender: None,
            };

            if let Err(e) = sender.send(message) {
                error!("Failed to send execution stop request: {}", e);
                return Err(format!("Failed to stop execution: {}", e));
            }

            info!("Stopped scenario execution on {}:{}", namespace, interface);
            Ok(())
        } else {
            warn!("Execution query channel not available");
            Err("Execution query channel not available".to_string())
        }
    }

    /// Pause scenario execution
    pub fn pause_execution(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> Result<(), String> {
        if let Some(sender) = &self.execution_query_sender {
            let request = ScenarioExecutionRequest::Pause {
                namespace: namespace.to_string(),
                interface: interface.to_string(),
            };

            let message = ScenarioExecutionQueryMessage {
                backend_name: backend_name.to_string(),
                request,
                response_sender: None,
            };

            if let Err(e) = sender.send(message) {
                error!("Failed to send execution pause request: {}", e);
                return Err(format!("Failed to pause execution: {}", e));
            }

            info!("Paused scenario execution on {}:{}", namespace, interface);
            Ok(())
        } else {
            warn!("Execution query channel not available");
            Err("Execution query channel not available".to_string())
        }
    }

    /// Resume scenario execution
    pub fn resume_execution(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> Result<(), String> {
        if let Some(sender) = &self.execution_query_sender {
            let request = ScenarioExecutionRequest::Resume {
                namespace: namespace.to_string(),
                interface: interface.to_string(),
            };

            let message = ScenarioExecutionQueryMessage {
                backend_name: backend_name.to_string(),
                request,
                response_sender: None,
            };

            if let Err(e) = sender.send(message) {
                error!("Failed to send execution resume request: {}", e);
                return Err(format!("Failed to resume execution: {}", e));
            }

            info!("Resumed scenario execution on {}:{}", namespace, interface);
            Ok(())
        } else {
            warn!("Execution query channel not available");
            Err("Execution query channel not available".to_string())
        }
    }

    /// Handle scenario list response
    pub fn handle_scenario_list_response(
        &mut self,
        backend_name: String,
        scenarios: Vec<NetworkScenario>,
    ) {
        info!(
            "Received {} scenarios from backend: {}",
            scenarios.len(),
            backend_name
        );
        self.available_scenarios.insert(backend_name, scenarios);
    }

    /// Handle execution status update
    pub fn handle_execution_update(
        &mut self,
        backend_name: String,
        namespace: String,
        interface: String,
        execution: ScenarioExecution,
    ) {
        let execution_key = format!("{}/{}", namespace, interface);
        debug!(
            "Updating execution status for {}: {} - {:?}",
            execution_key, execution.scenario.id, execution.state
        );

        self.active_executions
            .entry(backend_name)
            .or_default()
            .insert(execution_key, execution);
    }

    /// Remove execution when it completes or is stopped
    #[allow(dead_code)]
    pub fn remove_execution(&mut self, backend_name: &str, namespace: &str, interface: &str) {
        let execution_key = format!("{}/{}", namespace, interface);
        if let Some(executions) = self.active_executions.get_mut(backend_name) {
            if executions.remove(&execution_key).is_some() {
                info!("Removed execution for {}", execution_key);
            }
        }
    }

    /// Clean up backend state when backend disconnects
    #[allow(dead_code)]
    pub fn cleanup_backend_state(&mut self, backend_name: &str) {
        info!("Cleaning up scenario state for backend: {}", backend_name);
        self.available_scenarios.remove(backend_name);
        self.active_executions.remove(backend_name);
    }

    /// Get statistics about scenario state
    #[allow(dead_code)]
    pub fn get_stats(&self) -> ScenarioManagerStats {
        let total_scenarios: usize = self.available_scenarios.values().map(|v| v.len()).sum();
        let total_executions: usize = self
            .active_executions
            .values()
            .map(|executions| executions.len())
            .sum();

        ScenarioManagerStats {
            backend_count: self.available_scenarios.len(),
            total_scenarios,
            total_executions,
            details_visible: self.show_scenario_details,
        }
    }
}

/// Statistics about scenario manager state
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScenarioManagerStats {
    pub backend_count: usize,
    pub total_scenarios: usize,
    pub total_executions: usize,
    pub details_visible: bool,
}

impl std::fmt::Display for ScenarioManagerStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Scenario Stats: {} backends, {} scenarios, {} active executions{}",
            self.backend_count,
            self.total_scenarios,
            self.total_executions,
            if self.details_visible {
                " (details visible)"
            } else {
                ""
            }
        )
    }
}
