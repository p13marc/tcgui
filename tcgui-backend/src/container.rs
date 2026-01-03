//! Container runtime discovery and management.
//!
//! This module provides discovery and management of Docker and Podman containers,
//! enabling TC GUI to apply traffic control rules to container network namespaces.

use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use bollard::Docker;
use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::models::ContainerInspectResponse;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, info, warn};

use nlink::netlink::{Connection, Route};

/// Container runtime type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerRuntime {
    /// Docker container runtime
    Docker,
    /// Podman container runtime
    Podman,
}

impl std::fmt::Display for ContainerRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerRuntime::Docker => write!(f, "docker"),
            ContainerRuntime::Podman => write!(f, "podman"),
        }
    }
}

/// Container state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerState {
    /// Container is running
    Running,
    /// Container is paused
    Paused,
    /// Container is stopped/exited
    Stopped,
    /// Unknown state
    Unknown,
}

impl From<&str> for ContainerState {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "running" => ContainerState::Running,
            "paused" => ContainerState::Paused,
            "exited" | "stopped" | "dead" => ContainerState::Stopped,
            _ => ContainerState::Unknown,
        }
    }
}

/// Network configuration for a container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerNetwork {
    /// Network name (e.g., "bridge", "my-network")
    pub network_name: String,
    /// Network ID
    pub network_id: String,
    /// Interface name inside the container (usually "eth0")
    pub interface_name: String,
    /// IP address assigned to the container
    pub ip_address: Option<IpAddr>,
    /// MAC address
    pub mac_address: Option<String>,
    /// Gateway IP address
    pub gateway: Option<IpAddr>,
    /// Whether this is an internal network (no external connectivity)
    pub is_internal: bool,
}

/// Discovered container with network information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    /// Full container ID
    pub id: String,
    /// Short container ID (first 12 characters)
    pub short_id: String,
    /// Container name (without leading slash)
    pub name: String,
    /// Container runtime (Docker or Podman)
    pub runtime: ContainerRuntime,
    /// Main process PID (if running)
    pub pid: Option<u32>,
    /// Path to network namespace (e.g., /proc/<pid>/ns/net)
    pub namespace_path: Option<PathBuf>,
    /// Network configurations
    pub networks: Vec<ContainerNetwork>,
    /// Current container state
    pub state: ContainerState,
    /// Container creation timestamp (Unix epoch)
    pub created: i64,
    /// Container image name
    pub image: String,
}

impl Container {
    /// Returns a display name for the container suitable for UI.
    #[allow(dead_code)]
    pub fn display_name(&self) -> String {
        format!("{} ({})", self.name, self.short_id)
    }

    /// Returns the namespace identifier for this container.
    #[allow(dead_code)]
    pub fn namespace_id(&self) -> String {
        format!("container:{}", self.name)
    }
}

/// Manager for container runtime discovery and operations.
pub struct ContainerManager {
    /// Docker client (if Docker socket is available)
    docker_client: Option<Docker>,
    /// Podman client (if Podman socket is available)
    podman_client: Option<Docker>, // Podman uses Docker-compatible API
    /// Available container runtimes
    available_runtimes: Vec<ContainerRuntime>,
}

impl ContainerManager {
    /// Creates a new ContainerManager, detecting available container runtimes.
    pub async fn new() -> Self {
        let mut available_runtimes = Vec::new();

        // Try Docker
        let docker_client = Self::connect_docker().await;
        if docker_client.is_some() {
            available_runtimes.push(ContainerRuntime::Docker);
            info!("Docker runtime detected");
        }

        // Try Podman
        let podman_client = Self::connect_podman().await;
        if podman_client.is_some() {
            available_runtimes.push(ContainerRuntime::Podman);
            info!("Podman runtime detected");
        }

        if available_runtimes.is_empty() {
            debug!("No container runtimes detected");
        }

        Self {
            docker_client,
            podman_client,
            available_runtimes,
        }
    }

