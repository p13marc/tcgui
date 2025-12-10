//! Network Scenario data structures and types.
//!
//! This module defines the core data structures for the Network Scenario feature,
//! which allows users to define, manage, and execute dynamic network condition
//! changes over time. This simulates real-world network variations such as mobile
//! devices moving away from base stations, network congestion patterns, or
//! intermittent connectivity issues.

use crate::{TcNetemConfig, TcValidate, TcValidationError};
use serde::{Deserialize, Serialize};

/// Unique identifier for scenarios
pub type ScenarioId = String;

/// Network scenario definition containing a sequence of TC parameter changes over time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkScenario {
    /// Unique scenario identifier
    pub id: ScenarioId,
    /// Human-readable name
    pub name: String,
    /// Detailed description of what this scenario simulates
    pub description: String,
    /// Sequence of network condition changes
    pub steps: Vec<ScenarioStep>,
    /// Whether to loop the scenario when it completes
    pub loop_scenario: bool,
    /// Creation timestamp (Unix timestamp in seconds)
    pub created_at: u64,
    /// Last modification timestamp (Unix timestamp in seconds)
    pub modified_at: u64,
    /// Optional metadata for categorization and searching
    pub metadata: ScenarioMetadata,
    /// Whether to restore original TC configuration on failure/abort (default: true)
    #[serde(default = "default_cleanup_on_failure")]
    pub cleanup_on_failure: bool,
}

/// Default value for cleanup_on_failure (true)
fn default_cleanup_on_failure() -> bool {
    true
}

/// Metadata for scenario organization and searching
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScenarioMetadata {
    /// Category tags (e.g., "mobile", "congestion", "testing")
    pub tags: Vec<String>,
    /// Author or creator of the scenario
    pub author: Option<String>,
    /// Scenario version for tracking updates
    pub version: String,
    /// Expected duration in milliseconds (calculated from steps)
    pub duration_ms: u64,
}

/// Individual step in a network scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioStep {
    /// How long to maintain these settings (in milliseconds)
    pub duration_ms: u64,
    /// TC netem configuration to apply at this step
    pub tc_config: TcNetemConfig,
    /// Human-readable description of this step
    pub description: String,
}

/// Current execution state of a running scenario
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioExecution {
    /// The scenario being executed
    pub scenario: NetworkScenario,
    /// When execution started (Unix timestamp in milliseconds)
    pub start_time: u64,
    /// Current step index being executed
    pub current_step: usize,
    /// Current execution state
    pub state: ExecutionState,
    /// Target network namespace
    pub target_namespace: String,
    /// Target network interface
    pub target_interface: String,
    /// Execution statistics
    pub stats: ExecutionStats,
    /// Whether this execution should loop indefinitely
    #[serde(default)]
    pub loop_execution: bool,
    /// Current loop iteration (0-based, only relevant when loop_execution is true)
    #[serde(default)]
    pub loop_iteration: u32,
}

/// Execution statistics for monitoring and debugging
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionStats {
    /// Total number of steps completed
    pub steps_completed: usize,
    /// Total number of TC apply operations performed
    pub tc_operations: usize,
    /// Number of failed TC operations
    pub failed_operations: usize,
    /// Last error message (if any)
    pub last_error: Option<String>,
    /// Execution progress percentage (0.0-100.0)
    pub progress_percent: f32,
}

/// Current state of scenario execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionState {
    /// Scenario is actively running
    Running,
    /// Scenario is temporarily paused
    Paused {
        /// When the pause occurred (Unix timestamp in milliseconds)
        paused_at: u64,
    },
    /// Scenario execution has been stopped
    Stopped,
    /// Scenario has completed successfully
    Completed,
    /// Scenario failed due to an error
    Failed {
        /// Error message describing the failure
        error: String,
    },
}

/// Scenario management request messages (Query/Reply pattern)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioRequest {
    /// Add a new scenario
    Add(NetworkScenario),
    /// Remove a scenario by ID
    Remove { id: ScenarioId },
    /// List all available scenarios
    List,
    /// Get a specific scenario by ID
    Get { id: ScenarioId },
    /// Update an existing scenario
    Update(NetworkScenario),
}

/// Response to scenario management requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioResponse {
    /// Scenario was successfully added
    Added { id: ScenarioId },
    /// Scenario was successfully removed
    Removed { success: bool },
    /// List of all scenarios
    Listed { scenarios: Vec<NetworkScenario> },
    /// Retrieved scenario (None if not found)
    Retrieved { scenario: Option<NetworkScenario> },
    /// Scenario was successfully updated
    Updated { success: bool },
    /// Operation failed with error message
    Error { message: String },
}

