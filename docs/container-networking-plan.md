# Container Networking Support Plan

## Overview

Add support for discovering and managing network interfaces inside Docker and Podman containers. This enables applying TC rules to container networks for testing distributed applications under degraded network conditions.

## Problem Statement

Currently, TC GUI discovers namespaces via `ip netns list`, but:

1. **Docker/Podman don't use `ip netns`**: Containers create network namespaces directly via `clone()` syscall, not through `ip netns add`
2. **Namespace paths differ**: Container namespaces are at `/proc/<pid>/ns/net`, not `/var/run/netns/`
3. **Container context needed**: Users want to see "container-name" not "namespace-xyz"
4. **Internal networks**: Podman `--internal` networks are isolated and need special handling

## Container Networking Architecture

### Docker Networking

```
┌─────────────────────────────────────────────────────────────┐
│ Host Network Namespace                                       │
│  ┌─────────┐     ┌─────────┐                                │
│  │ docker0 │     │ br-xxxx │  (user-defined bridge)         │
│  │ bridge  │     │ bridge  │                                │
│  └────┬────┘     └────┬────┘                                │
│       │               │                                      │
│   vethXXXX        vethYYYY                                  │
│       │               │                                      │
└───────┼───────────────┼─────────────────────────────────────┘
        │               │
┌───────┴───────┐ ┌─────┴───────┐
│ Container NS  │ │ Container NS │
│   eth0        │ │   eth0       │
│ 172.17.0.2    │ │ 172.18.0.2   │
└───────────────┘ └──────────────┘
```

### Podman Networking (Rootless)

```
┌─────────────────────────────────────────────────────────────┐
│ User's Network Namespace (slirp4netns or pasta)             │
│                                                              │
│  ┌──────────────┐    TAP device                             │
│  │ slirp4netns  │◄────────────► Container NS                │
│  │ or pasta     │               eth0 (10.0.2.x)             │
│  └──────────────┘                                           │
└─────────────────────────────────────────────────────────────┘

OR with --network bridge (rootful):
┌─────────────────────────────────────────────────────────────┐
│ Host Network Namespace                                       │
│  ┌─────────┐                                                │
│  │ podman0 │  CNI/netavark bridge                           │
│  │ bridge  │                                                │
│  └────┬────┘                                                │
│   vethXXXX                                                  │
└───────┼─────────────────────────────────────────────────────┘
        │
┌───────┴───────┐
│ Container NS  │
│   eth0        │
└───────────────┘
```

### Internal Networks (`--internal`)

- No connection to host or external networks
- Containers can only communicate with each other on same network
- Bridge exists but no NAT/masquerade rules
- TC on internal bridge affects all container-to-container traffic

## Discovery Methods

### Method 1: Container Runtime APIs

Query Docker/Podman directly for container info:

```rust
// Docker API (via unix socket or HTTP)
GET /containers/json
GET /containers/{id}/json  // includes NetworkSettings.SandboxKey = namespace path

// Podman API (compatible with Docker API)
GET /v4.0.0/libpod/containers/json
GET /v4.0.0/libpod/containers/{id}/json
```

**Pros:**
- Rich metadata (container name, image, networks, IPs)
- Official supported API
- Works for rootless podman

**Cons:**
- Requires socket access (`/var/run/docker.sock` or podman socket)
- Different API versions between Docker/Podman
- Additional dependency

### Method 2: Process Namespace Scanning

Scan `/proc` for container processes and their namespaces:

```rust
// Find container processes (they typically have specific cgroup patterns)
// /proc/<pid>/cgroup contains docker/podman identifiers
// /proc/<pid>/ns/net is the network namespace

async fn discover_container_namespaces() -> Vec<ContainerNamespace> {
    let mut containers = Vec::new();
    
    for entry in fs::read_dir("/proc")? {
        let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
        
        // Check if it's a container process
        let cgroup = fs::read_to_string(format!("/proc/{}/cgroup", pid))?;
        if cgroup.contains("docker") || cgroup.contains("libpod") {
            let ns_path = format!("/proc/{}/ns/net", pid);
            // ... extract container ID from cgroup path
        }
    }
    
    containers
}
```

