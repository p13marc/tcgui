//! Test namespace operations using nlink

use std::path::Path;

use nlink::netlink::{Connection, Route, namespace};

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

    // 2. For each namespace, try to enter and list interfaces using nlink
    for ns_name in &namespaces {
        println!("\n2. Testing namespace '{}' with nlink:", ns_name);

        // Method 1: Use namespace::enter() to temporarily enter the namespace
        println!("   Interfaces (via namespace::enter + /proc/net/dev):");
        match namespace::enter(ns_name) {
            Ok(_guard) => {
                // While guard is held, we're in the namespace
                match std::fs::read_to_string("/proc/net/dev") {
                    Ok(content) => {
                        for line in content.lines().skip(2) {
                            if let Some(name) = line.split(':').next() {
                                println!("      - {}", name.trim());
                            }
                        }
                    }
                    Err(e) => {
                        println!("      ERROR reading /proc/net/dev: {}", e);
                    }
                }
                // Guard drops here, restoring original namespace
            }
            Err(e) => {
                println!("   ERROR: Failed to enter namespace: {}", e);
                continue;
            }
        }

        // Method 2: Use Connection::new_in_namespace_path for netlink queries
        println!("   Interfaces (via nlink Connection in namespace):");
        match list_interfaces_nlink(ns_name) {
            Ok(interfaces) => {
                for iface in interfaces {
                    println!("      - {}", iface);
                }
            }
            Err(e) => {
                println!("      ERROR: {}", e);
            }
        }
    }

    println!("\n=== Test complete ===");
}

fn list_interfaces_nlink(ns_name: &str) -> Result<Vec<String>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create runtime: {}", e))?;

    rt.block_on(async {
        let ns_path = format!("/var/run/netns/{}", ns_name);

        // Create a connection directly in the target namespace
        let conn = Connection::<Route>::new_in_namespace_path(&ns_path)
            .map_err(|e| format!("Failed to create nlink connection in namespace: {}", e))?;

        // Query all links
        let links = conn
            .get_links()
            .await
            .map_err(|e| format!("Failed to get links: {}", e))?;

        let interfaces: Vec<String> = links
            .iter()
            .filter_map(|link| link.name().map(|s| s.to_string()))
            .collect();

        Ok(interfaces)
    })
}
