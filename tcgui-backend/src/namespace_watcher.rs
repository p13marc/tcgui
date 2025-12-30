//! Inotify-based watcher for network namespace changes.
//!
//! Watches `/var/run/netns` for namespace creation and deletion events,
//! enabling immediate reaction to namespace changes instead of polling.

use std::path::Path;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Events emitted when namespaces change.
#[derive(Debug, Clone)]
pub enum NamespaceEvent {
    /// A new namespace was created
    Created(String),
    /// A namespace was deleted
    Deleted(String),
}

/// Watches `/var/run/netns` for namespace changes using inotify.
pub struct NamespaceWatcher {
    _watcher: RecommendedWatcher,
}

impl NamespaceWatcher {
    /// Creates a new namespace watcher and returns a receiver for events.
    ///
    /// Returns `None` if `/var/run/netns` doesn't exist or can't be watched.
    pub fn new(buffer_size: usize) -> Option<(Self, mpsc::Receiver<NamespaceEvent>)> {
        let netns_path = Path::new("/var/run/netns");

        // Check if the directory exists
        if !netns_path.exists() {
            warn!("/var/run/netns does not exist, namespace watching disabled");
            return None;
        }

        let (tx, rx) = mpsc::channel(buffer_size);

        // Create the watcher with a callback that sends events
        let tx_clone = tx.clone();
        let watcher =
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    if let Some(ns_event) = Self::process_event(&event) {
                        if tx_clone.blocking_send(ns_event).is_err() {
                            debug!("Namespace event receiver dropped");
                        }
                    }
                }
                Err(e) => {
                    error!("Filesystem watch error: {}", e);
                }
            });

        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create filesystem watcher: {}", e);
                return None;
            }
        };

        // Configure for immediate notification
        if let Err(e) = watcher
            .configure(Config::default().with_poll_interval(std::time::Duration::from_millis(100)))
        {
            warn!("Failed to configure watcher: {}", e);
        }

        // Watch the netns directory
        if let Err(e) = watcher.watch(netns_path, RecursiveMode::NonRecursive) {
            error!("Failed to watch /var/run/netns: {}", e);
            return None;
        }

        info!("Watching /var/run/netns for namespace changes");

        Some((Self { _watcher: watcher }, rx))
    }

    /// Process a notify event and convert to NamespaceEvent if relevant.
    fn process_event(event: &Event) -> Option<NamespaceEvent> {
        // Get the namespace name from the path
        let get_ns_name = |paths: &[std::path::PathBuf]| -> Option<String> {
            paths.first().and_then(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
        };

        match &event.kind {
            EventKind::Create(_) => {
                if let Some(name) = get_ns_name(&event.paths) {
                    info!("Namespace created: {}", name);
                    return Some(NamespaceEvent::Created(name));
                }
            }
            EventKind::Remove(_) => {
                if let Some(name) = get_ns_name(&event.paths) {
                    info!("Namespace deleted: {}", name);
                    return Some(NamespaceEvent::Deleted(name));
                }
            }
            _ => {
                // Ignore other events (modify, access, etc.)
                debug!("Ignoring filesystem event: {:?}", event.kind);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watcher_creation() {
        // This test just verifies the watcher can be created
        // Actual watching depends on system state
        let result = NamespaceWatcher::new(10);
        // May or may not succeed depending on whether /var/run/netns exists
        if result.is_some() {
            println!("Watcher created successfully");
        } else {
            println!("Watcher not created (expected if /var/run/netns doesn't exist)");
        }
    }
}
