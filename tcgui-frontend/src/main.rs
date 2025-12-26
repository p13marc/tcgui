use clap::{Arg, Command};
use tracing::info;

use tcgui_frontend::app::TcGui;
use tcgui_shared::{ZenohConfig, ZenohMode, errors::ZenohConfigError};

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
            Arg::new("no-multicast")
                .long("no-multicast")
                .action(clap::ArgAction::SetTrue)
                .help("Disable multicast scouting for peer discovery"),
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

    // SAFETY: This is called during single-threaded initialization before any
    // threads are spawned, so there's no risk of data races.
    unsafe { std::env::set_var("RUST_LOG", log_level) };

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

    // Check if any zenoh-specific options are provided
    let has_zenoh_config = matches.contains_id("zenoh-connect")
        || matches.contains_id("zenoh-listen")
        || matches.get_flag("no-multicast")
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

        // Disable multicast scouting if requested
        if matches.get_flag("no-multicast") {
            zenoh_config = zenoh_config.disable_multicast_scouting();
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
                    eprintln!(
                        "Error: Unsupported protocol '{}' in endpoint '{}'. Supported: tcp, udp, tls, quic",
                        protocol, endpoint
                    );
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

        iced::application(
            move || TcGui::new_with_config(zenoh_config.clone()),
            TcGui::update,
            TcGui::view,
        )
        .subscription(TcGui::subscription)
        .run()
    } else {
        // Use default peer mode without specific configuration
        info!("[FRONTEND] Starting tcgui-frontend with default peer mode");

        iced::application(TcGui::new, TcGui::update, TcGui::view)
            .subscription(TcGui::subscription)
            .run()
    }
}
