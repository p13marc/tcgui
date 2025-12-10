//! Scenario execution engine for dynamic network condition changes over time.
//!
//! This module provides the core execution engine for running network scenarios,
//! including precise timing control, parameter interpolation, and state management.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, instrument, warn};
use zenoh::Session;

use tcgui_shared::scenario::{ExecutionState, ExecutionStats, NetworkScenario, ScenarioExecution};
use tcgui_shared::{topics, TcOperation, TcRequest, TcResponse};

use crate::tc_commands::TcCommandManager;

/// Execution engine for managing scenario playback across multiple interfaces
pub struct ScenarioExecutionEngine {
    /// Active scenario executions (keyed by namespace/interface)
    active_executions: Arc<RwLock<HashMap<String, ScenarioExecutor>>>,
    /// Zenoh session for publishing updates and executing TC commands
    session: Arc<Session>,
    /// Backend name for topic routing
    backend_name: String,
    /// TC command manager for applying network configurations
    tc_manager: TcCommandManager,
    /// Execution update publisher channel
    update_sender: mpsc::UnboundedSender<ScenarioExecutionUpdate>,
}

/// Individual scenario executor for a specific interface
pub struct ScenarioExecutor {
    /// Current execution state
    execution: ScenarioExecution,
    /// Execution task handle for cleanup operations
    task_handle: tokio::task::JoinHandle<()>,
    /// Control channel for pausing/resuming/stopping execution
    control_sender: mpsc::UnboundedSender<ExecutorControlMessage>,
}

/// Internal control messages for executor tasks
#[derive(Debug, Clone)]
enum ExecutorControlMessage {
    Pause,
    Resume,
    Stop,
}

/// Execution update message for publishing to frontend
#[derive(Debug, Clone)]
pub struct ScenarioExecutionUpdate {
    pub namespace: String,
    pub interface: String,
    pub execution: ScenarioExecution,
    pub backend_name: String,
}

impl ScenarioExecutor {
    /// Cleanup the executor by aborting the task
    pub fn cleanup(self) {
        self.task_handle.abort();
    }
}

impl ScenarioExecutionEngine {
    /// Create a new scenario execution engine
    pub fn new(session: Arc<Session>, backend_name: String, tc_manager: TcCommandManager) -> Self {
        let (update_sender, mut update_receiver) = mpsc::unbounded_channel();
        let session_clone = session.clone();
        let backend_name_clone = backend_name.clone();

        // Spawn update publisher task
        tokio::spawn(async move {
            while let Some(update) = update_receiver.recv().await {
                if let Err(e) =
                    Self::publish_execution_update(&session_clone, &backend_name_clone, update)
                        .await
                {
                    error!("Failed to publish execution update: {}", e);
                }
            }
        });

        Self {
            active_executions: Arc::new(RwLock::new(HashMap::new())),
            session,
            backend_name,
            tc_manager,
            update_sender,
        }
    }

    /// Start executing a scenario on the specified interface
    #[instrument(skip(self), fields(scenario_id = %scenario.id, namespace = %namespace, interface = %interface))]
    pub async fn start_scenario(
        &self,
        scenario: NetworkScenario,
        namespace: String,
        interface: String,
    ) -> Result<String> {
        let execution_key = format!("{}/{}", namespace, interface);

        // Check if there's already an active execution on this interface
        {
            let executions = self.active_executions.read().await;
            if executions.contains_key(&execution_key) {
                return Err(anyhow::anyhow!(
                    "Scenario already running on interface {}:{}",
                    namespace,
                    interface
                ));
            }
        }

        info!(
            "Starting scenario '{}' on interface {}:{}",
            scenario.id, namespace, interface
        );

        // Create execution state
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let execution = ScenarioExecution {
            scenario: scenario.clone(),
            start_time,
            current_step: 0,
            state: ExecutionState::Running,
            target_namespace: namespace.clone(),
            target_interface: interface.clone(),
            stats: ExecutionStats::default(),
        };

        // Create control channels
        let (control_sender, control_receiver) = mpsc::unbounded_channel();

        // Start execution task
        let task_handle = self.spawn_execution_task(
            execution.clone(),
            control_receiver,
            self.tc_manager.clone(),
            self.update_sender.clone(),
        );

        // Create executor
        let executor = ScenarioExecutor {
            execution: execution.clone(),
            task_handle,
            control_sender,
        };

        // Store the executor
        {
            let mut executions = self.active_executions.write().await;
            executions.insert(execution_key.clone(), executor);
        }

        // Send initial execution update
        let _ = self.update_sender.send(ScenarioExecutionUpdate {
            namespace: namespace.clone(),
            interface: interface.clone(),
            execution,
            backend_name: self.backend_name.clone(),
        });

        info!("Started scenario execution with key: {}", execution_key);
        Ok(execution_key)
    }