**Pros:**
- No runtime API dependency
- Works with any container runtime
- Direct namespace access

**Cons:**
- Fragile cgroup parsing
- Race conditions (container may exit)
- Less metadata available
- Requires read access to /proc/<pid>

### Method 3: Hybrid Approach (Recommended)

1. Try container runtime APIs first (rich metadata)
2. Fall back to /proc scanning if APIs unavailable
3. Use `nsenter` to execute commands in container namespace

```rust
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub runtime: ContainerRuntime,
    pub namespace_path: String,      // /proc/<pid>/ns/net
    pub networks: Vec<ContainerNetwork>,
    pub state: ContainerState,
}

pub enum ContainerRuntime {
    Docker,
    Podman,
    Containerd,
    Unknown,
}

pub struct ContainerNetwork {
    pub name: String,               // "bridge", "my-network", etc.
    pub interface: String,          // "eth0" inside container
    pub ip_address: Option<String>,
    pub gateway: Option<String>,
    pub is_internal: bool,          // --internal flag
}
```

## Namespace Access

To run commands inside a container's network namespace:

```bash
# Using nsenter (requires CAP_SYS_ADMIN or root)
nsenter --net=/proc/<pid>/ns/net ip link show
nsenter --net=/proc/<pid>/ns/net tc qdisc show dev eth0

# Using container runtime exec (if container is running)
docker exec <container> tc qdisc show dev eth0
podman exec <container> tc qdisc show dev eth0
```

**Important**: `nsenter` with `--net` requires:
- `CAP_SYS_PTRACE` to access another process's namespace
- `CAP_NET_ADMIN` to modify network settings
- Or root access

For rootless Podman, we'd need to:
1. Enter the user namespace first
2. Then enter the network namespace
3. Or use `podman exec` which handles this

## Implementation Design

### New Types

```rust
// src/container.rs

use std::path::PathBuf;

/// Container runtime detection and management
pub struct ContainerManager {
    docker_socket: Option<PathBuf>,
    podman_socket: Option<PathBuf>,
    runtime_available: Vec<ContainerRuntime>,
}

/// Discovered container with network info
#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub short_id: String,           // First 12 chars
    pub name: String,
    pub runtime: ContainerRuntime,
    pub pid: Option<u32>,           // Main process PID
    pub namespace_path: Option<PathBuf>,
    pub networks: Vec<ContainerNetwork>,
    pub state: ContainerState,
    pub created: u64,
    pub image: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Docker,
    Podman,
    Containerd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct ContainerNetwork {
    pub network_name: String,
    pub network_id: String,
    pub interface_name: String,     // Usually "eth0"
    pub ip_address: Option<IpAddr>,
    pub mac_address: Option<String>,
    pub gateway: Option<IpAddr>,
    pub is_internal: bool,
}

impl ContainerManager {
    pub async fn new() -> Self {
        let docker_socket = Self::find_docker_socket();
        let podman_socket = Self::find_podman_socket();
        
        let mut runtime_available = Vec::new();
        if docker_socket.is_some() {
            runtime_available.push(ContainerRuntime::Docker);
        }
        if podman_socket.is_some() {
            runtime_available.push(ContainerRuntime::Podman);
        }
        
        Self {
            docker_socket,
            podman_socket,
            runtime_available,
        }
    }
    
    fn find_docker_socket() -> Option<PathBuf> {
        let paths = [
            PathBuf::from("/var/run/docker.sock"),
            PathBuf::from("/run/docker.sock"),
        ];
        paths.into_iter().find(|p| p.exists())
    }
    
    fn find_podman_socket() -> Option<PathBuf> {
        // Rootful
        let rootful = PathBuf::from("/run/podman/podman.sock");
        if rootful.exists() {
            return Some(rootful);
        }
        
        // Rootless (user socket)
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let user_socket = PathBuf::from(runtime_dir).join("podman/podman.sock");
            if user_socket.exists() {
                return Some(user_socket);
            }
        }
        
        None
    }
    
    pub async fn discover_containers(&self) -> Result<Vec<Container>> {
        let mut containers = Vec::new();
        
        // Try Docker
        if let Some(socket) = &self.docker_socket {
            if let Ok(docker_containers) = self.query_docker(socket).await {
                containers.extend(docker_containers);
            }
        }
        
        // Try Podman
        if let Some(socket) = &self.podman_socket {
            if let Ok(podman_containers) = self.query_podman(socket).await {
                containers.extend(podman_containers);
            }
        }
        
        Ok(containers)
    }
    
    /// Execute a command in the container's network namespace
    pub async fn exec_in_netns(&self, container: &Container, cmd: &[&str]) -> Result<String> {
        if let Some(ns_path) = &container.namespace_path {
            // Use nsenter
            let mut args = vec!["--net=".to_string() + ns_path.to_str().unwrap()];
            args.extend(cmd.iter().map(|s| s.to_string()));
            
            let output = Command::new("nsenter")
                .args(&args)
                .output()
                .await?;
                
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            // Fall back to container exec
            match container.runtime {
                ContainerRuntime::Docker => {
                    let output = Command::new("docker")
                        .args(["exec", &container.id])
                        .args(cmd)
                        .output()
                        .await?;
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                }
                ContainerRuntime::Podman => {
                    let output = Command::new("podman")
                        .args(["exec", &container.id])
                        .args(cmd)
                        .output()
                        .await?;
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                }
                _ => Err(anyhow::anyhow!("Unsupported runtime")),
            }
        }
    }
}
```

