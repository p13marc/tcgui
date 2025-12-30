//! Service layer for TC GUI backend.
//!
//! This module contains the business logic services that handle specific domains
//! of functionality. Services are designed to be testable, dependency-injected,
//! and follow single responsibility principle.

pub mod bandwidth_service;
pub mod network_service;
pub mod tc_service;

pub use bandwidth_service::BandwidthService;
pub use network_service::NetworkService;
pub use tc_service::TcService;

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

/// Base trait for all services
pub trait Service {
    /// Service name for logging and identification
    fn name(&self) -> &'static str;

    /// Initialize the service
    fn initialize(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Gracefully shutdown the service
    fn shutdown(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;

    /// Get service health status
    fn health_check(&self) -> Pin<Box<dyn Future<Output = Result<ServiceHealth>> + Send + '_>>;
}

/// Service health status
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceHealth {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}

impl ServiceHealth {
    pub fn is_healthy(&self) -> bool {
        matches!(self, ServiceHealth::Healthy)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            ServiceHealth::Healthy => None,
            ServiceHealth::Degraded { reason } => Some(reason),
            ServiceHealth::Unhealthy { reason } => Some(reason),
        }
    }
}

/// Service dependencies container
pub struct ServiceContainer {
    pub tc_service: TcService,
    pub network_service: NetworkService,
    pub bandwidth_service: BandwidthService,
}

impl ServiceContainer {
    /// Create new service container with all services
    pub async fn new(
        tc_service: TcService,
        network_service: NetworkService,
        bandwidth_service: BandwidthService,
    ) -> Result<Self> {
        Ok(Self {
            tc_service,
            network_service,
            bandwidth_service,
        })
    }

    /// Initialize all services
    pub async fn initialize(&mut self) -> Result<()> {
        tracing::info!("Initializing service container");

        // Initialize services in dependency order
        self.network_service.initialize().await?;
        self.bandwidth_service.initialize().await?;
        self.tc_service.initialize().await?;

        tracing::info!("Service container initialized successfully");
        Ok(())
    }

    /// Shutdown all services gracefully
    pub async fn shutdown(&mut self) -> Result<()> {
        tracing::info!("Shutting down service container");

        // Shutdown in reverse order
        let _ = self.tc_service.shutdown().await;
        let _ = self.bandwidth_service.shutdown().await;
        let _ = self.network_service.shutdown().await;

        tracing::info!("Service container shutdown complete");
        Ok(())
    }

    /// Perform health check on all services
    pub async fn health_check(&self) -> Result<Vec<(String, ServiceHealth)>> {
        let mut results = Vec::new();

        results.push((
            self.tc_service.name().to_string(),
            self.tc_service.health_check().await?,
        ));
        results.push((
            self.network_service.name().to_string(),
            self.network_service.health_check().await?,
        ));
        results.push((
            self.bandwidth_service.name().to_string(),
            self.bandwidth_service.health_check().await?,
        ));

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_health_is_healthy() {
        assert!(ServiceHealth::Healthy.is_healthy());
        assert!(
            !ServiceHealth::Degraded {
                reason: "test".to_string()
            }
            .is_healthy()
        );
        assert!(
            !ServiceHealth::Unhealthy {
                reason: "test".to_string()
            }
            .is_healthy()
        );
    }

    #[test]
    fn test_service_health_reason() {
        assert_eq!(ServiceHealth::Healthy.reason(), None);
        assert_eq!(
            ServiceHealth::Degraded {
                reason: "test".to_string()
            }
            .reason(),
            Some("test")
        );
        assert_eq!(
            ServiceHealth::Unhealthy {
                reason: "error".to_string()
            }
            .reason(),
            Some("error")
        );
    }
}