/// Scenario execution control request messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioExecutionRequest {
    /// Start executing a scenario on specified interface
    Start {
        scenario_id: ScenarioId,
        namespace: String,
        interface: String,
        /// Whether to loop the scenario indefinitely (overrides scenario's loop_scenario field)
        #[serde(default)]
        loop_execution: bool,
    },
    /// Stop execution on specified interface
    Stop {
        namespace: String,
        interface: String,
    },
    /// Pause execution on specified interface
    Pause {
        namespace: String,
        interface: String,
    },
    /// Resume paused execution on specified interface
    Resume {
        namespace: String,
        interface: String,
    },
    /// Get execution status for specified interface
    Status {
        namespace: String,
        interface: String,
    },
    /// List all active executions
    ListActive,
}

/// Response to scenario execution requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioExecutionResponse {
    /// Execution started successfully
    Started {
        execution_id: String,
        estimated_duration_ms: u64,
    },
    /// Execution stopped successfully
    Stopped { success: bool },
    /// Execution paused successfully
    Paused { success: bool },
    /// Execution resumed successfully
    Resumed { success: bool },
    /// Current execution status
    Status {
        execution: Box<Option<ScenarioExecution>>,
    },
    /// List of active executions
    ActiveExecutions { executions: Vec<ScenarioExecution> },
    /// Operation failed with error message
    Error { message: String },
}

/// Scenario execution status update (Pub/Sub)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioExecutionUpdate {
    /// Target namespace
    pub namespace: String,
    /// Target interface
    pub interface: String,
    /// Current execution status
    pub execution: ScenarioExecution,
    /// Backend that generated this update
    pub backend_name: String,
    /// Update timestamp
    pub timestamp: u64,
}

/// Validation trait implementation for scenario data structures
impl TcValidate for NetworkScenario {
    type Error = ScenarioValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Validate basic fields
        if self.id.is_empty() {
            return Err(ScenarioValidationError::EmptyField("id".to_string()));
        }
        if self.name.is_empty() {
            return Err(ScenarioValidationError::EmptyField("name".to_string()));
        }
        if self.steps.is_empty() {
            return Err(ScenarioValidationError::EmptyField("steps".to_string()));
        }

        // Validate steps
        for (index, step) in self.steps.iter().enumerate() {
            step.validate()
                .map_err(|e| ScenarioValidationError::StepValidation {
                    step_index: index,
                    error: e,
                })?;
        }

        // Validate total duration is reasonable (not too long)
        let max_duration_ms = 24 * 60 * 60 * 1000; // 24 hours
        let total_duration: u64 = self.steps.iter().map(|s| s.duration_ms).sum();
        if total_duration > max_duration_ms {
            return Err(ScenarioValidationError::InvalidDuration {
                duration_ms: total_duration,
                max_duration_ms,
            });
        }

        Ok(())
    }
}

impl TcValidate for ScenarioStep {
    type Error = ScenarioStepValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Validate TC configuration
        self.tc_config
            .validate()
            .map_err(ScenarioStepValidationError::TcConfigError)?;

        // Validate description
        if self.description.is_empty() {
            return Err(ScenarioStepValidationError::EmptyDescription);
        }

        // Validate duration
        if self.duration_ms == 0 {
            return Err(ScenarioStepValidationError::InvalidDuration(0));
        }

        Ok(())
    }
}

/// Scenario validation errors
#[derive(Debug, Clone)]
pub enum ScenarioValidationError {
    EmptyField(String),
    StepValidation {
        step_index: usize,
        error: ScenarioStepValidationError,
    },
    InvalidDuration {
        duration_ms: u64,
        max_duration_ms: u64,
    },
}

impl std::fmt::Display for ScenarioValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioValidationError::EmptyField(field) => {
                write!(f, "Scenario field '{}' cannot be empty", field)
            }
            ScenarioValidationError::StepValidation { step_index, error } => {
                write!(f, "Validation error in step {}: {}", step_index, error)
            }
            ScenarioValidationError::InvalidDuration {
                duration_ms,
                max_duration_ms,
            } => {
                write!(
                    f,
                    "Scenario duration {}ms exceeds maximum {}ms",
                    duration_ms, max_duration_ms
                )
            }
        }
    }
}

impl std::error::Error for ScenarioValidationError {}

/// Scenario step validation errors
#[derive(Debug, Clone)]
pub enum ScenarioStepValidationError {
    TcConfigError(TcValidationError),
    EmptyDescription,
    InvalidDuration(u64),
}

