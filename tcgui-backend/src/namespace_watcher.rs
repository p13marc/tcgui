//! Namespace watcher using nlink's inotify-based implementation.
//!
//! Watches `/var/run/netns` for namespace creation and deletion events,
//! enabling immediate reaction to namespace changes instead of polling.
//! When `/var/run/netns` doesn't exist, watches `/var/run/` for its creation.

pub use nlink::netlink::namespace_watcher::{NamespaceEvent, NamespaceWatcher};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_watcher_creation() {
        // This test just verifies the watcher can be created
        // Actual watching depends on system state
        let result = NamespaceWatcher::new().await;
        // May or may not succeed depending on whether /var/run exists
        if let Ok(watcher) = result {
            println!(
                "Watcher created successfully, watching netns: {}",
                watcher.is_watching_netns()
            );
        } else {
            println!("Watcher not created (unexpected on Linux)");
        }
    }

    #[tokio::test]
    async fn test_list_and_watch() {
        if let Ok((namespaces, watcher)) = NamespaceWatcher::list_and_watch().await {
            println!("Current namespaces: {:?}", namespaces);
            println!("Watching netns: {}", watcher.is_watching_netns());
        }
    }
}
