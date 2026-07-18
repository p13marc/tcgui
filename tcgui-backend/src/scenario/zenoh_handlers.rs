//! Zenoh handlers for scenario management and execution requests.
//!
//! This module provides the Zenoh query handlers that process scenario
//! management requests and execution control requests from the frontend.

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};
use zenoh::{Session, query::Query};

use tcgui_shared::scenario::{
    ScenarioError, ScenarioExecutionRequest, ScenarioExecutionResponse, ScenarioExecutionUpdate,
    ScenarioRequest, ScenarioResponse,
};
use tcgui_shared::topics;
use zenoh::key_expr::OwnedKeyExpr;

use super::ScenarioManager;

/// Reply to a scenario-management query on the queryable's **own concrete key**
/// (never the echoed, possibly-wildcard `query.key_expr()` — RFC keyspace-v2
/// 05 §2.1), routing the `Error` variant onto Zenoh's reply-error channel with a
/// namespaced name (05 §3) rather than shipping a failure on the value channel.
async fn reply_scenario(
    query: &Query,
    concrete_key: OwnedKeyExpr,
    response: &ScenarioResponse,
) -> Result<()> {
    if let ScenarioResponse::Error { error } = response {
        query
            .reply_err(format!("error/scenario: {error:?}"))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to reply_err to scenario query: {e}"))?;
    } else {
        let payload = serde_json::to_vec(response)?;
        query
            .reply(concrete_key, payload)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send scenario response: {e}"))?;
    }
    Ok(())
}

/// Concrete-key + reply-error twin of [`reply_scenario`] for execution queries.
async fn reply_execution(
    query: &Query,
    concrete_key: OwnedKeyExpr,
    response: &ScenarioExecutionResponse,
) -> Result<()> {
    if let ScenarioExecutionResponse::Error { error } = response {
        query
            .reply_err(format!("error/execution: {error:?}"))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to reply_err to execution query: {e}"))?;
    } else {
        let payload = serde_json::to_vec(response)?;
        query
            .reply(concrete_key, payload)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send execution response: {e}"))?;
    }
    Ok(())
}

/// Zenoh handlers for scenario management
pub struct ScenarioZenohHandlers {
    scenario_manager: Arc<ScenarioManager>,
    session: Arc<Session>,
    backend_name: String,
}

impl ScenarioZenohHandlers {
    /// Create new scenario management handlers
    pub fn new(
        scenario_manager: Arc<ScenarioManager>,
        session: Arc<Session>,
        backend_name: String,
    ) -> Self {
        info!(
            "Creating scenario Zenoh handlers for backend: {}",
            backend_name
        );
        Self {
            scenario_manager,
            session,
            backend_name,
        }
    }

    /// Start the scenario management query handler
    #[instrument(skip(self))]
    pub async fn start_query_handler(&self) -> Result<()> {
        let scenario_query_topic = topics::scenario_query_service(&self.backend_name);
        let queryable = self
            .session
            .declare_queryable(&scenario_query_topic)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare scenario queryable: {}", e))?;

        info!(
            "Scenario management query handler started on topic: {}",
            scenario_query_topic.as_str()
        );

        let scenario_manager = self.scenario_manager.clone();
        let backend_name = self.backend_name.clone();

        tokio::spawn(async move {
            while let Ok(query) = queryable.recv_async().await {
                let scenario_manager = scenario_manager.clone();
                let backend_name = backend_name.clone();

                tokio::spawn(async move {
                    if let Err(e) =
                        Self::handle_scenario_query(&scenario_manager, &backend_name, query).await
                    {
                        error!("Error handling scenario query: {}", e);
                    }
                });
            }
        });

        Ok(())
    }

    /// Handle individual scenario management query
    #[instrument(skip(scenario_manager, query))]
    async fn handle_scenario_query(
        scenario_manager: &ScenarioManager,
        backend_name: &str,
        query: Query,
    ) -> Result<()> {
        debug!(
            "Received scenario query on key: {}",
            query.key_expr().as_str()
        );

        // Parse the request
        let request: ScenarioRequest = match query.payload() {
            Some(payload) => serde_json::from_slice(payload.to_bytes().as_ref())?,
            None => {
                warn!("Received scenario query without payload");
                let error_response = ScenarioResponse::Error {
                    error: ScenarioError::validation("Missing request payload"),
                };
                return reply_scenario(
                    &query,
                    topics::scenario_query_service(backend_name),
                    &error_response,
                )
                .await;
            }
        };

        // Process the request
        let response = Self::process_scenario_request(scenario_manager, request).await;
        reply_scenario(
            &query,
            topics::scenario_query_service(backend_name),
            &response,
        )
        .await?;

        debug!("Scenario query processed successfully");
        Ok(())
    }