impl std::fmt::Display for ScenarioStepValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioStepValidationError::TcConfigError(e) => {
                write!(f, "TC configuration error: {}", e)
            }
            ScenarioStepValidationError::EmptyDescription => {
                write!(f, "Step description cannot be empty")
            }
            ScenarioStepValidationError::InvalidDuration(duration) => {
                write!(
                    f,
                    "Invalid step duration: {}ms (must be greater than 0)",
                    duration
                )
            }
        }
    }
}

impl std::error::Error for ScenarioStepValidationError {}

impl NetworkScenario {
    /// Create a new scenario with current timestamps
    pub fn new(id: ScenarioId, name: String, description: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            name,
            description,
            steps: Vec::new(),
            loop_scenario: false,
            created_at: now,
            modified_at: now,
            metadata: ScenarioMetadata::default(),
            cleanup_on_failure: true,
        }
    }

    /// Add a step to the scenario
    pub fn add_step(&mut self, step: ScenarioStep) {
        self.steps.push(step);
        self.update_modified_time();
        self.recalculate_duration();
    }

    /// Update the modified timestamp to current time
    pub fn update_modified_time(&mut self) {
        self.modified_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Calculate and update the total scenario duration
    pub fn recalculate_duration(&mut self) {
        self.metadata.duration_ms = self.steps.iter().map(|step| step.duration_ms).sum();
    }

    /// Get estimated total duration (sum of all step durations)
    pub fn estimated_total_duration_ms(&self) -> u64 {
        self.steps.iter().map(|step| step.duration_ms).sum()
    }
}

impl ScenarioStep {
    /// Create a new scenario step
    pub fn new(duration_ms: u64, description: String, tc_config: TcNetemConfig) -> Self {
        Self {
            duration_ms,
            tc_config,
            description,
        }
    }
}

impl ScenarioExecution {
    /// Calculate current progress percentage (0.0-100.0)
    pub fn calculate_progress(&self) -> f32 {
        match self.state {
            ExecutionState::Completed => 100.0,
            ExecutionState::Failed { .. } => self.stats.progress_percent,
            ExecutionState::Stopped => self.stats.progress_percent,
            ExecutionState::Running | ExecutionState::Paused { .. } => {
                if self.scenario.steps.is_empty() {
                    return 100.0;
                }

                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let elapsed_ms = current_time.saturating_sub(self.start_time);
                let total_duration = self.scenario.estimated_total_duration_ms();

                if total_duration == 0 {
                    return 100.0;
                }

                ((elapsed_ms as f32 / total_duration as f32) * 100.0).min(100.0)
            }
        }
    }

