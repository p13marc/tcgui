//! Network namespace utilities using native Linux syscalls.
//!
//! This module provides utilities for executing code within network namespaces
//! using the `setns` syscall directly, eliminating the need to spawn external
//! processes like `nsenter` or `ip netns exec`.
//!
//! # Architecture
//!
//! The module uses `nix::sched::setns` to switch the calling thread's network
//! namespace. Since namespace changes affect the entire thread, operations
//! are executed in a dedicated blocking thread via `tokio::task::spawn_blocking`.
//!
//! # Example
//!
//! ```rust,no_run
//! use tcgui_backend::netns::{run_in_namespace, NamespacePath};
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Run code in a container's network namespace
//! let ns_path = NamespacePath::Path(PathBuf::from("/proc/12345/ns/net"));
//! let result = run_in_namespace(ns_path, || {
//!     // Code here runs in the target namespace
//!     std::fs::read_to_string("/proc/net/dev")
//! }).await??;
//! # Ok(())
//! # }
//! ```

use std::fs::File;

use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use anyhow::Result;
use nix::sched::{CloneFlags, setns};
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// Errors that can occur during namespace operations.
#[derive(Error, Debug)]
pub enum NamespaceError {
    /// Failed to open namespace file
    #[error("Failed to open namespace file {path}: {source}")]
    OpenNamespace {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to enter namespace via setns
    #[error("Failed to enter namespace {path}: {source}")]
    EnterNamespace {
        path: PathBuf,
        #[source]
        source: nix::Error,
    },

    /// Failed to return to original namespace
    #[error("Failed to return to original namespace: {source}")]
    ReturnNamespace {
        #[source]
        source: nix::Error,
    },

    /// Namespace path not found
    #[error("Namespace path does not exist: {0}")]
    NamespaceNotFound(PathBuf),

    /// Traditional namespace not found
    #[error("Network namespace '{0}' not found in /var/run/netns/")]
    TraditionalNamespaceNotFound(String),

    /// Operation failed inside namespace
    #[error("Operation failed inside namespace: {0}")]
    OperationFailed(String),
}

/// Specifies how to locate a network namespace.
#[derive(Debug, Clone)]
pub enum NamespacePath {
    /// The default/host network namespace (no switch needed)
    Default,

    /// A traditional named namespace (found in /var/run/netns/)
    Named(String),

    /// A direct path to a namespace file (e.g., /proc/<pid>/ns/net)
    Path(PathBuf),

    /// A container namespace with name (for lookup in cache)
    Container(String),
}

impl NamespacePath {
    /// Resolves the namespace path to an actual file path.
    ///
    /// Returns `None` for the default namespace (no switch needed).
    pub fn resolve(&self) -> Result<Option<PathBuf>, NamespaceError> {
        match self {
            NamespacePath::Default => Ok(None),

            NamespacePath::Named(name) => {
                let path = PathBuf::from(format!("/var/run/netns/{}", name));
                if path.exists() {
                    Ok(Some(path))
                } else {
                    Err(NamespaceError::TraditionalNamespaceNotFound(name.clone()))
                }
            }

            NamespacePath::Path(path) => {
                if path.exists() {
                    Ok(Some(path.clone()))
                } else {
                    Err(NamespaceError::NamespaceNotFound(path.clone()))
                }
            }

            NamespacePath::Container(name) => {
                // Container namespaces need external resolution (PID lookup)
                // This is a placeholder - actual resolution happens in the caller
                Err(NamespaceError::NamespaceNotFound(PathBuf::from(format!(
                    "container:{}",
                    name
                ))))
            }
        }
    }

    /// Creates a NamespacePath from a namespace string.
    ///
    /// Handles the following formats:
    /// - "default" -> Default namespace
    /// - "container:<name>" -> Container namespace
    /// - Any other string -> Traditional named namespace
    pub fn from_namespace_str(namespace: &str) -> Self {
        if namespace == "default" {
            NamespacePath::Default
        } else if let Some(container_name) = namespace.strip_prefix("container:") {
            NamespacePath::Container(container_name.to_string())
        } else {
            NamespacePath::Named(namespace.to_string())
        }
    }