    /// Process scenario management request
    async fn process_scenario_request(
        scenario_manager: &ScenarioManager,
        request: ScenarioRequest,
    ) -> ScenarioResponse {
        match request {
            ScenarioRequest::Add(scenario) => {
                info!("Adding scenario: {}", scenario.id);
                match scenario_manager.store_scenario(scenario.clone()).await {
                    Ok(()) => ScenarioResponse::Added { id: scenario.id },
                    Err(e) => {
                        error!("Failed to add scenario: {}", e);
                        ScenarioResponse::Error {
                            error: ScenarioError::internal(format!(
                                "Failed to add scenario: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioRequest::Remove { id } => {
                info!("Removing scenario: {}", id);
                match scenario_manager.delete_scenario(&id).await {
                    Ok(success) => ScenarioResponse::Removed { success },
                    Err(e) => {
                        error!("Failed to remove scenario: {}", e);
                        ScenarioResponse::Error {
                            error: ScenarioError::internal(format!(
                                "Failed to remove scenario: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioRequest::List => {
                debug!("Listing all scenarios");
                match scenario_manager.list_all_scenarios_with_errors().await {
                    Ok((scenarios, load_errors)) => {
                        info!(
                            "Listed {} scenarios ({} load errors)",
                            scenarios.len(),
                            load_errors.len()
                        );
                        ScenarioResponse::Listed {
                            scenarios,
                            load_errors,
                        }
                    }
                    Err(e) => {
                        error!("Failed to list scenarios: {}", e);
                        ScenarioResponse::Error {
                            error: ScenarioError::internal(format!(
                                "Failed to list scenarios: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioRequest::Get { id } => {
                debug!("Getting scenario: {}", id);
                match scenario_manager.get_scenario(&id).await {
                    Ok(scenario) => ScenarioResponse::Retrieved { scenario },
                    Err(e) => {
                        error!("Failed to get scenario: {}", e);
                        ScenarioResponse::Error {
                            error: ScenarioError::permanent(format!(
                                "Failed to get scenario: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioRequest::Update(scenario) => {
                info!("Updating scenario: {}", scenario.id);
                match scenario_manager.store_scenario(scenario).await {
                    Ok(()) => ScenarioResponse::Updated { success: true },
                    Err(e) => {
                        error!("Failed to update scenario: {}", e);
                        ScenarioResponse::Error {
                            error: ScenarioError::internal(format!(
                                "Failed to update scenario: {}",
                                e
                            )),
                        }
                    }
                }
            }
        }
    }
}

/// Zenoh handlers for scenario execution
pub struct ScenarioExecutionHandlers {
    scenario_manager: Arc<ScenarioManager>,
    session: Arc<Session>,
    backend_name: String,
}

impl ScenarioExecutionHandlers {
    /// Create new scenario execution handlers
    pub fn new(
        scenario_manager: Arc<ScenarioManager>,
        session: Arc<Session>,
        backend_name: String,
    ) -> Self {
        info!(
            "Creating scenario execution Zenoh handlers for backend: {}",
            backend_name
        );
        Self {
            scenario_manager,
            session,
            backend_name,
        }
    }

    /// Start the scenario execution query handler
    #[instrument(skip(self))]
    pub async fn start_query_handler(&self) -> Result<()> {
        let execution_query_topic = topics::scenario_execution_query_service(&self.backend_name);
        let queryable = self
            .session
            .declare_queryable(&execution_query_topic)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare execution queryable: {}", e))?;

        info!(
            "Scenario execution query handler started on topic: {}",
            execution_query_topic.as_str()
        );

        let scenario_manager = self.scenario_manager.clone();
        let backend_name = self.backend_name.clone();

        tokio::spawn(async move {
            while let Ok(query) = queryable.recv_async().await {
                let scenario_manager = scenario_manager.clone();
                let backend_name = backend_name.clone();

                tokio::spawn(async move {
                    if let Err(e) =
                        Self::handle_execution_query(&scenario_manager, &backend_name, query).await
                    {
                        error!("Error handling execution query: {}", e);
                    }
                });
            }
        });

        Ok(())
    }

    /// Handle individual scenario execution query
    #[instrument(skip(scenario_manager, query))]
    async fn handle_execution_query(
        scenario_manager: &ScenarioManager,
        backend_name: &str,
        query: Query,
    ) -> Result<()> {
        debug!(
            "Received execution query on key: {}",
            query.key_expr().as_str()
        );

        // Parse the request
        let request: ScenarioExecutionRequest = match query.payload() {
            Some(payload) => serde_json::from_slice(payload.to_bytes().as_ref())?,
            None => {
                warn!("Received execution query without payload");
                let error_response = ScenarioExecutionResponse::Error {
                    error: ScenarioError::validation("Missing request payload"),
                };
                return reply_execution(
                    &query,
                    topics::scenario_execution_query_service(backend_name),
                    &error_response,
                )
                .await;
            }
        };

        // Process the request
        let response = Self::process_execution_request(scenario_manager, request).await;
        reply_execution(
            &query,
            topics::scenario_execution_query_service(backend_name),
            &response,
        )
        .await?;

        debug!("Execution query processed successfully");
        Ok(())
    }

    /// Process scenario execution request
    async fn process_execution_request(
        scenario_manager: &ScenarioManager,
        request: ScenarioExecutionRequest,
    ) -> ScenarioExecutionResponse {
        match request {
            ScenarioExecutionRequest::Start {
                scenario_id,
                namespace,
                interface,
                loop_execution,
            } => {
                info!(
                    "Starting scenario '{}' on {}:{} (loop: {})",
                    scenario_id, namespace, interface, loop_execution
                );
                match scenario_manager
                    .start_scenario_execution(&scenario_id, namespace, interface, loop_execution)
                    .await
                {
                    Ok(execution_id) => {
                        // Get scenario duration for estimated time
                        let estimated_duration = scenario_manager
                            .get_scenario(&scenario_id)
                            .await
                            .ok()
                            .flatten()
                            .map(|s| s.estimated_total_duration_ms())
                            .unwrap_or(0);

                        ScenarioExecutionResponse::Started {
                            execution_id,
                            estimated_duration_ms: estimated_duration,
                        }
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        error!("Failed to start scenario execution: {}", err_str);
                        // Categorize the error based on content
                        let error = if err_str.contains("not found") {
                            ScenarioError::permanent(format!("Scenario not found: {}", err_str))
                        } else if err_str.contains("already running") {
                            ScenarioError::permanent(err_str)
                                .with_suggestion("Stop the existing execution first.")
                        } else {
                            ScenarioError::transient(format!(
                                "Failed to start execution: {}",
                                err_str
                            ))
                        };
                        ScenarioExecutionResponse::Error { error }
                    }
                }
            }
            ScenarioExecutionRequest::Stop {
                namespace,
                interface,
            } => {
                info!("Stopping scenario execution on {}:{}", namespace, interface);
                match scenario_manager
                    .stop_scenario_execution(&namespace, &interface)
                    .await
                {
                    Ok(success) => ScenarioExecutionResponse::Stopped { success },
                    Err(e) => {
                        error!("Failed to stop scenario execution: {}", e);
                        ScenarioExecutionResponse::Error {
                            error: ScenarioError::transient(format!(
                                "Failed to stop execution: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioExecutionRequest::Pause {
                namespace,
                interface,
            } => {
                info!("Pausing scenario execution on {}:{}", namespace, interface);
                match scenario_manager
                    .pause_scenario_execution(&namespace, &interface)
                    .await
                {
                    Ok(success) => ScenarioExecutionResponse::Paused { success },
                    Err(e) => {
                        error!("Failed to pause scenario execution: {}", e);
                        ScenarioExecutionResponse::Error {
                            error: ScenarioError::transient(format!(
                                "Failed to pause execution: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioExecutionRequest::Resume {
                namespace,
                interface,
            } => {
                info!("Resuming scenario execution on {}:{}", namespace, interface);
                match scenario_manager
                    .resume_scenario_execution(&namespace, &interface)
                    .await
                {
                    Ok(success) => ScenarioExecutionResponse::Resumed { success },
                    Err(e) => {
                        error!("Failed to resume scenario execution: {}", e);
                        ScenarioExecutionResponse::Error {
                            error: ScenarioError::transient(format!(
                                "Failed to resume execution: {}",
                                e
                            )),
                        }
                    }
                }
            }
            ScenarioExecutionRequest::Status {
                namespace,
                interface,
            } => {
                debug!("Getting execution status for {}:{}", namespace, interface);
                let execution = scenario_manager
                    .get_execution_status(&namespace, &interface)
                    .await;
                ScenarioExecutionResponse::Status {
                    execution: Box::new(execution),
                }
            }
            ScenarioExecutionRequest::ListActive => {
                debug!("Listing active executions");
                let executions = scenario_manager.list_active_executions().await;
                info!("Listed {} active executions", executions.len());
                ScenarioExecutionResponse::ActiveExecutions { executions }
            }
        }
    }

    /// Start the scenario execution status publishing service
    #[instrument(skip(self))]
    pub async fn start_status_publisher(&self) -> Result<()> {
        info!("Starting scenario execution status publisher");

        let session = self.session.clone();
        let scenario_manager = self.scenario_manager.clone();
        let backend_name = self.backend_name.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));

            loop {
                interval.tick().await;

                let active_executions = scenario_manager.list_active_executions().await;

                for execution in active_executions {
                    let update = ScenarioExecutionUpdate {
                        namespace: execution.target_namespace.clone(),
                        interface: execution.target_interface.clone(),
                        execution: execution.clone(),
                        backend_name: backend_name.clone(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    };

                    let update_topic = topics::scenario_execution_updates(
                        &backend_name,
                        &execution.target_namespace,
                        &execution.target_interface,
                    );

                    match serde_json::to_vec(&update) {
                        Ok(payload) => {
                            if let Err(e) = session.put(&update_topic, payload).await {
                                error!(
                                    "Failed to publish execution status for {}:{}: {}",
                                    execution.target_namespace, execution.target_interface, e
                                );
                            }
                        }
                        Err(e) => {
                            error!("Failed to serialize execution update: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }
}