    /// Attempts to connect to Docker daemon.
    async fn connect_docker() -> Option<Docker> {
        // Try standard Docker socket locations
        let socket_paths = ["/var/run/docker.sock", "/run/docker.sock"];

        for path in socket_paths {
            if std::path::Path::new(path).exists() {
                match Docker::connect_with_unix(path, 120, bollard::API_DEFAULT_VERSION) {
                    Ok(client) => {
                        // Verify connection works
                        match client.ping().await {
                            Ok(_) => {
                                debug!("Connected to Docker at {}", path);
                                return Some(client);
                            }
                            Err(e) => {
                                debug!("Docker socket exists at {} but ping failed: {}", path, e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to connect to Docker at {}: {}", path, e);
                    }
                }
            }
        }

        None
    }

    /// Attempts to connect to Podman daemon.
    async fn connect_podman() -> Option<Docker> {
        // Podman socket locations (rootful and rootless)
        let mut socket_paths = vec![
            PathBuf::from("/run/podman/podman.sock"),
            PathBuf::from("/var/run/podman/podman.sock"),
        ];

        // Add rootless user socket
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            socket_paths.push(PathBuf::from(runtime_dir).join("podman/podman.sock"));
        }

        // Also check user's UID-based path
        if let Ok(uid) = std::env::var("UID").or_else(|_| {
            std::fs::read_to_string("/proc/self/loginuid").map(|s| s.trim().to_string())
        }) {
            socket_paths.push(PathBuf::from(format!(
                "/run/user/{}/podman/podman.sock",
                uid
            )));
        }

        for path in socket_paths {
            if path.exists() {
                let path_str = path.to_string_lossy();
                match Docker::connect_with_unix(&path_str, 120, bollard::API_DEFAULT_VERSION) {
                    Ok(client) => {
                        // Verify connection works
                        match client.ping().await {
                            Ok(_) => {
                                debug!("Connected to Podman at {}", path_str);
                                return Some(client);
                            }
                            Err(e) => {
                                debug!(
                                    "Podman socket exists at {} but ping failed: {}",
                                    path_str, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to connect to Podman at {}: {}", path_str, e);
                    }
                }
            }
        }

        None
    }

    /// Returns whether any container runtime is available.
    pub fn is_available(&self) -> bool {
        !self.available_runtimes.is_empty()
    }

    /// Returns the list of available container runtimes.
    pub fn available_runtimes(&self) -> &[ContainerRuntime] {
        &self.available_runtimes
    }

    /// Discovers all running containers from available runtimes.
    pub async fn discover_containers(&self) -> Result<Vec<Container>> {
        let mut containers = Vec::new();

        // Query Docker
        if let Some(client) = &self.docker_client {
            match self.list_containers(client, ContainerRuntime::Docker).await {
                Ok(docker_containers) => {
                    debug!("Discovered {} Docker containers", docker_containers.len());
                    containers.extend(docker_containers);
                }
                Err(e) => {
                    warn!("Failed to list Docker containers: {}", e);
                }
            }
        }

        // Query Podman
        if let Some(client) = &self.podman_client {
            match self.list_containers(client, ContainerRuntime::Podman).await {
                Ok(podman_containers) => {
                    debug!("Discovered {} Podman containers", podman_containers.len());
                    containers.extend(podman_containers);
                }
                Err(e) => {
                    warn!("Failed to list Podman containers: {}", e);
                }
            }
        }

        Ok(containers)
    }

    /// Lists containers from a specific runtime client.
    async fn list_containers(
        &self,
        client: &Docker,
        runtime: ContainerRuntime,
    ) -> Result<Vec<Container>> {
        let options = ListContainersOptions::<String> {
            all: false, // Only running containers
            ..Default::default()
        };

        let container_summaries = client
            .list_containers(Some(options))
            .await
            .context("Failed to list containers")?;

        let mut containers = Vec::new();

        for summary in container_summaries {
            let id = summary.id.unwrap_or_default();
            if id.is_empty() {
                continue;
            }

            // Get detailed container info
            match self.inspect_container(client, &id, runtime).await {
                Ok(container) => {
                    containers.push(container);
                }
                Err(e) => {
                    warn!("Failed to inspect container {}: {}", id, e);
                }
            }
        }

        Ok(containers)
    }

    /// Inspects a container to get detailed information.
    async fn inspect_container(
        &self,
        client: &Docker,
        id: &str,
        runtime: ContainerRuntime,
    ) -> Result<Container> {
        let info = client
            .inspect_container(id, None::<InspectContainerOptions>)
            .await
            .context("Failed to inspect container")?;

        let full_id = info.id.clone().unwrap_or_else(|| id.to_string());
        let short_id: String = full_id.chars().take(12).collect();

        // Extract name (remove leading slash)
        let name = info
            .name
            .clone()
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_else(|| short_id.clone());

        // Extract state
        let state = info
            .state
            .as_ref()
            .and_then(|s| s.status.as_ref())
            .map(|s| {
                let status_str = format!("{:?}", s);
                ContainerState::from(status_str.as_str())
            })
            .unwrap_or(ContainerState::Unknown);

        // Extract PID
        let pid = info
            .state
            .as_ref()
            .and_then(|s| s.pid)
            .and_then(|p| if p > 0 { Some(p as u32) } else { None });

        // Build namespace path from PID
        let namespace_path = pid.map(|p| PathBuf::from(format!("/proc/{}/ns/net", p)));

        // Extract networks
        let networks = self.extract_networks(&info);

        // Extract image name
        let image = info
            .config
            .as_ref()
            .and_then(|c| c.image.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Extract creation time (simplified - just use 0 if parsing fails)
        let created = info
            .created
            .and_then(|s| {
                // Parse RFC3339 timestamp manually to avoid chrono dependency
                // Format: "2024-01-15T10:30:00.123456789Z"
                s.split('T').next().and_then(|date| {
                    let parts: Vec<&str> = date.split('-').collect();
                    if parts.len() == 3 {
                        // Just return a simplified timestamp (days since epoch approximation)
                        parts[0].parse::<i64>().ok()
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(0);

        Ok(Container {
            id: full_id,
            short_id,
            name,
            runtime,
            pid,
            namespace_path,
            networks,
            state,
            created,
            image,
        })
    }

    /// Extracts network information from container inspection data.
    fn extract_networks(&self, info: &ContainerInspectResponse) -> Vec<ContainerNetwork> {
        let mut networks = Vec::new();

        let network_settings = match &info.network_settings {
            Some(ns) => ns,
            None => return networks,
        };

        let network_map = match &network_settings.networks {
            Some(nm) => nm,
            None => return networks,
        };

        for (idx, (network_name, endpoint)) in network_map.iter().enumerate() {
            let ip_address = endpoint
                .ip_address
                .as_ref()
                .and_then(|ip| IpAddr::from_str(ip).ok());

            let gateway = endpoint
                .gateway
                .as_ref()
                .and_then(|gw| IpAddr::from_str(gw).ok());

            // Interface name is typically eth0 for the first network, eth1 for second, etc.
            let interface_name = if idx == 0 {
                "eth0".to_string()
            } else {
                format!("eth{}", idx)
            };

            networks.push(ContainerNetwork {
                network_name: network_name.clone(),
                network_id: endpoint.network_id.clone().unwrap_or_default(),
                interface_name,
                ip_address,
                mac_address: endpoint.mac_address.clone(),
                gateway,
                is_internal: false, // TODO: Query network details to determine this
            });
        }

        networks
    }

    /// Executes a command in a container's network namespace using nsenter.
    #[allow(dead_code)] // Reserved for future container TC operations
    pub async fn exec_in_netns(&self, container: &Container, cmd: &[&str]) -> Result<String> {
        if let Some(ns_path) = &container.namespace_path {
            // Use nsenter to execute in the network namespace
            let ns_arg = format!("--net={}", ns_path.display());

            let output = Command::new("nsenter")
                .arg(&ns_arg)
                .args(cmd)
                .output()
                .await
                .context("Failed to execute nsenter")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("nsenter command failed: {}", stderr);
            }

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            // Fall back to container exec
            self.exec_in_container(container, cmd).await
        }
    }

    /// Executes a command inside a container using docker/podman exec.
    #[allow(dead_code)] // Reserved for future container TC operations
    async fn exec_in_container(&self, container: &Container, cmd: &[&str]) -> Result<String> {
        let runtime_cmd = match container.runtime {
            ContainerRuntime::Docker => "docker",
            ContainerRuntime::Podman => "podman",
        };

        let output = Command::new(runtime_cmd)
            .arg("exec")
            .arg(&container.id)
            .args(cmd)
            .output()
            .await
            .context(format!("Failed to execute {} exec", runtime_cmd))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("{} exec failed: {}", runtime_cmd, stderr);
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Discovers network interfaces inside a container's namespace.
    ///
    /// Uses nlink to query interfaces via netlink in the container's namespace.
    #[allow(dead_code)] // Reserved for future container interface discovery
    pub async fn discover_container_interfaces(
        &self,
        container: &Container,
    ) -> Result<Vec<String>> {
        if let Some(ns_path) = &container.namespace_path {
            // Create a connection in the container's namespace
            let conn = Connection::<Route>::new_in_namespace_path(ns_path)
                .map_err(|e| anyhow::anyhow!("Failed to connect to container namespace: {}", e))?;

            // Query interfaces
            let links = conn
                .get_links()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get links: {}", e))?;

            // Extract interface names, filter out loopback
            let interfaces: Vec<String> = links
                .iter()
                .filter_map(|link| link.name().map(|s| s.to_string()))
                .filter(|name| name != "lo")
                .collect();

            Ok(interfaces)
        } else {
            Err(anyhow::anyhow!(
                "Container {} has no namespace path",
                container.name
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_state_from_str() {
        assert_eq!(ContainerState::from("running"), ContainerState::Running);
        assert_eq!(ContainerState::from("Running"), ContainerState::Running);
        assert_eq!(ContainerState::from("paused"), ContainerState::Paused);
        assert_eq!(ContainerState::from("exited"), ContainerState::Stopped);
        assert_eq!(ContainerState::from("stopped"), ContainerState::Stopped);
        assert_eq!(ContainerState::from("dead"), ContainerState::Stopped);
        assert_eq!(
            ContainerState::from("unknown_state"),
            ContainerState::Unknown
        );
    }

    #[test]
    fn test_container_runtime_display() {
        assert_eq!(ContainerRuntime::Docker.to_string(), "docker");
        assert_eq!(ContainerRuntime::Podman.to_string(), "podman");
    }

    #[test]
    fn test_container_display_name() {
        let container = Container {
            id: "abc123def456".to_string(),
            short_id: "abc123def456".to_string(),
            name: "my-container".to_string(),
            runtime: ContainerRuntime::Docker,
            pid: Some(12345),
            namespace_path: Some(PathBuf::from("/proc/12345/ns/net")),
            networks: vec![],
            state: ContainerState::Running,
            created: 0,
            image: "nginx:latest".to_string(),
        };

        assert_eq!(container.display_name(), "my-container (abc123def456)");
        assert_eq!(container.namespace_id(), "container:my-container");
    }

    #[test]
    fn test_container_network() {
        let network = ContainerNetwork {
            network_name: "bridge".to_string(),
            network_id: "abc123".to_string(),
            interface_name: "eth0".to_string(),
            ip_address: Some("172.17.0.2".parse().unwrap()),
            mac_address: Some("02:42:ac:11:00:02".to_string()),
            gateway: Some("172.17.0.1".parse().unwrap()),
            is_internal: false,
        };

        assert_eq!(network.network_name, "bridge");
        assert!(network.ip_address.is_some());
    }
}
