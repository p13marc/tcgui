//! Test namespace operations using rtnetlink

use std::fs::File;
use std::os::fd::AsFd;
use std::path::Path;

use futures_util::TryStreamExt;
use nix::sched::{CloneFlags, setns};

fn main() {
    println!("=== Testing namespace operations ===\n");

    // 1. List namespaces from /var/run/netns
    let netns_dir = Path::new("/var/run/netns");
    println!("1. Listing namespaces in {:?}:", netns_dir);

    let namespaces: Vec<String> = if let Ok(entries) = std::fs::read_dir(netns_dir) {
        entries
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect()
    } else {
        println!("   (directory not found or not readable)");
        vec![]
    };

    for ns in &namespaces {
        println!("   - {}", ns);
    }

    if namespaces.is_empty() {
        println!("   (no namespaces found)");
        return;
    }

    // 2. For each namespace, try to enter and list interfaces using rtnetlink
    for ns_name in &namespaces {
        println!("\n2. Testing namespace '{}' with rtnetlink:", ns_name);

        let ns_path = format!("/var/run/netns/{}", ns_name);

        // Save current namespace
        let current_ns = match File::open("/proc/self/ns/net") {
            Ok(f) => f,
            Err(e) => {
                println!("   ERROR: Failed to open current namespace: {}", e);
                continue;
            }
        };

        // Open target namespace
        let target_ns = match File::open(&ns_path) {
            Ok(f) => f,
            Err(e) => {
                println!("   ERROR: Failed to open namespace file {}: {}", ns_path, e);
                continue;
            }
        };

        // Enter target namespace
        if let Err(e) = setns(target_ns.as_fd(), CloneFlags::CLONE_NEWNET) {
            println!("   ERROR: Failed to enter namespace: {}", e);
            continue;
        }

        println!("   Successfully entered namespace!");

        // Use rtnetlink to list interfaces (namespace-aware)
        println!("   Interfaces (via rtnetlink):");
        match list_interfaces_rtnetlink() {
            Ok(interfaces) => {
                for iface in interfaces {
                    println!("      - {}", iface);
                }
            }
            Err(e) => {
                println!("      ERROR: {}", e);
            }
        }

        // Also show /proc/net/dev (namespace-aware)
        println!("   Interfaces (via /proc/net/dev):");
        match std::fs::read_to_string("/proc/net/dev") {
            Ok(content) => {
                for line in content.lines().skip(2) {
                    if let Some(name) = line.split(':').next() {
                        println!("      - {}", name.trim());
                    }
                }
            }
            Err(e) => {
                println!("      ERROR: {}", e);
            }
        }

        // Restore original namespace
        if let Err(e) = setns(current_ns.as_fd(), CloneFlags::CLONE_NEWNET) {
            println!("   WARNING: Failed to restore namespace: {}", e);
        } else {
            println!("   Restored to original namespace");
        }
    }

    println!("\n=== Test complete ===");
}

fn list_interfaces_rtnetlink() -> Result<Vec<String>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create runtime: {}", e))?;

    rt.block_on(async {
        let (connection, handle, _) = rtnetlink::new_connection()
            .map_err(|e| format!("Failed to create rtnetlink connection: {}", e))?;

        tokio::spawn(connection);

        let mut interfaces = Vec::new();
        let mut links = handle.link().get().execute();

        while let Some(msg) = links
            .try_next()
            .await
            .map_err(|e| format!("Failed to get link: {}", e))?
        {
            let name = msg
                .attributes
                .iter()
                .find_map(|attr| {
                    if let netlink_packet_route::link::LinkAttribute::IfName(name) = attr {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| format!("unknown{}", msg.header.index));

            interfaces.push(name);
        }

        Ok(interfaces)
    })
}
