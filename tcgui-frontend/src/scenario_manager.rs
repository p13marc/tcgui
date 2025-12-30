//! Scenario management for TC GUI frontend.
//!
//! This module provides the ScenarioManager that handles scenario-related state,
//! Zenoh queries, and coordination between the UI and backend scenario services.

use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use tcgui_shared::scenario::{
    NetworkScenario, ScenarioExecution, ScenarioExecutionRequest, ScenarioExecutionUpdate,
    ScenarioLoadError, ScenarioRequest,
};

use crate::messages::{ScenarioExecutionQueryMessage, ScenarioQueryMessage};

/// Tracked execution with timestamp for deduplication
#[derive(Clone, Debug)]
struct TrackedExecution {
    execution: ScenarioExecution,
    timestamp: u64,
}

/// Sort options for scenario list
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScenarioSortOption {
    #[default]
    Name,
    Duration,
    StepCount,
}

impl ScenarioSortOption {
    pub fn label(&self) -> &'static str {
        match self {
            ScenarioSortOption::Name => "Name",
            ScenarioSortOption::Duration => "Duration",
            ScenarioSortOption::StepCount => "Steps",
        }
    }

    pub fn all() -> &'static [ScenarioSortOption] {
        &[
            ScenarioSortOption::Name,
            ScenarioSortOption::Duration,
            ScenarioSortOption::StepCount,
        ]
    }
}

/// Manages scenario state and operations for the frontend
#[derive(Clone, Default)]
pub struct ScenarioManager {
    /// Available scenarios from backend
    available_scenarios: HashMap<String, Vec<NetworkScenario>>, // backend_name -> scenarios
    /// Currently active scenario executions with timestamps
    active_executions: HashMap<String, HashMap<String, TrackedExecution>>, // backend -> (namespace/interface -> tracked_execution)
    /// Currently selected scenario for details view
    selected_scenario: Option<NetworkScenario>,
    /// Whether scenario details are visible
    show_scenario_details: bool,
    /// Query channels for scenario operations
    scenario_query_sender: Option<mpsc::UnboundedSender<ScenarioQueryMessage>>,
    execution_query_sender: Option<mpsc::UnboundedSender<ScenarioExecutionQueryMessage>>,
    /// Search/filter text for scenario list
    search_filter: String,
    /// Current sort option
    sort_option: ScenarioSortOption,
    /// Whether sort is ascending
    sort_ascending: bool,
    /// Backends currently loading scenarios
    loading_backends: std::collections::HashSet<String>,
    /// Execution timelines that are collapsed (key: "backend/namespace/interface")
    collapsed_timelines: std::collections::HashSet<String>,
    /// Errors that occurred while loading scenario files
    load_errors: HashMap<String, Vec<ScenarioLoadError>>, // backend_name -> errors
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

    /// Get all available scenarios for a backend (raw, unfiltered)
    fn get_raw_scenarios(&self, backend_name: &str) -> Vec<NetworkScenario> {
        self.available_scenarios
            .get(backend_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Get available scenarios for a backend, filtered and sorted
    pub fn get_available_scenarios(&self, backend_name: &str) -> Vec<NetworkScenario> {
        let mut scenarios = self.get_raw_scenarios(backend_name);

        // Apply search filter
        if !self.search_filter.is_empty() {
            let filter_lower = self.search_filter.to_lowercase();
            scenarios.retain(|s| {
                s.name.to_lowercase().contains(&filter_lower)
                    || s.id.to_lowercase().contains(&filter_lower)
                    || s.description.to_lowercase().contains(&filter_lower)
                    || s.metadata
                        .tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&filter_lower))
            });
        }

        // Apply sorting
        match self.sort_option {
            ScenarioSortOption::Name => {
                scenarios.sort_by(|a, b| a.name.cmp(&b.name));
            }
            ScenarioSortOption::Duration => {
                scenarios.sort_by(|a, b| {
                    a.estimated_total_duration_ms()
                        .cmp(&b.estimated_total_duration_ms())
                });
            }
            ScenarioSortOption::StepCount => {
                scenarios.sort_by(|a, b| a.steps.len().cmp(&b.steps.len()));
            }
        }

        // Reverse if descending
        if !self.sort_ascending {
            scenarios.reverse();
        }

        scenarios
    }

    /// Get the current search filter
    pub fn get_search_filter(&self) -> &str {
        &self.search_filter
    }

