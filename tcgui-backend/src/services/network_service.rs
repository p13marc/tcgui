//! Network Service
//!
//! This service handles network interface monitoring and management:
//! - Scanning for available network interfaces
//! - Publishing interface lists and statistics
//! - Monitoring interface state changes
//! - Supporting namespace-aware operations

use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration as StdDuration, Instant, SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};
use tracing::{error, info, instrument, warn};
use zenoh::Session;
use zenoh_ext::{AdvancedPublisher, AdvancedPublisherBuilderExt, CacheConfig, MissDetectionConfig};

use tcgui_shared::{
    errors::TcguiError, topics, InterfaceListUpdate, NetworkInterface, NetworkNamespace,
};

use super::ServiceHealth;
use crate::interfaces::{Interface, NamespaceInterfaces};
use crate::utils::service_resilience::{execute_network_discovery, execute_zenoh_communication};

/// Network Service for managing network interface discovery and monitoring
pub struct NetworkService {
    /// Zenoh session for messaging
    session: Session,

    /// Backend name for topic routing
    backend_name: String,

    /// Interface list publishers per namespace
    interface_publishers: HashMap<String, AdvancedPublisher<'static>>, // namespace -> publisher

    /// Namespace list publisher
    namespace_publisher: Option<AdvancedPublisher<'static>>,

    /// Cache of current interfaces per namespace
    interface_cache: HashMap<String, Vec<Interface>>, // namespace -> interfaces

    /// Cache of namespace list
    namespace_cache: Vec<NetworkNamespace>,

    /// Background monitoring task handle
    monitor_task: Option<JoinHandle<()>>,

    /// Last scan timestamp per namespace
    last_scan_times: HashMap<String, Instant>,

    /// Service health status
    health_status: ServiceHealth,

    /// Scanning configuration
    scan_interval: Duration,
    cache_duration: StdDuration,
}

impl NetworkService {
    /// Create a new network service
    pub fn new(session: Session, backend_name: String) -> Self {
        Self {
            session,
            backend_name,
            interface_publishers: HashMap::new(),
            namespace_publisher: None,
            interface_cache: HashMap::new(),
            namespace_cache: Vec::new(),
            monitor_task: None,
            last_scan_times: HashMap::new(),
            health_status: ServiceHealth::Healthy,
            scan_interval: Duration::from_secs(5), // Scan every 5 seconds
            cache_duration: StdDuration::from_secs(10), // Cache for 10 seconds
        }
    }

    /// Initialize the service and start monitoring
    pub async fn initialize(&mut self) -> Result<()> {
        info!("Initializing network service");

        // Initialize namespace publisher
        self.setup_namespace_publisher().await?;

        // Start background monitoring task
        self.start_monitoring().await?;

        // Perform initial scan
        self.refresh_all_namespaces().await?;

        self.health_status = ServiceHealth::Healthy;
        info!("Network service initialized successfully");
        Ok(())
    }

    /// Shutdown the service
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down network service");

        // Stop monitoring task
        if let Some(task) = self.monitor_task.take() {
            task.abort();
        }

        // Clear caches and publishers
        self.interface_publishers.clear();
        self.interface_cache.clear();
        self.namespace_cache.clear();
        self.last_scan_times.clear();