### Integration with NetworkManager

```rust
// Extend NetworkManager to include containers

impl NetworkManager {
    pub async fn discover_all_interfaces(&self) -> Result<Vec<NetworkNamespace>> {
        let mut namespaces = Vec::new();
        
        // 1. Traditional namespaces (existing code)
        let traditional = self.discover_traditional_namespaces().await?;
        namespaces.extend(traditional);
        
        // 2. Container namespaces (new)
        if self.container_manager.is_available() {
            let containers = self.container_manager.discover_containers().await?;
            for container in containers {
                let ns = self.container_to_namespace(container).await?;
                namespaces.push(ns);
            }
        }
        
        Ok(namespaces)
    }
    
    fn container_to_namespace(&self, container: Container) -> NetworkNamespace {
        NetworkNamespace {
            name: format!("container:{}", container.name),
            display_name: Some(container.name.clone()),
            namespace_type: NamespaceType::Container {
                runtime: container.runtime,
                container_id: container.id.clone(),
                image: container.image.clone(),
            },
            interfaces: /* discover via nsenter */,
        }
    }
}
```

### Shared Types Updates

```rust
// tcgui-shared/src/lib.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NamespaceType {
    /// Traditional network namespace (ip netns)
    Traditional,
    /// Default/root namespace
    Default,
    /// Container namespace
    Container {
        runtime: String,        // "docker", "podman"
        container_id: String,
        image: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNamespace {
    pub name: String,                       // Unique identifier
    pub display_name: Option<String>,       // Human-readable name
    pub namespace_type: NamespaceType,
    pub interfaces: HashMap<String, NetworkInterface>,
}
```

### Frontend Display

Show containers distinctly in the UI:

```
┌─────────────────────────────────────────────────────────────┐
│ 📦 Container: my-webapp (nginx:latest)                      │
│    Runtime: Podman | Network: bridge                        │
├─────────────────────────────────────────────────────────────┤
│ eth0 [UP]  172.17.0.5    📈 1.2M 📤 340K    [TC: none]     │
└─────────────────────────────────────────────────────────────┘
```

## TC Application Points

### Where to Apply TC for Containers