    /// Stop scenario execution on the specified interface
    #[instrument(skip(self))]
    pub async fn stop_scenario(&self, namespace: &str, interface: &str) -> Result<bool> {
        let execution_key = format!("{}/{}", namespace, interface);

        let mut executions = self.active_executions.write().await;
        if let Some(mut executor) = executions.remove(&execution_key) {
            info!("Stopping scenario execution: {}", execution_key);

            // Send stop signal
            let _ = executor.control_sender.send(ExecutorControlMessage::Stop);

            // Update execution state
            executor.execution.state = ExecutionState::Stopped;

            // Clone execution for the update before consuming executor
            let final_execution = executor.execution.clone();

            // Send final update
            let _ = self.update_sender.send(ScenarioExecutionUpdate {
                namespace: namespace.to_string(),
                interface: interface.to_string(),
                execution: final_execution,
                backend_name: self.backend_name.clone(),
            });

            // Cleanup the executor task
            executor.cleanup();

            Ok(true)
        } else {
            debug!("No active execution found for: {}", execution_key);
            Ok(false)
        }
    }

    /// Pause scenario execution on the specified interface
    #[instrument(skip(self))]
    pub async fn pause_scenario(&self, namespace: &str, interface: &str) -> Result<bool> {
        let execution_key = format!("{}/{}", namespace, interface);

        let mut executions = self.active_executions.write().await;
        if let Some(executor) = executions.get_mut(&execution_key) {
            info!("Pausing scenario execution: {}", execution_key);

            let _ = executor.control_sender.send(ExecutorControlMessage::Pause);

            let paused_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            executor.execution.state = ExecutionState::Paused { paused_at };

            // Send update
            let _ = self.update_sender.send(ScenarioExecutionUpdate {
                namespace: namespace.to_string(),
                interface: interface.to_string(),
                execution: executor.execution.clone(),
                backend_name: self.backend_name.clone(),
            });

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Resume paused scenario execution
    #[instrument(skip(self))]
    pub async fn resume_scenario(&self, namespace: &str, interface: &str) -> Result<bool> {
        let execution_key = format!("{}/{}", namespace, interface);

        let mut executions = self.active_executions.write().await;
        if let Some(executor) = executions.get_mut(&execution_key) {
            info!("Resuming scenario execution: {}", execution_key);

            let _ = executor.control_sender.send(ExecutorControlMessage::Resume);
            executor.execution.state = ExecutionState::Running;

            // Send update
            let _ = self.update_sender.send(ScenarioExecutionUpdate {
                namespace: namespace.to_string(),
                interface: interface.to_string(),
                execution: executor.execution.clone(),
                backend_name: self.backend_name.clone(),
            });

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get execution status for the specified interface
    pub async fn get_execution_status(
        &self,
        namespace: &str,
        interface: &str,
    ) -> Option<ScenarioExecution> {
        let execution_key = format!("{}/{}", namespace, interface);
        let executions = self.active_executions.read().await;
        executions
            .get(&execution_key)
            .map(|executor| executor.execution.clone())
    }

    /// List all active executions
    pub async fn list_active_executions(&self) -> Vec<ScenarioExecution> {
        let executions = self.active_executions.read().await;
        executions
            .values()
            .map(|executor| executor.execution.clone())
            .collect()
    }

    /// Spawn execution task for a scenario
    fn spawn_execution_task(
        &self,
        mut execution: ScenarioExecution,
        mut control_receiver: mpsc::UnboundedReceiver<ExecutorControlMessage>,
        _tc_manager: TcCommandManager,
        update_sender: mpsc::UnboundedSender<ScenarioExecutionUpdate>,
    ) -> tokio::task::JoinHandle<()> {
        let backend_name = self.backend_name.clone();
        let session = self.session.clone();

        tokio::spawn(async move {
            info!(
                "Starting execution task for scenario '{}' on {}:{}",
                execution.scenario.id, execution.target_namespace, execution.target_interface
            );

            let mut paused_duration = Duration::from_millis(0);
            let mut pause_start: Option<Instant> = None;

            let scenario_steps = execution.scenario.steps.clone();

            // Send initial execution update
            execution.current_step = 0;
            let _ = update_sender.send(ScenarioExecutionUpdate {
                namespace: execution.target_namespace.clone(),
                interface: execution.target_interface.clone(),
                execution: execution.clone(),
                backend_name: backend_name.clone(),
            });

            for (step_index, step) in scenario_steps.iter().enumerate() {
                execution.current_step = step_index;

                // Apply TC configuration for this step
                info!(
                    "Executing step {} of scenario '{}': {} (duration: {}ms)",
                    step_index + 1,
                    execution.scenario.id,
                    step.description,
                    step.duration_ms
                );

                let tc_request = TcRequest {
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                    operation: TcOperation::ApplyConfig {
                        config: step.tc_config.clone(),
                    },
                };

                match Self::execute_tc_command(&session, &backend_name, &tc_request).await {
                    Ok(response) if response.success => {
                        debug!("Successfully applied TC config for step {}", step_index + 1);
                        execution.stats.tc_operations += 1;
                    }
                    Ok(response) => {
                        warn!(
                            "TC operation failed for step {}: {}",
                            step_index + 1,
                            response.message
                        );
                        execution.stats.failed_operations += 1;
                        execution.stats.last_error = Some(response.message);
                    }
                    Err(e) => {
                        error!(
                            "Error executing TC command for step {}: {}",
                            step_index + 1,
                            e
                        );
                        execution.stats.failed_operations += 1;
                        execution.stats.last_error = Some(e.to_string());
                    }
                }

                // Send step start update
                let _ = update_sender.send(ScenarioExecutionUpdate {
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                    execution: execution.clone(),
                    backend_name: backend_name.clone(),
                });

                // Wait for step duration
                let step_duration = Duration::from_millis(step.duration_ms);
                debug!(
                    "Maintaining step {} configuration for {:?}",
                    step_index + 1,
                    step_duration
                );

                if (Self::interruptible_sleep(
                    step_duration,
                    &mut control_receiver,
                    &mut execution,
                    &mut pause_start,
                    &mut paused_duration,
                    &update_sender,
                    &backend_name,
                )
                .await)
                    .is_err()
                {
                    // Execution was stopped
                    return;
                }

                execution.stats.steps_completed += 1;
                execution.stats.progress_percent =
                    ((step_index + 1) as f32 / scenario_steps.len() as f32) * 100.0;

                // Send progress update
                let _ = update_sender.send(ScenarioExecutionUpdate {
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                    execution: execution.clone(),
                    backend_name: backend_name.clone(),
                });

                // Check for stop/pause messages during execution
                while let Ok(control_msg) = control_receiver.try_recv() {
                    match control_msg {
                        ExecutorControlMessage::Stop => {
                            info!("Scenario execution stopped by user");
                            execution.state = ExecutionState::Stopped;
                            return;
                        }
                        ExecutorControlMessage::Pause => {
                            info!("Scenario execution paused by user");
                            pause_start = Some(Instant::now());
                            execution.state = ExecutionState::Paused {
                                paused_at: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64,
                            };
                        }
                        ExecutorControlMessage::Resume => {
                            if let Some(paused_at) = pause_start.take() {
                                paused_duration += paused_at.elapsed();
                                info!(
                                    "Scenario execution resumed (total paused: {:?})",
                                    paused_duration
                                );
                                execution.state = ExecutionState::Running;
                            }
                        }
                    }
                }
            }

            // Scenario completed successfully
            info!(
                "Scenario '{}' execution completed successfully",
                execution.scenario.id
            );
            execution.state = ExecutionState::Completed;
            execution.stats.progress_percent = 100.0;

            // Send final completion update
            let _ = update_sender.send(ScenarioExecutionUpdate {
                namespace: execution.target_namespace.clone(),
                interface: execution.target_interface.clone(),
                execution,
                backend_name,
            });
        })
    }

    /// Interruptible sleep that handles pause/resume/stop control messages
    async fn interruptible_sleep(
        mut duration: Duration,
        control_receiver: &mut mpsc::UnboundedReceiver<ExecutorControlMessage>,
        execution: &mut ScenarioExecution,
        pause_start: &mut Option<Instant>,
        paused_duration: &mut Duration,
        update_sender: &mpsc::UnboundedSender<ScenarioExecutionUpdate>,
        backend_name: &str,
    ) -> Result<(), ()> {
        let _sleep_start = Instant::now();

        loop {
            // Sleep in small chunks to be responsive to control messages
            let chunk_duration = Duration::from_millis(100).min(duration);

            tokio::select! {
                _ = sleep(chunk_duration) => {
                    duration = duration.saturating_sub(chunk_duration);
                    if duration.is_zero() {
                        return Ok(());
                    }
                }

                control_msg = control_receiver.recv() => {
                    match control_msg {
                        Some(ExecutorControlMessage::Stop) => {
                            execution.state = ExecutionState::Stopped;
                            return Err(());
                        }
                        Some(ExecutorControlMessage::Pause) => {
                            *pause_start = Some(Instant::now());
                            execution.state = ExecutionState::Paused { paused_at:
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64
                            };

                            // Send pause update
                            let _ = update_sender.send(ScenarioExecutionUpdate {
                                namespace: execution.target_namespace.clone(),
                                interface: execution.target_interface.clone(),
                                execution: execution.clone(),
                                backend_name: backend_name.to_string(),
                            });

                            // Wait for resume
                            loop {
                                if let Some(control_msg) = control_receiver.recv().await {
                                    match control_msg {
                                        ExecutorControlMessage::Stop => {
                                            execution.state = ExecutionState::Stopped;
                                            return Err(());
                                        }
                                        ExecutorControlMessage::Resume => {
                                            if let Some(paused_at) = pause_start.take() {
                                                *paused_duration += paused_at.elapsed();
                                            }
                                            execution.state = ExecutionState::Running;
                                            break;
                                        }
                                        ExecutorControlMessage::Pause => {
                                            // Already paused, ignore
                                        }
                                    }
                                }
                            }
                        }
                        Some(ExecutorControlMessage::Resume) => {
                            // Not paused, ignore
                        }
                        None => {
                            // Channel closed, stop execution
                            execution.state = ExecutionState::Stopped;
                            return Err(());
                        }
                    }
                }
            }
        }
    }

    /// Execute a TC command via Zenoh query
    async fn execute_tc_command(
        session: &Session,
        backend_name: &str,
        tc_request: &TcRequest,
    ) -> Result<TcResponse> {
        let tc_query_topic = topics::tc_query_service(backend_name);
        let request_payload = serde_json::to_vec(tc_request)?;

        debug!("Executing TC command via query: {:?}", tc_request);

        let replies = session
            .get(&tc_query_topic)
            .payload(request_payload)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send TC query: {}", e))?;

        // Get the first reply
        let reply = replies
            .recv_async()
            .await
            .map_err(|e| anyhow::anyhow!("No TC response received: {}", e))?;

        match reply.result() {
            Ok(sample) => {
                let response: TcResponse =
                    serde_json::from_slice(sample.payload().to_bytes().as_ref())?;
                Ok(response)
            }
            Err(e) => Err(anyhow::anyhow!("TC query failed: {:?}", e)),
        }
    }

    /// Publish execution update to Zenoh
    async fn publish_execution_update(
        session: &Session,
        backend_name: &str,
        update: ScenarioExecutionUpdate,
    ) -> Result<()> {
        let topic =
            topics::scenario_execution_updates(backend_name, &update.namespace, &update.interface);

        let payload = serde_json::to_vec(&tcgui_shared::scenario::ScenarioExecutionUpdate {
            namespace: update.namespace,
            interface: update.interface,
            execution: update.execution,
            backend_name: update.backend_name,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        })?;

        session
            .put(&topic, payload)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to publish execution update: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgui_shared::scenario::{NetworkScenario, ScenarioStep};
    use tcgui_shared::TcNetemConfig;

    fn create_test_scenario() -> NetworkScenario {
        let mut scenario = NetworkScenario::new(
            "test-scenario".to_string(),
            "Test Scenario".to_string(),
            "A test scenario for execution testing".to_string(),
        );

        let mut tc_config = TcNetemConfig::new();
        tc_config.loss.enabled = true;
        tc_config.loss.percentage = 5.0;

        scenario.add_step(ScenarioStep::new(
            5000, // 5 seconds duration
            "Step 1: Apply 5% packet loss".to_string(),
            tc_config.clone(),
        ));

        tc_config.loss.percentage = 10.0;
        scenario.add_step(ScenarioStep::new(
            10000, // 10 seconds duration
            "Step 2: Increase to 10% packet loss".to_string(),
            tc_config,
        ));

        scenario
    }

    #[test]
    fn test_execution_key_format() {
        let namespace = "test-namespace";
        let interface = "eth0";
        let expected_key = format!("{}/{}", namespace, interface);

        assert_eq!(expected_key, "test-namespace/eth0");
    }

    #[test]
    fn test_scenario_duration_calculation() {
        let scenario = create_test_scenario();

        // Scenario has steps with 5000ms + 10000ms = 15000ms total
        assert_eq!(scenario.estimated_total_duration_ms(), 15000);
    }

    #[test]
    fn test_executor_control_messages() {
        // Test that control messages are properly constructed
        let pause_msg = ExecutorControlMessage::Pause;
        let resume_msg = ExecutorControlMessage::Resume;
        let stop_msg = ExecutorControlMessage::Stop;

        assert!(matches!(pause_msg, ExecutorControlMessage::Pause));
        assert!(matches!(resume_msg, ExecutorControlMessage::Resume));
        assert!(matches!(stop_msg, ExecutorControlMessage::Stop));
    }
}