    /// Check if execution is currently active (running or paused)
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            ExecutionState::Running | ExecutionState::Paused { .. }
        )
    }

    /// Get execution key for identifying this execution
    pub fn execution_key(&self) -> String {
        format!("{}/{}", self.target_namespace, self.target_interface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_creation() {
        let scenario = NetworkScenario::new(
            "test-scenario".to_string(),
            "Test Scenario".to_string(),
            "A test scenario for validation".to_string(),
        );

        assert_eq!(scenario.id, "test-scenario");
        assert_eq!(scenario.name, "Test Scenario");
        assert_eq!(scenario.description, "A test scenario for validation");
        assert!(scenario.steps.is_empty());
        assert!(!scenario.loop_scenario);
        assert!(scenario.created_at > 0);
        assert_eq!(scenario.created_at, scenario.modified_at);
    }

    #[test]
    fn test_scenario_step_creation() {
        let mut tc_config = TcNetemConfig::new();
        tc_config.loss.enabled = true;
        tc_config.loss.percentage = 5.0;

        let step = ScenarioStep::new(30000, "Initial step".to_string(), tc_config);

        assert_eq!(step.duration_ms, 30000);
        assert_eq!(step.description, "Initial step");
    }

    #[test]
    fn test_scenario_validation_empty_fields() {
        let mut scenario = NetworkScenario::new("".to_string(), "".to_string(), "desc".to_string());

        let result = scenario.validate();
        assert!(result.is_err());

        if let Err(ScenarioValidationError::EmptyField(field)) = result {
            assert_eq!(field, "id");
        }

        scenario.id = "valid-id".to_string();
        let result = scenario.validate();
        assert!(result.is_err());

        if let Err(ScenarioValidationError::EmptyField(field)) = result {
            assert_eq!(field, "name");
        }
    }

    #[test]
    fn test_scenario_validation_empty_steps() {
        let scenario = NetworkScenario::new(
            "test".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );

        let result = scenario.validate();
        assert!(result.is_err());

        if let Err(ScenarioValidationError::EmptyField(field)) = result {
            assert_eq!(field, "steps");
        }
    }

    #[test]
    fn test_scenario_step_validation() {
        let mut tc_config = TcNetemConfig::new();
        tc_config.loss.enabled = true;
        tc_config.loss.percentage = 150.0; // Invalid percentage

        let step = ScenarioStep::new(1000, "Invalid step".to_string(), tc_config);

        let result = step.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_scenario_step_validation_empty_description() {
        let tc_config = TcNetemConfig::new();
        let step = ScenarioStep::new(1000, "".to_string(), tc_config);

        let result = step.validate();
        assert!(result.is_err());

        if let Err(ScenarioStepValidationError::EmptyDescription) = result {
            // Expected error
        } else {
            panic!("Expected EmptyDescription error");
        }
    }

    #[test]
    fn test_scenario_step_validation_zero_duration() {
        let tc_config = TcNetemConfig::new();
        let step = ScenarioStep::new(0, "Zero duration".to_string(), tc_config);

        let result = step.validate();
        assert!(result.is_err());

        if let Err(ScenarioStepValidationError::InvalidDuration(0)) = result {
            // Expected error
        } else {
            panic!("Expected InvalidDuration error");
        }
    }

    #[test]
    fn test_scenario_duration_calculation() {
        let mut scenario = NetworkScenario::new(
            "test".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );

        let tc_config = TcNetemConfig::new();
        scenario.add_step(ScenarioStep::new(
            10000,
            "Step 1".to_string(),
            tc_config.clone(),
        ));
        scenario.add_step(ScenarioStep::new(
            20000,
            "Step 2".to_string(),
            tc_config.clone(),
        ));
        scenario.add_step(ScenarioStep::new(30000, "Step 3".to_string(), tc_config));

        // Total duration is sum of all step durations: 10000 + 20000 + 30000 = 60000
        assert_eq!(scenario.metadata.duration_ms, 60000);
        assert_eq!(scenario.estimated_total_duration_ms(), 60000);
    }

    #[test]
    fn test_scenario_execution_progress() {
        let scenario = NetworkScenario::new(
            "test".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );

        let mut execution = ScenarioExecution {
            scenario,
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            current_step: 0,
            state: ExecutionState::Running,
            target_namespace: "default".to_string(),
            target_interface: "eth0".to_string(),
            stats: ExecutionStats::default(),
            loop_execution: false,
            loop_iteration: 0,
        };

        // Should be 100% for empty scenario
        assert_eq!(execution.calculate_progress(), 100.0);

        // Test completed state
        execution.state = ExecutionState::Completed;
        assert_eq!(execution.calculate_progress(), 100.0);

        // Test failed state with partial progress
        execution.state = ExecutionState::Failed {
            error: "Test error".to_string(),
        };
        execution.stats.progress_percent = 50.0;
        assert_eq!(execution.calculate_progress(), 50.0);
    }

    #[test]
    fn test_scenario_execution_active_state() {
        let scenario = NetworkScenario::new(
            "test".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );

        let mut execution = ScenarioExecution {
            scenario,
            start_time: 0,
            current_step: 0,
            state: ExecutionState::Running,
            target_namespace: "default".to_string(),
            target_interface: "eth0".to_string(),
            stats: ExecutionStats::default(),
            loop_execution: false,
            loop_iteration: 0,
        };

        assert!(execution.is_active());

        execution.state = ExecutionState::Paused { paused_at: 12345 };
        assert!(execution.is_active());

        execution.state = ExecutionState::Completed;
        assert!(!execution.is_active());

        execution.state = ExecutionState::Stopped;
        assert!(!execution.is_active());

        execution.state = ExecutionState::Failed {
            error: "Test".to_string(),
        };
        assert!(!execution.is_active());
    }

    #[test]
    fn test_execution_key_generation() {
        let scenario = NetworkScenario::new(
            "test".to_string(),
            "Test".to_string(),
            "Description".to_string(),
        );

        let execution = ScenarioExecution {
            scenario,
            start_time: 0,
            current_step: 0,
            state: ExecutionState::Running,
            target_namespace: "test-namespace".to_string(),
            target_interface: "eth1".to_string(),
            stats: ExecutionStats::default(),
            loop_execution: false,
            loop_iteration: 0,
        };

        assert_eq!(execution.execution_key(), "test-namespace/eth1");
    }
}