1. **Inside container namespace** (eth0):
   - Affects only that container's traffic
   - Most precise control
   - Requires namespace access

2. **On host veth endpoint** (vethXXXX):
   - Affects container from host perspective
   - Easier access (no namespace switching)
   - May affect all traffic to/from container

3. **On bridge** (docker0, podman0):
   - Affects all containers on that network
   - Good for simulating network-wide conditions
   - Simpler but less granular

### Recommended Approach

Default to applying TC inside the container namespace for precision:

```rust
pub async fn apply_tc_to_container(
    &self,
    container: &Container,
    interface: &str,  // Usually "eth0"
    config: &TcConfig,
) -> Result<()> {
    // Build tc command
    let tc_args = self.build_tc_args(config);
    
    // Execute in container's network namespace
    self.container_manager.exec_in_netns(
        container,
        &["tc", "qdisc", "replace", "dev", interface, "root", "netem", &tc_args]
    ).await
}
```

## Implementation Phases

### Phase 1: Container Discovery (3-4 hours)

1. Create `src/container.rs` with `ContainerManager`
2. Implement Docker socket discovery and API queries
3. Implement Podman socket discovery and API queries
4. Add container types to `tcgui-shared`

### Phase 2: Namespace Integration (2-3 hours)

1. Integrate `ContainerManager` into `NetworkManager`
2. Add `nsenter`-based command execution
3. Discover interfaces inside container namespaces
4. Map container networks to interfaces

### Phase 3: TC Operations (2-3 hours)

1. Extend `TcCommandBuilder` for container namespaces
2. Add container-aware TC apply/remove commands
3. Handle namespace switching for TC operations
4. Add bandwidth monitoring for container interfaces

### Phase 4: Frontend Updates (2-3 hours)

1. Display containers with distinct styling
2. Show container metadata (image, runtime, networks)
3. Add container-specific icons
4. Group by runtime or network

### Phase 5: Internal Network Support (2 hours)

1. Detect `--internal` networks
2. Show internal network indicator in UI
3. Document limitations (no external connectivity to test)

## Required Capabilities

The backend will need additional capabilities:

```bash
# Current
setcap cap_net_admin+ep tcgui-backend

# With container support (if using nsenter)
setcap cap_net_admin,cap_sys_ptrace+ep tcgui-backend
```

Or run as root for full container namespace access.

Alternative: Use `docker exec`/`podman exec` which handles namespace switching internally.

## Configuration Options

```json5
// ~/.config/tcgui/config.json5
{
  "containers": {
    "enabled": true,
    "docker": {
      "enabled": true,
      "socket": "/var/run/docker.sock"
    },
    "podman": {
      "enabled": true,
      "socket": "auto"  // Auto-detect rootful/rootless
    },
    "discovery_interval_secs": 5,
    "show_stopped_containers": false
  }
}
```

## Estimated Effort

| Phase | Effort | Cumulative |
|-------|--------|------------|
| Container Discovery | 3-4 hours | 3-4 hours |
| Namespace Integration | 2-3 hours | 5-7 hours |
| TC Operations | 2-3 hours | 7-10 hours |
| Frontend Updates | 2-3 hours | 9-13 hours |
| Internal Networks | 2 hours | 11-15 hours |

**Total: 11-15 hours** for full implementation

## Testing Strategy

1. **Unit tests**: Mock container runtime responses
2. **Integration tests**: Test with real Docker/Podman containers
3. **Manual testing scenarios**:
   - Single container with bridge network
   - Multiple containers on same network
   - Internal network (--internal)
   - Rootless Podman
   - Container start/stop detection

## Dependencies

- `hyper` or `reqwest` with Unix socket support for API queries
- `serde_json` for API response parsing (already used)
- `nix` crate for namespace operations (optional, can use `nsenter` command)

## Future Enhancements

- Kubernetes pod support (via CRI)
- Container network topology view
- Auto-apply TC rules when container starts
- TC profiles per container image
- Integration with container compose files