    /// Set the search filter
    pub fn set_search_filter(&mut self, filter: String) {
        debug!("Setting search filter to: {}", filter);
        self.search_filter = filter;
    }

    /// Get the current sort option
    pub fn get_sort_option(&self) -> ScenarioSortOption {
        self.sort_option
    }

    /// Set the sort option
    pub fn set_sort_option(&mut self, option: ScenarioSortOption) {
        debug!("Setting sort option to: {:?}", option);
        if self.sort_option == option {
            // Toggle direction if same option
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_option = option;
            self.sort_ascending = true;
        }
    }

    /// Check if sort is ascending
    pub fn is_sort_ascending(&self) -> bool {
        self.sort_ascending
    }

    /// Check if a backend is currently loading scenarios
    pub fn is_loading(&self, backend_name: &str) -> bool {
        self.loading_backends.contains(backend_name)
    }

    /// Mark a backend as loading scenarios
    pub fn set_loading(&mut self, backend_name: &str, loading: bool) {
        if loading {
            self.loading_backends.insert(backend_name.to_string());
        } else {
            self.loading_backends.remove(backend_name);
        }
    }

    /// Get the count of raw (unfiltered) scenarios for a backend
    pub fn get_raw_scenario_count(&self, backend_name: &str) -> usize {
        self.available_scenarios
            .get(backend_name)
            .map(|s| s.len())
            .unwrap_or(0)
    }

