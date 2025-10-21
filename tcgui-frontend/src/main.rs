mod app;
mod backend_manager;
mod interface; // Now modular!
mod interface_selector;
mod message_handlers;
mod messages;
mod query_manager;
mod scenario_manager;
mod scenario_view;
mod ui_state;
mod view;
mod zenoh_manager;

use clap::{Arg, Command};
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

use app::TcGui;
use tcgui_shared::{errors::ZenohConfigError, ZenohConfig, ZenohMode};

/// Global storage for the spawned backend process
static SPAWNED_BACKEND: std::sync::OnceLock<Arc<Mutex<Option<Child>>>> = std::sync::OnceLock::new();

/// Sets up signal handlers and cleanup for spawned backend process
fn setup_cleanup_handler() {
    // Set up Ctrl+C handler
    if let Err(e) = ctrlc::set_handler(move || {
        info!("[FRONTEND] Received Ctrl+C, cleaning up spawned backend...");
        cleanup_spawned_backend();
        std::process::exit(0);
    }) {
        error!("Failed to set Ctrl+C handler: {}", e);
    }
}

/// Cleans up the spawned backend process
fn cleanup_spawned_backend() {
    if let Some(backend_handle) = SPAWNED_BACKEND.get() {
        if let Ok(mut handle) = backend_handle.lock() {
            if let Some(mut child) = handle.take() {
                let pid = child.id();
                info!(
                    "[FRONTEND] Terminating spawned backend process with PID: {}",
                    pid
                );

                // Try graceful termination first
                match child.kill() {
                    Ok(_) => {
                        info!("[FRONTEND] Successfully terminated backend process {}", pid);
                        // Wait for the process to actually exit
                        if let Err(e) = child.wait() {
                            error!("Error waiting for backend process {} to exit: {}", pid, e);
                        }
                    }
                    Err(e) => {
                        error!("Failed to terminate backend process {}: {}", pid, e);

                        // Try to kill using system command as fallback
                        info!("[FRONTEND] Attempting to force-kill backend process {} using system command...", pid);
                        let kill_result = std::process::Command::new("kill")
                            .args(["-9", &pid.to_string()])
                            .output();

                        match kill_result {
                            Ok(output) => {
                                if output.status.success() {
                                    info!("Successfully force-killed backend process {}", pid);
                                } else {
                                    error!(
                                        "Failed to force-kill backend process {}: {}",
                                        pid,
                                        String::from_utf8_lossy(&output.stderr)
                                    );
                                }
                            }
                            Err(e) => {
                                error!("Failed to execute kill command for process {}: {}", pid, e);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Spawns a backend process with the given name
fn spawn_backend(backend_name: String, verbose: bool) {
    info!("[FRONTEND] Attempting to spawn backend: {}", backend_name);

    // First, check if backend binary exists in the expected location
    let backend_path = "./target/release/tcgui-backend";
    let debug_backend_path = "./target/debug/tcgui-backend";

    // Try debug build first, then release build (consistent with new debug-first approach)
    let (binary_path, is_debug) = if std::path::Path::new(debug_backend_path).exists() {
        (debug_backend_path, true)
    } else if std::path::Path::new(backend_path).exists() {
        (backend_path, false)
    } else {
        error!(
            "Backend binary not found at {} or {}",
            debug_backend_path, backend_path
        );
        error!(
            "Please build the backend with: cargo build (or cargo build --release for optimized)"
        );
        std::process::exit(1);
    };

    info!(
        "[FRONTEND] Using {} backend binary at: {}",
        if is_debug { "debug" } else { "release" },
        binary_path
    );

    // Build the command with sudo for root privileges
    let mut cmd = ProcessCommand::new("sudo");
    cmd.arg(binary_path)
        .arg("--exclude-loopback")
        .arg("--name")
        .arg(&backend_name);

    if verbose {
        cmd.arg("--verbose");
    }

    // Spawn the backend process in the background
    match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => {
            let pid = child.id();
            info!(
                "[FRONTEND] Successfully spawned backend '{}' with PID: {}",
                backend_name, pid
            );

            // Store the child process for cleanup on exit
            let backend_handle = SPAWNED_BACKEND.get_or_init(|| Arc::new(Mutex::new(None)));
            if let Ok(mut handle) = backend_handle.lock() {
                *handle = Some(child);
            }

            // Set up cleanup handler for when the frontend exits
            setup_cleanup_handler();

            // Give the backend a moment to start up
            std::thread::sleep(std::time::Duration::from_millis(1500));
            info!("[FRONTEND] Backend should be ready to accept connections");
        }
        Err(e) => {
            error!(
                "[FRONTEND] Failed to spawn backend '{}': {}",
                backend_name, e
            );
            error!(
                "Make sure you can run: sudo {} --exclude-loopback --name {}",
                binary_path, backend_name
            );
            error!("If permission denied, check if sudo is available and configured properly");
            std::process::exit(1);
        }
    }
}

pub fn main() -> iced::Result {
    let matches = Command::new("tcgui-frontend")
        .about("TC GUI Frontend - User interface")
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(clap::ArgAction::SetTrue)
                .help("Enable verbose logging"),
        )
        .arg(
            Arg::new("zenoh-mode")
                .long("zenoh-mode")
                .value_name("MODE")
                .help("Zenoh session mode: peer or client")
                .required(false)
                .default_value("peer"),
        )
        .arg(
            Arg::new("zenoh-connect")
                .long("zenoh-connect")
                .value_name("ENDPOINTS")
                .help("Zenoh connect endpoints (comma-separated, e.g., tcp/192.168.1.1:7447)")
                .required(false),
        )
        .arg(
            Arg::new("zenoh-listen")
                .long("zenoh-listen")
                .value_name("ENDPOINTS")
                .help("Zenoh listen endpoints (comma-separated, e.g., tcp/0.0.0.0:7447)")
                .required(false),
        )
        .arg(
            Arg::new("backend")
                .long("backend")
                .value_name("NAME")
                .help("Automatically spawn backend with the given name (e.g., trefze3)")
                .required(false),
        )
        .get_matches();

    // Initialize logging with component prefixes and appropriate levels
    let log_level = if matches.get_flag("verbose") {
        "debug"
    } else if std::env::var("RUST_LOG").is_err() {
        // Default to info level, but filter out overly verbose crates
        "info,wgpu_core=warn,wgpu_hal=warn,naga=warn,winit=warn,zenoh=info"
    } else {
        // Respect existing RUST_LOG but still filter noisy crates
        &std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
    };

    std::env::set_var("RUST_LOG", log_level);

    // Initialize tracing with component prefix for frontend
    tracing_subscriber::fmt()
        .with_target(false) // Don't show the module target
        .with_level(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true)
        .event_format(
            tracing_subscriber::fmt::format()
                .with_target(false)
                .compact(),
        )
        .init();

    // Handle backend spawning if requested
    if let Some(backend_name) = matches.get_one::<String>("backend") {
        spawn_backend(backend_name.clone(), matches.get_flag("verbose"));
    }

    // Check if any zenoh-specific options are provided
    let has_zenoh_config = matches.contains_id("zenoh-connect")
        || matches.contains_id("zenoh-listen")
        || (matches
            .get_one::<String>("zenoh-mode")
            .is_some_and(|mode| mode != "peer"));

    if has_zenoh_config {
        // Parse zenoh configuration only if options are provided
        let zenoh_mode_str = matches
            .get_one::<String>("zenoh-mode")
            .expect("zenoh-mode has a default value and should always be present");
        let zenoh_mode = match zenoh_mode_str.to_lowercase().as_str() {
            "peer" => ZenohMode::Peer,
            "client" => ZenohMode::Client,
            _ => {
                tracing::error!("Invalid zenoh mode: {}, using peer", zenoh_mode_str);
                ZenohMode::Peer
            }
        };

        let mut zenoh_config = ZenohConfig {
            mode: zenoh_mode,
            endpoints: vec![],
            properties: std::collections::HashMap::new(),
        };

        // Add connect endpoints if specified
        if let Some(connect_endpoints) = matches.get_one::<String>("zenoh-connect") {
            for endpoint in connect_endpoints.split(',') {
                zenoh_config = zenoh_config.add_connect_endpoint(endpoint.trim());
            }
        }

        // Add listen endpoints if specified
        if let Some(listen_endpoints) = matches.get_one::<String>("zenoh-listen") {
            for endpoint in listen_endpoints.split(',') {
                zenoh_config = zenoh_config.add_listen_endpoint(endpoint.trim());
            }
        }

        // Validate zenoh configuration
        if let Err(e) = zenoh_config.validate() {
            match e {
                ZenohConfigError::InvalidMode { mode } => {
                    eprintln!(
                        "Error: Invalid zenoh mode '{}'. Use 'peer' or 'client'.",
                        mode
                    );
                }
                ZenohConfigError::InvalidEndpoint { endpoint, reason } => {
                    eprintln!("Error: Invalid endpoint '{}' - {}", endpoint, reason);
                }
                ZenohConfigError::InvalidProtocol { protocol, endpoint } => {
                    eprintln!("Error: Unsupported protocol '{}' in endpoint '{}'. Supported: tcp, udp, tls, quic", protocol, endpoint);
                }
                ZenohConfigError::ModeEndpointMismatch { mode, reason } => {
                    eprintln!("Error: {:?} mode {}", mode, reason);
                }
                ZenohConfigError::InvalidAddress {
                    address,
                    protocol,
                    reason,
                } => {
                    eprintln!(
                        "Error: Invalid {} address '{}' - {}",
                        protocol, address, reason
                    );
                }
                _ => {
                    eprintln!("Error: Invalid zenoh configuration: {}", e);
                }
            }
            std::process::exit(1);
        }

        info!("[FRONTEND] Starting tcgui-frontend with custom zenoh config");
        info!(
            "Zenoh configuration - Mode: {:?}, Endpoints: {:?}",
            zenoh_config.mode, zenoh_config.endpoints
        );

        let result = iced::application(
            move || TcGui::new_with_config(zenoh_config.clone()),
            TcGui::update,
            TcGui::view,
        )
        .subscription(TcGui::subscription)
        .run();

        // Clean up spawned backend when frontend exits
        cleanup_spawned_backend();
        result
    } else {
        // Use default peer mode without specific configuration
        info!("[FRONTEND] Starting tcgui-frontend with default peer mode");

        let result = iced::application(TcGui::new, TcGui::update, TcGui::view)
            .subscription(TcGui::subscription)
            .run();

        // Clean up spawned backend when frontend exits
        cleanup_spawned_backend();
        result
    }
}