        info!("Network service shut down");
        Ok(())
    }

    /// Get service health status
    pub async fn health_check(&self) -> Result<ServiceHealth> {
        Ok(self.health_status.clone())
    }

    /// Get interfaces for a specific namespace (with caching)
    #[instrument(skip(self), fields(service = "network", namespace))]
    pub async fn get_interfaces(&mut self, namespace: &str) -> Result<Vec<Interface>> {
        // Check if we have a cached version that's still fresh
        if let Some(last_scan) = self.last_scan_times.get(namespace) {
            if last_scan.elapsed() < self.cache_duration {
                if let Some(cached_interfaces) = self.interface_cache.get(namespace) {
                    info!("Returning cached interfaces for namespace: {}", namespace);
                    return Ok(cached_interfaces.clone());
                }
            }
        }

        // Cache miss or stale data - perform fresh scan
        info!("Scanning interfaces for namespace: {}", namespace);
        self.scan_namespace_interfaces(namespace).await
    }

    /// Get all namespaces (with caching)
    #[instrument(skip(self), fields(service = "network"))]
    pub async fn get_namespaces(&mut self) -> Result<Vec<NetworkNamespace>> {
        // For simplicity, always return fresh namespace data
        // In a full implementation, this could be cached too
        info!("Scanning available namespaces");
        self.scan_namespaces().await
    }

    /// Force refresh of all namespace data
    #[instrument(skip(self), fields(service = "network"))]
    pub async fn refresh_all_namespaces(&mut self) -> Result<()> {
        info!("Refreshing all namespace data");

        // Scan and cache namespaces
        match self.scan_namespaces().await {
            Ok(namespaces) => {
                self.namespace_cache = namespaces.clone();

                // Publish namespace list
                if let Err(e) = self.publish_namespace_list(&namespaces).await {
                    warn!("Failed to publish namespace list: {}", e);
                }

                // Scan interfaces for each namespace
                for ns_info in &namespaces {
                    if let Err(e) = self
                        .scan_and_publish_namespace_interfaces(&ns_info.name)
                        .await
                    {
                        warn!(
                            "Failed to scan interfaces for namespace {}: {}",
                            ns_info.name, e
                        );
                    }
                }
            }
            Err(e) => {
                error!("Failed to scan namespaces: {}", e);
                self.health_status = ServiceHealth::Degraded {
                    reason: format!("Namespace scan failed: {}", e),
                };
                return Err(e);
            }
        }

        Ok(())
    }

    /// Setup namespace publisher
    async fn setup_namespace_publisher(&mut self) -> Result<()> {
        let namespace_topic = topics::interface_list(&self.backend_name);
        info!(
            "Creating namespace publisher on: {}",
            namespace_topic.as_str()
        );

        let session = self.session.clone();
        let topic = namespace_topic.clone();

        let publisher = execute_zenoh_communication(
            move || {
                let session = session.clone();
                let topic = topic.clone();
                async move {
                    session
                        .declare_publisher(topic)
                        .cache(CacheConfig::default().max_samples(1))
                        .sample_miss_detection(
                            MissDetectionConfig::default().heartbeat(Duration::from_millis(2000)),
                        )
                        .publisher_detection()
                        .await
                        .map_err(|e| {
                            anyhow::Error::from(TcguiError::ZenohError {
                                message: format!("Failed to declare namespace publisher: {}", e),
                            })
                        })
                }
            },
            "setup_namespace_publisher",
            "network_service",
        )
        .await?;

        self.namespace_publisher = Some(publisher);
        Ok(())
    }

    /// Get or create interface publisher for a namespace
    async fn get_interface_publisher(
        &mut self,
        namespace: &str,
    ) -> Result<&AdvancedPublisher<'static>> {
        if !self.interface_publishers.contains_key(namespace) {
            let interface_topic = topics::interface_list(&self.backend_name);
            info!(
                "Creating interface publisher for {} on: {}",
                namespace,
                interface_topic.as_str()
            );

            let session = self.session.clone();
            let topic = interface_topic.clone();

            let publisher = execute_zenoh_communication(
                move || {
                    let session = session.clone();
                    let topic = topic.clone();
                    async move {
                        session
                            .declare_publisher(topic)
                            .cache(CacheConfig::default().max_samples(1))
                            .sample_miss_detection(
                                MissDetectionConfig::default()
                                    .heartbeat(Duration::from_millis(2000)),
                            )
                            .publisher_detection()
                            .await
                            .map_err(|e| {
                                anyhow::Error::from(TcguiError::ZenohError {
                                    message: format!(
                                        "Failed to declare interface publisher: {}",
                                        e
                                    ),
                                })
                            })
                    }
                },
                "get_interface_publisher",
                "network_service",
            )
            .await?;

            self.interface_publishers
                .insert(namespace.to_string(), publisher);
        }

        Ok(self.interface_publishers.get(namespace).unwrap())
    }

    /// Scan and cache interfaces for a specific namespace
    async fn scan_namespace_interfaces(&mut self, namespace: &str) -> Result<Vec<Interface>> {
        let namespace_clone = namespace.to_string();

        let interfaces = execute_network_discovery(
            move || {
                let ns = namespace_clone.clone();
                async move { NamespaceInterfaces::scan_interfaces_in_namespace(&ns).await }
            },
            "scan_namespace_interfaces",
            "network_service",
        )
        .await
        .map_err(|e| {
            error!(
                "Failed to scan interfaces in namespace {}: {}",
                namespace, e
            );
            self.health_status = ServiceHealth::Degraded {
                reason: format!("Interface scan failed: {}", e),
            };
            e
        })?;

        // Update cache
        self.interface_cache
            .insert(namespace.to_string(), interfaces.clone());
        self.last_scan_times
            .insert(namespace.to_string(), Instant::now());

        info!(
            "Scanned {} interfaces in namespace: {}",
            interfaces.len(),
            namespace
        );
        Ok(interfaces)
    }

    /// Scan and publish interfaces for a namespace
    async fn scan_and_publish_namespace_interfaces(&mut self, namespace: &str) -> Result<()> {
        let interfaces = self.scan_namespace_interfaces(namespace).await?;

        // Convert to network interface for publishing
        let network_interfaces: Vec<NetworkInterface> = interfaces
            .iter()
            .map(|iface| NetworkInterface {
                name: iface.name.clone(),
                index: iface.index,
                namespace: namespace.to_string(),
                is_up: iface.is_up,
                has_tc_qdisc: false, // Will be updated by TC detection
                interface_type: tcgui_shared::InterfaceType::Virtual, // Default type
            })
            .collect();

        // Publish interface list
        self.publish_interface_list(namespace, &network_interfaces)
            .await?;

        Ok(())
    }

    /// Scan available namespaces
    async fn scan_namespaces(&self) -> Result<Vec<NetworkNamespace>> {
        let namespaces = execute_network_discovery(
            || async { NamespaceInterfaces::scan_namespaces().await },
            "scan_namespaces",
            "network_service",
        )
        .await
        .map_err(|e| {
            error!("Failed to scan namespaces: {}", e);
            e
        })?;

        let namespace_infos: Vec<NetworkNamespace> = namespaces
            .into_iter()
            .map(|ns| NetworkNamespace {
                name: ns.name,
                id: None,
                is_active: true,
                interfaces: vec![], // Will be populated separately
            })
            .collect();

        info!("Scanned {} namespaces", namespace_infos.len());
        Ok(namespace_infos)
    }

    /// Publish interface list for a namespace
    #[instrument(skip(self, interfaces), fields(service = "network", namespace, interface_count = interfaces.len()))]
    async fn publish_interface_list(
        &mut self,
        namespace: &str,
        interfaces: &[NetworkInterface],
    ) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let backend_name = self.backend_name.clone();
        let publisher = self.get_interface_publisher(namespace).await?;

        let interface_list = InterfaceListUpdate {
            namespaces: vec![NetworkNamespace {
                name: namespace.to_string(),
                id: None,
                is_active: true,
                interfaces: interfaces.to_vec(),
            }],
            timestamp,
            backend_name,
        };

        let payload = serde_json::to_string(&interface_list)?;

        // Use resilient Zenoh communication for publishing
        let publisher_clone = publisher;
        execute_zenoh_communication(
            move || {
                let publisher = publisher_clone;
                let payload = payload.clone();
                async move {
                    publisher.put(payload).await.map_err(|e| {
                        anyhow::Error::from(TcguiError::ZenohError {
                            message: format!("Failed to publish interface list: {}", e),
                        })
                    })
                }
            },
            "publish_interface_list",
            "network_service",
        )
        .await?;

        info!(
            "Published {} interfaces for namespace: {}",
            interfaces.len(),
            namespace
        );
        Ok(())
    }

    /// Publish namespace list
    #[instrument(skip(self, namespaces), fields(service = "network", namespace_count = namespaces.len()))]
    async fn publish_namespace_list(&mut self, namespaces: &[NetworkNamespace]) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let backend_name = self.backend_name.clone();

        if let Some(publisher) = &self.namespace_publisher {
            let namespace_list = InterfaceListUpdate {
                namespaces: namespaces.to_vec(),
                timestamp,
                backend_name,
            };

            let payload = serde_json::to_string(&namespace_list)?;

            // Use resilient Zenoh communication for publishing
            let publisher_clone = publisher;
            execute_zenoh_communication(
                move || {
                    let publisher = publisher_clone;
                    let payload = payload.clone();
                    async move {
                        publisher.put(payload).await.map_err(|e| {
                            anyhow::Error::from(TcguiError::ZenohError {
                                message: format!("Failed to publish namespace list: {}", e),
                            })
                        })
                    }
                },
                "publish_namespace_list",
                "network_service",
            )
            .await?;

            info!("Published {} namespaces", namespaces.len());
        } else {
            warn!("Namespace publisher not available");
        }

        Ok(())
    }

    /// Start background monitoring task
    async fn start_monitoring(&mut self) -> Result<()> {
        let backend_name = self.backend_name.clone();
        let session = self.session.clone();
        let scan_interval = self.scan_interval;

        let task = tokio::spawn(async move {
            info!("Starting network monitoring task");
            let mut interval_timer = interval(scan_interval);

            loop {
                interval_timer.tick().await;

                // Create a temporary network service for monitoring
                // In practice, this would be refactored to share state properly
                let mut monitor_service =
                    NetworkService::new(session.clone(), backend_name.clone());

                if let Err(e) = monitor_service.setup_namespace_publisher().await {
                    warn!("Failed to setup namespace publisher in monitor: {}", e);
                    continue;
                }

                if let Err(e) = monitor_service.refresh_all_namespaces().await {
                    warn!("Failed to refresh namespaces in monitor: {}", e);
                }
            }
        });

        self.monitor_task = Some(task);
        info!("Network monitoring task started");
        Ok(())
    }

    /// Get cached interfaces without scanning
    pub fn get_cached_interfaces(&self, namespace: &str) -> Option<&Vec<Interface>> {
        self.interface_cache.get(namespace)
    }

    /// Get cached namespaces without scanning
    pub fn get_cached_namespaces(&self) -> &Vec<NetworkNamespace> {
        &self.namespace_cache
    }

    /// Service name
    pub fn name(&self) -> &'static str {
        "network_service"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interfaces::Interface;
    use zenoh::Wait;

    #[test]
    fn test_service_name() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let service = NetworkService::new(session, "test".to_string());
        assert_eq!(service.name(), "network_service");
    }

    #[test]
    fn test_service_creation() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let service = NetworkService::new(session, "test".to_string());

        assert!(service.interface_cache.is_empty());
        assert!(service.namespace_cache.is_empty());
        assert_eq!(service.backend_name, "test");
        assert!(matches!(service.health_status, ServiceHealth::Healthy));
    }

    #[test]
    fn test_get_cached_interfaces() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let mut service = NetworkService::new(session, "test".to_string());

        // Initially no cached interfaces
        assert!(service.get_cached_interfaces("default").is_none());

        // Add some interfaces to cache
        let test_interface = Interface {
            name: "eth0".to_string(),
            index: 1,
            is_up: true,
            mtu: 1500,
            mac_address: "00:11:22:33:44:55".to_string(),
            ip_addresses: vec!["192.168.1.100".to_string()],
            interface_type: "ethernet".to_string(),
        };

        service
            .interface_cache
            .insert("default".to_string(), vec![test_interface]);

        // Should return cached interfaces
        let cached = service.get_cached_interfaces("default");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
        assert_eq!(cached.unwrap()[0].name, "eth0");
    }

    #[test]
    fn test_get_cached_namespaces() {
        let session = zenoh::open(zenoh::Config::default()).wait().unwrap();
        let mut service = NetworkService::new(session, "test".to_string());

        // Initially empty
        assert!(service.get_cached_namespaces().is_empty());

        // Add namespace to cache
        let test_namespace = NetworkNamespace {
            name: "test-ns".to_string(),
            id: None,
            is_active: true,
            interfaces: vec![],
        };

        service.namespace_cache.push(test_namespace);

        // Should return cached namespaces
        let cached = service.get_cached_namespaces();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name, "test-ns");
    }
}