    /// Get active executions for a backend
    pub fn get_active_executions(&self, backend_name: &str) -> Vec<ScenarioExecution> {
        self.active_executions
            .get(backend_name)
            .map(|executions| {
                executions
                    .values()
                    .map(|tracked| tracked.execution.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if there's an execution running on an interface
    pub fn is_execution_active(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> bool {
        let execution_key = format!("{}/{}", namespace, interface);
        self.active_executions
            .get(backend_name)
            .is_some_and(|executions| executions.contains_key(&execution_key))
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

    /// Toggle execution timeline visibility
    pub fn toggle_execution_timeline(
        &mut self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) {
        let key = format!("{}/{}/{}", backend_name, namespace, interface);
        if self.collapsed_timelines.contains(&key) {
            self.collapsed_timelines.remove(&key);
            debug!("Expanded timeline for {}", key);
        } else {
            self.collapsed_timelines.insert(key.clone());
            debug!("Collapsed timeline for {}", key);
        }
    }

    /// Check if execution timeline is collapsed
    pub fn is_timeline_collapsed(
        &self,
        backend_name: &str,
        namespace: &str,
        interface: &str,
    ) -> bool {
        let key = format!("{}/{}/{}", backend_name, namespace, interface);
        self.collapsed_timelines.contains(&key)
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
        loop_execution: bool,
    ) -> Result<(), String> {
        if let Some(sender) = &self.execution_query_sender {
            let request = ScenarioExecutionRequest::Start {
                scenario_id: scenario_id.to_string(),
                namespace: namespace.to_string(),
                interface: interface.to_string(),
                loop_execution,
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
                "Started scenario '{}' execution on {}:{} (loop: {})",
                scenario_id, namespace, interface, loop_execution
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
        load_errors: Vec<ScenarioLoadError>,
    ) {
        info!(
            "Received {} scenarios from backend: {} ({} load errors)",
            scenarios.len(),
            backend_name,
            load_errors.len()
        );
        self.loading_backends.remove(&backend_name);
        self.available_scenarios
            .insert(backend_name.clone(), scenarios);
        if load_errors.is_empty() {
            self.load_errors.remove(&backend_name);
        } else {
            self.load_errors.insert(backend_name, load_errors);
        }
    }

    /// Get load errors for a specific backend
    pub fn get_load_errors(&self, backend_name: &str) -> Option<&Vec<ScenarioLoadError>> {
        self.load_errors.get(backend_name)
    }

    /// Handle execution status update with timestamp-based deduplication
    pub fn handle_execution_update(&mut self, update: ScenarioExecutionUpdate) {
        let execution_key = format!("{}/{}", update.namespace, update.interface);

        let executions = self
            .active_executions
            .entry(update.backend_name.clone())
            .or_default();

        // Only update if this is a newer message (prevents stale Zenoh history messages)
        let should_update = match executions.get(&execution_key) {
            Some(existing) => update.timestamp >= existing.timestamp,
            None => true,
        };

        if should_update {
            debug!(
                "Updating execution status for {}: {} - {:?} (step {}, ts={})",
                execution_key,
                update.execution.scenario.id,
                update.execution.state,
                update.execution.current_step,
                update.timestamp
            );
            executions.insert(
                execution_key,
                TrackedExecution {
                    execution: update.execution,
                    timestamp: update.timestamp,
                },
            );
        } else {
            debug!(
                "Ignoring stale execution update for {}: ts={} < existing",
                execution_key, update.timestamp
            );
        }
    }

    /// Remove execution when it completes or is stopped
    pub fn remove_execution(&mut self, backend_name: &str, namespace: &str, interface: &str) {
        let execution_key = format!("{}/{}", namespace, interface);
        if let Some(executions) = self.active_executions.get_mut(backend_name)
            && executions.remove(&execution_key).is_some()
        {
            info!("Removed execution for {}", execution_key);
        }
    }

    /// Clean up backend state when backend disconnects
    pub fn cleanup_backend_state(&mut self, backend_name: &str) {
        info!("Cleaning up scenario state for backend: {}", backend_name);
        self.available_scenarios.remove(backend_name);
        self.active_executions.remove(backend_name);
    }

    /// Get statistics about scenario state
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

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::TcNetemConfig;
    use tcgui_shared::scenario::{ExecutionState, ExecutionStats, ScenarioMetadata, ScenarioStep};

    fn create_test_scenario(id: &str, name: &str, steps: usize) -> NetworkScenario {
        NetworkScenario {
            id: id.to_string(),
            name: name.to_string(),
            description: format!("Test scenario {}", id),
            metadata: ScenarioMetadata::default(),
            steps: (0..steps)
                .map(|i| ScenarioStep {
                    duration_ms: 1000,
                    tc_config: TcNetemConfig::default(),
                    description: format!("Step {}", i),
                })
                .collect(),
            loop_scenario: false,
            created_at: 0,
            modified_at: 0,
            cleanup_on_failure: true,
        }
    }

    fn create_test_execution(
        scenario_id: &str,
        step: usize,
        state: ExecutionState,
    ) -> ScenarioExecution {
        ScenarioExecution {
            scenario: create_test_scenario(scenario_id, scenario_id, 3),
            start_time: 0,
            current_step: step,
            state,
            target_namespace: "ns1".to_string(),
            target_interface: "eth0".to_string(),
            stats: ExecutionStats::default(),
            loop_execution: false,
            loop_iteration: 0,
        }
    }

    #[test]
    fn test_scenario_manager_default() {
        let manager = ScenarioManager::new();
        assert!(!manager.is_showing_details());
        assert!(manager.get_selected_scenario().is_none());
        assert!(manager.get_search_filter().is_empty());
        assert_eq!(manager.get_sort_option(), ScenarioSortOption::Name);
        // Default is descending (bool default is false)
        assert!(!manager.is_sort_ascending());
    }

    #[test]
    fn test_scenario_sort_options() {
        assert_eq!(ScenarioSortOption::Name.label(), "Name");
        assert_eq!(ScenarioSortOption::Duration.label(), "Duration");
        assert_eq!(ScenarioSortOption::StepCount.label(), "Steps");
        assert_eq!(ScenarioSortOption::all().len(), 3);
    }

    #[test]
    fn test_search_filter() {
        let mut manager = ScenarioManager::new();

        manager.set_search_filter("test".to_string());
        assert_eq!(manager.get_search_filter(), "test");

        manager.set_search_filter(String::new());
        assert!(manager.get_search_filter().is_empty());
    }

    #[test]
    fn test_sort_option_toggle() {
        let mut manager = ScenarioManager::new();

        // Initially Name descending (bool default is false)
        assert_eq!(manager.get_sort_option(), ScenarioSortOption::Name);
        assert!(!manager.is_sort_ascending());

        // Same option toggles direction
        manager.set_sort_option(ScenarioSortOption::Name);
        assert_eq!(manager.get_sort_option(), ScenarioSortOption::Name);
        assert!(manager.is_sort_ascending());

        // Different option resets to ascending
        manager.set_sort_option(ScenarioSortOption::Duration);
        assert_eq!(manager.get_sort_option(), ScenarioSortOption::Duration);
        assert!(manager.is_sort_ascending());
    }

    #[test]
    fn test_loading_state() {
        let mut manager = ScenarioManager::new();

        assert!(!manager.is_loading("backend1"));

        manager.set_loading("backend1", true);
        assert!(manager.is_loading("backend1"));
        assert!(!manager.is_loading("backend2"));

        manager.set_loading("backend1", false);
        assert!(!manager.is_loading("backend1"));
    }

    #[test]
    fn test_scenario_list_handling() {
        let mut manager = ScenarioManager::new();

        let scenarios = vec![
            create_test_scenario("scenario1", "Alpha", 2),
            create_test_scenario("scenario2", "Beta", 3),
            create_test_scenario("scenario3", "Gamma", 1),
        ];

        manager.handle_scenario_list_response("backend1".to_string(), scenarios, vec![]);

        assert_eq!(manager.get_raw_scenario_count("backend1"), 3);
        assert_eq!(manager.get_raw_scenario_count("backend2"), 0);

        let available = manager.get_available_scenarios("backend1");
        assert_eq!(available.len(), 3);
        // Default sort by name descending (bool default is false)
        assert_eq!(available[0].name, "Gamma");
        assert_eq!(available[1].name, "Beta");
        assert_eq!(available[2].name, "Alpha");
    }

    #[test]
    fn test_scenario_sorting() {
        let mut manager = ScenarioManager::new();

        let scenarios = vec![
            create_test_scenario("scenario1", "Beta", 2),
            create_test_scenario("scenario2", "Alpha", 5),
            create_test_scenario("scenario3", "Gamma", 1),
        ];

        manager.handle_scenario_list_response("backend1".to_string(), scenarios, vec![]);

        // Sort by name descending (default)
        let available = manager.get_available_scenarios("backend1");
        assert_eq!(available[0].name, "Gamma");

        // Toggle to ascending by clicking same option
        manager.set_sort_option(ScenarioSortOption::Name);
        let available = manager.get_available_scenarios("backend1");
        assert_eq!(available[0].name, "Alpha");

        // Sort by step count (resets to ascending)
        manager.set_sort_option(ScenarioSortOption::StepCount);
        let available = manager.get_available_scenarios("backend1");
        assert_eq!(available[0].steps.len(), 1); // Gamma
        assert_eq!(available[2].steps.len(), 5); // Alpha

        // Toggle to descending
        manager.set_sort_option(ScenarioSortOption::StepCount);
        let available = manager.get_available_scenarios("backend1");
        assert_eq!(available[0].steps.len(), 5); // Alpha now first
    }

    #[test]
    fn test_scenario_filtering() {
        let mut manager = ScenarioManager::new();

        let mut scenario_with_tag = create_test_scenario("scenario1", "Network Test", 2);
        scenario_with_tag.metadata.tags = vec!["production".to_string()];

        let scenarios = vec![
            scenario_with_tag,
            create_test_scenario("scenario2", "Other Scenario", 3),
        ];

        manager.handle_scenario_list_response("backend1".to_string(), scenarios, vec![]);

        // No filter - all scenarios
        assert_eq!(manager.get_available_scenarios("backend1").len(), 2);

        // Filter by name
        manager.set_search_filter("Network".to_string());
        assert_eq!(manager.get_available_scenarios("backend1").len(), 1);

        // Filter by tag
        manager.set_search_filter("production".to_string());
        assert_eq!(manager.get_available_scenarios("backend1").len(), 1);

        // Filter with no matches
        manager.set_search_filter("nonexistent".to_string());
        assert_eq!(manager.get_available_scenarios("backend1").len(), 0);
    }

    #[test]
    fn test_scenario_details() {
        let mut manager = ScenarioManager::new();

        let scenario = create_test_scenario("scenario1", "Test", 2);

        assert!(!manager.is_showing_details());
        assert!(manager.get_selected_scenario().is_none());

        manager.show_scenario_details(scenario.clone());
        assert!(manager.is_showing_details());
        assert_eq!(manager.get_selected_scenario().unwrap().id, "scenario1");

        manager.hide_scenario_details();
        assert!(!manager.is_showing_details());
        assert!(manager.get_selected_scenario().is_none());
    }

    #[test]
    fn test_execution_tracking() {
        let mut manager = ScenarioManager::new();

        assert!(!manager.is_execution_active("backend1", "ns1", "eth0"));

        // Add execution via update
        let update = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 0, ExecutionState::Running),
            timestamp: 1000,
        };

        manager.handle_execution_update(update);
        assert!(manager.is_execution_active("backend1", "ns1", "eth0"));
        assert!(!manager.is_execution_active("backend1", "ns1", "eth1"));

        let executions = manager.get_active_executions("backend1");
        assert_eq!(executions.len(), 1);
        assert_eq!(executions[0].scenario.id, "scenario1");
    }

    #[test]
    fn test_execution_update_deduplication() {
        let mut manager = ScenarioManager::new();

        // First update
        let update1 = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 0, ExecutionState::Running),
            timestamp: 1000,
        };
        manager.handle_execution_update(update1);

        // Newer update should be applied
        let update2 = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 1, ExecutionState::Running),
            timestamp: 2000,
        };
        manager.handle_execution_update(update2);

        let executions = manager.get_active_executions("backend1");
        assert_eq!(executions[0].current_step, 1);

        // Older update should be ignored
        let update3 = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 0, ExecutionState::Running),
            timestamp: 500,
        };
        manager.handle_execution_update(update3);

        let executions = manager.get_active_executions("backend1");
        assert_eq!(executions[0].current_step, 1); // Still at step 1
    }