    /// Creates a NamespacePath for a container given its PID.
    pub fn from_container_pid(pid: u32) -> Self {
        NamespacePath::Path(PathBuf::from(format!("/proc/{}/ns/net", pid)))
    }
}

/// Runs a synchronous closure in a specified network namespace.
///
/// This function:
/// 1. Saves the current network namespace
/// 2. Switches to the target namespace using `setns`
/// 3. Executes the provided closure
/// 4. Returns to the original namespace
///
/// The operation runs in a blocking thread to avoid affecting async tasks.
///
/// # Type Parameters
///
/// * `F` - The closure type (must be Send + 'static)
/// * `T` - The return type (must be Send + 'static)
///
/// # Arguments
///
/// * `namespace` - The target namespace specification
/// * `f` - The closure to execute in the namespace
///
/// # Returns
///
/// * `Ok(Ok(T))` - Success, with the closure's return value
/// * `Ok(Err(e))` - The closure returned an error
/// * `Err(e)` - Namespace switching failed
///
/// # Example
///
/// ```rust,no_run
/// use tcgui_backend::netns::{run_in_namespace, NamespacePath};
///
/// # async fn example() -> anyhow::Result<()> {
/// let ns = NamespacePath::Named("my-namespace".to_string());
/// let interfaces = run_in_namespace(ns, || {
///     // Read network interfaces in the namespace
///     std::fs::read_to_string("/proc/net/dev")
/// }).await??;
/// # Ok(())
/// # }
/// ```
#[instrument(skip(f), fields(namespace = ?namespace))]
pub async fn run_in_namespace<F, T>(namespace: NamespacePath, f: F) -> Result<T, NamespaceError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // For default namespace, just run the closure directly
    if matches!(namespace, NamespacePath::Default) {
        debug!("Running in default namespace, no switch needed");
        return Ok(tokio::task::spawn_blocking(f)
            .await
            .expect("Blocking task panicked"));
    }

    // Resolve the namespace path
    let ns_path = namespace
        .resolve()?
        .ok_or_else(|| NamespaceError::NamespaceNotFound(PathBuf::from("unresolved")))?;

    debug!("Switching to namespace: {:?}", ns_path);

    // Run in a blocking thread since setns affects the whole thread
    tokio::task::spawn_blocking(move || run_in_namespace_sync(&ns_path, f))
        .await
        .expect("Blocking task panicked")
}

/// Synchronous version of namespace execution (runs on current thread).
///
/// This is the core implementation that actually performs the namespace switch.
/// It should only be called from a dedicated thread (not the async runtime).
///
/// # Safety
///
/// This function changes the network namespace of the calling thread.
/// After the closure completes (or panics), it attempts to restore the
/// original namespace.
#[instrument(skip(f), fields(ns_path = %ns_path.display()))]
pub fn run_in_namespace_sync<F, T>(ns_path: &Path, f: F) -> Result<T, NamespaceError>
where
    F: FnOnce() -> T,
{
    // Save current namespace
    let current_ns =
        File::open("/proc/self/ns/net").map_err(|e| NamespaceError::OpenNamespace {
            path: PathBuf::from("/proc/self/ns/net"),
            source: e,
        })?;

    // Open target namespace
    let target_ns = File::open(ns_path).map_err(|e| NamespaceError::OpenNamespace {
        path: ns_path.to_path_buf(),
        source: e,
    })?;

    // Enter target namespace
    setns(target_ns.as_fd(), CloneFlags::CLONE_NEWNET).map_err(|e| {
        NamespaceError::EnterNamespace {
            path: ns_path.to_path_buf(),
            source: e,
        }
    })?;

    debug!("Entered namespace {:?}", ns_path);

    // Execute the closure, catching panics to ensure we restore the namespace
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    // Always try to return to the original namespace
    if let Err(e) = setns(current_ns.as_fd(), CloneFlags::CLONE_NEWNET) {
        // This is critical - log and continue, but the thread is now in wrong namespace
        warn!(
            "Failed to return to original namespace: {}. Thread may be in wrong namespace!",
            e
        );
    } else {
        debug!("Returned to original namespace");
    }

    // Handle panic or return result
    match result {
        Ok(value) => Ok(value),
        Err(panic_payload) => {
            // Re-panic with original payload
            std::panic::resume_unwind(panic_payload);
        }
    }
}

