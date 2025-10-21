//! Scenario management and execution module.
//!
//! This module provides the core functionality for the Network Scenario feature,
//! including scenario storage, execution engine, and built-in scenario templates.

pub mod execution;
pub mod manager;
pub mod storage;
pub mod templates;
pub mod zenoh_handlers;

pub use execution::{ScenarioExecutionEngine, ScenarioExecutor};
pub use manager::ScenarioManager;
pub use storage::ScenarioZenohStorage;
pub use templates::BuiltinScenarioTemplates;
pub use zenoh_handlers::{ScenarioExecutionHandlers, ScenarioZenohHandlers};