    #[test]
    fn test_execution_removal() {
        let mut manager = ScenarioManager::new();

        let update = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 0, ExecutionState::Running),
            timestamp: 1000,
        };
        manager.handle_execution_update(update);

        assert!(manager.is_execution_active("backend1", "ns1", "eth0"));

        manager.remove_execution("backend1", "ns1", "eth0");
        assert!(!manager.is_execution_active("backend1", "ns1", "eth0"));
    }

    #[test]
    fn test_timeline_collapse() {
        let mut manager = ScenarioManager::new();

        assert!(!manager.is_timeline_collapsed("backend1", "ns1", "eth0"));

        manager.toggle_execution_timeline("backend1", "ns1", "eth0");
        assert!(manager.is_timeline_collapsed("backend1", "ns1", "eth0"));

        manager.toggle_execution_timeline("backend1", "ns1", "eth0");
        assert!(!manager.is_timeline_collapsed("backend1", "ns1", "eth0"));
    }

    #[test]
    fn test_cleanup_backend_state() {
        let mut manager = ScenarioManager::new();

        // Add scenarios and executions
        let scenarios = vec![create_test_scenario("scenario1", "Test", 2)];
        manager.handle_scenario_list_response("backend1".to_string(), scenarios, vec![]);

        let update = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 0, ExecutionState::Running),
            timestamp: 1000,
        };
        manager.handle_execution_update(update);

        assert_eq!(manager.get_raw_scenario_count("backend1"), 1);
        assert!(manager.is_execution_active("backend1", "ns1", "eth0"));

        // Cleanup
        manager.cleanup_backend_state("backend1");

        assert_eq!(manager.get_raw_scenario_count("backend1"), 0);
        assert!(!manager.is_execution_active("backend1", "ns1", "eth0"));
    }

    #[test]
    fn test_stats() {
        let mut manager = ScenarioManager::new();

        let scenarios = vec![
            create_test_scenario("scenario1", "Test1", 2),
            create_test_scenario("scenario2", "Test2", 3),
        ];
        manager.handle_scenario_list_response("backend1".to_string(), scenarios, vec![]);

        let update = ScenarioExecutionUpdate {
            backend_name: "backend1".to_string(),
            namespace: "ns1".to_string(),
            interface: "eth0".to_string(),
            execution: create_test_execution("scenario1", 0, ExecutionState::Running),
            timestamp: 1000,
        };
        manager.handle_execution_update(update);

        let stats = manager.get_stats();
        assert_eq!(stats.backend_count, 1);
        assert_eq!(stats.total_scenarios, 2);
        assert_eq!(stats.total_executions, 1);
        assert!(!stats.details_visible);

        // Test display
        let display = format!("{}", stats);
        assert!(display.contains("1 backends"));
        assert!(display.contains("2 scenarios"));
        assert!(display.contains("1 active executions"));
    }

    #[test]
    fn test_request_without_channel() {
        let manager = ScenarioManager::new();

        // Without channels set up, requests should fail gracefully
        assert!(manager.request_scenarios("backend1").is_err());
        assert!(
            manager
                .start_execution("backend1", "scenario1", "ns1", "eth0", false)
                .is_err()
        );
        assert!(manager.stop_execution("backend1", "ns1", "eth0").is_err());
        assert!(manager.pause_execution("backend1", "ns1", "eth0").is_err());
        assert!(manager.resume_execution("backend1", "ns1", "eth0").is_err());
    }
}