/// Runs a closure that returns a Result in a namespace.
///
/// Convenience wrapper that flattens the error handling for closures
/// that return `Result<T, E>`.
#[instrument(skip(f), fields(namespace = ?namespace))]
pub async fn run_in_namespace_result<F, T, E>(
    namespace: NamespacePath,
    f: F,
) -> Result<T, NamespaceError>
where
    F: FnOnce() -> Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: std::error::Error + Send + 'static,
{
    let result = run_in_namespace(namespace, f).await?;
    result.map_err(|e| NamespaceError::OperationFailed(e.to_string()))
}

/// Reads /proc/net/dev in a specified namespace.
///
/// This is a common operation that reads network interface statistics.
/// Returns the raw contents of /proc/net/dev.
#[instrument(skip_all, fields(namespace = ?namespace))]
pub async fn read_proc_net_dev(namespace: NamespacePath) -> Result<String, NamespaceError> {
    run_in_namespace(namespace, || {
        std::fs::read_to_string("/proc/net/dev").map_err(|e| e.to_string())
    })
    .await?
    .map_err(NamespaceError::OperationFailed)
}

/// Lists network interfaces in a namespace by reading /sys/class/net.
///
/// Returns a list of interface names.
#[instrument(skip_all, fields(namespace = ?namespace))]
pub async fn list_interfaces(namespace: NamespacePath) -> Result<Vec<String>, NamespaceError> {
    run_in_namespace(namespace, || {
        let mut interfaces = Vec::new();
        if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    interfaces.push(name.to_string());
                }
            }
        }
        interfaces
    })
    .await
}

/// Checks if an interface exists in a namespace.
#[instrument(skip_all, fields(namespace = ?namespace, interface = %interface))]
pub async fn interface_exists(
    namespace: NamespacePath,
    interface: &str,
) -> Result<bool, NamespaceError> {
    let iface = interface.to_string();
    run_in_namespace(namespace, move || {
        Path::new(&format!("/sys/class/net/{}", iface)).exists()
    })
    .await
}

/// Gets the operational state of an interface (up/down/unknown).
#[instrument(skip_all, fields(namespace = ?namespace, interface = %interface))]
pub async fn get_interface_operstate(
    namespace: NamespacePath,
    interface: &str,
) -> Result<String, NamespaceError> {
    let iface = interface.to_string();
    run_in_namespace(namespace, move || {
        let path = format!("/sys/class/net/{}/operstate", iface);
        std::fs::read_to_string(&path)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    })
    .await
}

/// Gets interface flags from /sys/class/net/<iface>/flags.
#[instrument(skip_all, fields(namespace = ?namespace, interface = %interface))]
pub async fn get_interface_flags(
    namespace: NamespacePath,
    interface: &str,
) -> Result<u32, NamespaceError> {
    let iface = interface.to_string();
    run_in_namespace(namespace, move || {
        let path = format!("/sys/class/net/{}/flags", iface);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| {
                let s = s.trim().trim_start_matches("0x");
                u32::from_str_radix(s, 16).ok()
            })
            .unwrap_or(0)
    })
    .await
}

/// Checks if an interface has IFF_UP flag set.
pub async fn is_interface_up(
    namespace: NamespacePath,
    interface: &str,
) -> Result<bool, NamespaceError> {
    const IFF_UP: u32 = 0x1;
    let flags = get_interface_flags(namespace, interface).await?;
    Ok(flags & IFF_UP != 0)
}

/// Discovers all named network namespaces from /var/run/netns/.
///
/// This replaces `ip netns list` with a direct filesystem read.
pub fn discover_named_namespaces() -> Vec<String> {
    let netns_dir = Path::new("/var/run/netns");
    if !netns_dir.exists() {
        return Vec::new();
    }

    let mut namespaces = Vec::new();
    if let Ok(entries) = std::fs::read_dir(netns_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                namespaces.push(name.to_string());
            }
        }
    }

    namespaces
}

/// Tests if a namespace is accessible by attempting to open it.
///
/// This is faster than spawning `ip netns exec` just to test access.
pub fn is_namespace_accessible(namespace: &NamespacePath) -> bool {
    match namespace {
        NamespacePath::Default => true,
        NamespacePath::Named(name) => {
            let path = format!("/var/run/netns/{}", name);
            File::open(&path).is_ok()
        }
        NamespacePath::Path(path) => File::open(path).is_ok(),
        NamespacePath::Container(_) => {
            // Container namespaces need PID resolution first
            false
        }
    }
}

/// Resolves a container namespace to its path using cached container info.
///
/// This is a helper for integrating with the container module.
pub fn container_namespace_path(pid: u32) -> PathBuf {
    PathBuf::from(format!("/proc/{}/ns/net", pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_path_from_str() {
        assert!(matches!(
            NamespacePath::from_namespace_str("default"),
            NamespacePath::Default
        ));

        assert!(matches!(
            NamespacePath::from_namespace_str("container:my-app"),
            NamespacePath::Container(name) if name == "my-app"
        ));

        assert!(matches!(
            NamespacePath::from_namespace_str("my-namespace"),
            NamespacePath::Named(name) if name == "my-namespace"
        ));
    }

    #[test]
    fn test_container_pid_path() {
        let path = NamespacePath::from_container_pid(12345);
        if let NamespacePath::Path(p) = path {
            assert_eq!(p, PathBuf::from("/proc/12345/ns/net"));
        } else {
            panic!("Expected Path variant");
        }
    }

    #[test]
    fn test_discover_named_namespaces() {
        // This test just verifies the function doesn't panic
        // Actual namespaces depend on system state
        let namespaces = discover_named_namespaces();
        // Should return empty or actual namespaces, never panic
        // (test just verifies no panic occurs)
    }

    #[test]
    fn test_default_namespace_accessible() {
        assert!(is_namespace_accessible(&NamespacePath::Default));
    }

    #[tokio::test]
    async fn test_run_in_default_namespace() {
        // Running in default namespace should work without issues
        let result = run_in_namespace(NamespacePath::Default, || 42).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_list_interfaces_default() {
        // List interfaces in default namespace
        let result = list_interfaces(NamespacePath::Default).await;
        assert!(result.is_ok());

        let interfaces = result.unwrap();
        // Should at least have loopback
        assert!(interfaces.contains(&"lo".to_string()));
    }

    #[tokio::test]
    async fn test_read_proc_net_dev_default() {
        let result = read_proc_net_dev(NamespacePath::Default).await;
        assert!(result.is_ok());

        let content = result.unwrap();
        // Should contain header and at least lo interface
        assert!(content.contains("Inter-"));
        assert!(content.contains("lo:"));
    }

    #[tokio::test]
    async fn test_interface_exists_default() {
        // Loopback should always exist
        let result = interface_exists(NamespacePath::Default, "lo").await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Non-existent interface
        let result = interface_exists(NamespacePath::Default, "nonexistent999").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_get_interface_operstate() {
        let result = get_interface_operstate(NamespacePath::Default, "lo").await;
        assert!(result.is_ok());
        // Loopback is typically "unknown" or "up"
        let state = result.unwrap();
        assert!(!state.is_empty());
    }

    #[tokio::test]
    async fn test_nonexistent_namespace() {
        let result = run_in_namespace(
            NamespacePath::Named("definitely_does_not_exist_12345".to_string()),
            || 42,
        )
        .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NamespaceError::TraditionalNamespaceNotFound(_)
        ));
    }
}
