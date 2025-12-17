//! CLI argument parsing for the TC GUI backend.
//!
//! This module handles command line argument parsing using clap and provides
//! a structured representation of CLI configuration that can be used by
//! other configuration components.

use anyhow::Result;
use clap::{Arg, ArgMatches, Command};

/// CLI configuration structure containing all parsed command line arguments
#[derive(Debug, Clone)]
pub struct CliConfig {
    pub verbose: bool,
    pub exclude_loopback: bool,
    pub backend_name: String,
    pub zenoh_mode: String,
    pub zenoh_connect: Option<String>,
    pub zenoh_listen: Option<String>,
    pub no_multicast: bool,
    pub scenario_dirs: Vec<String>,
    pub no_default_scenarios: bool,
    pub preset_dirs: Vec<String>,
    pub no_default_presets: bool,
}

impl CliConfig {
    /// Parse CLI arguments and create CliConfig
    pub fn from_args() -> Result<Self> {
        let matches = Self::build_cli().get_matches();
        Self::from_matches(&matches)
    }

    /// Create CliConfig from pre-parsed ArgMatches (useful for testing)
    pub fn from_matches(matches: &ArgMatches) -> Result<Self> {
        let verbose = matches.get_flag("verbose");
        let exclude_loopback = matches.get_flag("exclude-loopback");
        let no_multicast = matches.get_flag("no-multicast");
        let no_default_scenarios = matches.get_flag("no-default-scenarios");
        let no_default_presets = matches.get_flag("no-default-presets");

        let backend_name = matches
            .get_one::<String>("name")
            .ok_or_else(|| anyhow::anyhow!("Backend name is required"))?
            .clone();

        let zenoh_mode = matches
            .get_one::<String>("zenoh-mode")
            .ok_or_else(|| anyhow::anyhow!("Zenoh mode is required"))?
            .clone();

        let zenoh_connect = matches.get_one::<String>("zenoh-connect").cloned();
        let zenoh_listen = matches.get_one::<String>("zenoh-listen").cloned();

        let scenario_dirs: Vec<String> = matches
            .get_many::<String>("scenario-dir")
            .map(|vals| vals.cloned().collect())
            .unwrap_or_default();

        let preset_dirs: Vec<String> = matches
            .get_many::<String>("preset-dir")
            .map(|vals| vals.cloned().collect())
            .unwrap_or_default();

        Ok(Self {
            verbose,
            exclude_loopback,
            backend_name,
            zenoh_mode,
            zenoh_connect,
            zenoh_listen,
            no_multicast,
            scenario_dirs,
            no_default_scenarios,
            preset_dirs,
            no_default_presets,
        })
    }

    /// Build the clap Command structure
    pub fn build_cli() -> Command {
        Command::new("tcgui-backend")
            .version(env!("CARGO_PKG_VERSION"))
            .about("TC GUI Backend - Privileged network operations")
            .long_about("A privileged backend service for TC GUI that handles network interface \
                       discovery, traffic control configuration, and bandwidth monitoring across \
                       multiple network namespaces.")
            .arg(
                Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .action(clap::ArgAction::SetTrue)
                    .help("Enable verbose logging")
                    .long_help("Enable verbose debug logging. This will show detailed information \
                              about network operations, TC commands, and Zenoh communication."),
            )
            .arg(
                Arg::new("exclude-loopback")
                    .long("exclude-loopback")
                    .action(clap::ArgAction::SetTrue)
                    .help("Exclude loopback interface (lo) from monitoring")
                    .long_help("Exclude the loopback interface from interface discovery and monitoring. \
                              This can reduce noise in environments where loopback interfaces are not relevant."),
            )
            .arg(
                Arg::new("name")
                    .short('n')
                    .long("name")
                    .value_name("BACKEND_NAME")
                    .help("Unique name for this backend instance")
                    .long_help("Unique name for this backend instance, required for multi-backend setups. \
                              This name is used for Zenoh topic routing and service discovery.")
                    .required(false)
                    .default_value("default"),
            )
            .arg(
                Arg::new("zenoh-mode")
                    .long("zenoh-mode")
                    .value_name("MODE")
                    .help("Zenoh session mode: peer or client")
                    .long_help("Zenoh session mode. 'peer' mode enables direct peer-to-peer communication \
                              and can act as a router. 'client' mode connects to existing Zenoh routers.")
                    .value_parser(["peer", "client"])
                    .required(false)
                    .default_value("peer"),
            )
            .arg(
                Arg::new("zenoh-connect")
                    .long("zenoh-connect")
                    .value_name("ENDPOINTS")
                    .help("Zenoh connect endpoints (comma-separated)")
                    .long_help("Comma-separated list of Zenoh endpoints to connect to. \
                              Examples: tcp/192.168.1.1:7447, udp/multicast.address:7447, \
                              tls/secure.host:7448")
                    .required(false),
            )
            .arg(
                Arg::new("zenoh-listen")
                    .long("zenoh-listen")
                    .value_name("ENDPOINTS")
                    .help("Zenoh listen endpoints (comma-separated)")
                    .long_help("Comma-separated list of Zenoh endpoints to listen on. \
                              Examples: tcp/0.0.0.0:7447, udp/0.0.0.0:7447")
                    .required(false),
            )
            .arg(
                Arg::new("no-multicast")
                    .long("no-multicast")
                    .action(clap::ArgAction::SetTrue)
                    .help("Disable multicast scouting for peer discovery")
                    .long_help("Disable multicast scouting for automatic peer discovery. \
                              When disabled, you must explicitly specify connect endpoints. \
                              Useful in environments where multicast is not available or desired."),
            )
            .arg(
                Arg::new("scenario-dir")
                    .long("scenario-dir")
                    .value_name("DIRECTORY")
                    .action(clap::ArgAction::Append)
                    .help("Additional directory to load scenario files from")
                    .long_help("Additional directory to scan for .json5 scenario files. \
                              Can be specified multiple times. Directories are scanned in order \
                              with later ones taking priority (can override scenarios with same ID). \
                              Default directories: /usr/share/tcgui/scenarios, ~/.config/tcgui/scenarios, ./scenarios"),
            )
            .arg(
                Arg::new("no-default-scenarios")
                    .long("no-default-scenarios")
                    .action(clap::ArgAction::SetTrue)
                    .help("Disable loading scenarios from default directories")
                    .long_help("Disable automatic loading of scenarios from default directories \
                              (/usr/share/tcgui/scenarios, ~/.config/tcgui/scenarios, ./scenarios). \
                              Only scenarios from explicitly specified --scenario-dir will be loaded."),
            )
            .arg(
                Arg::new("preset-dir")
                    .long("preset-dir")
                    .value_name("DIRECTORY")
                    .action(clap::ArgAction::Append)
                    .help("Additional directory to load preset files from")
                    .long_help("Additional directory to scan for .json5 preset files. \
                              Can be specified multiple times. Directories are scanned in order \
                              with later ones taking priority (can override presets with same ID). \
                              Default directories: /usr/share/tcgui/presets, ~/.config/tcgui/presets, ./presets"),
            )
            .arg(
                Arg::new("no-default-presets")
                    .long("no-default-presets")
                    .action(clap::ArgAction::SetTrue)
                    .help("Disable loading presets from default directories")
                    .long_help("Disable automatic loading of presets from default directories \
                              (/usr/share/tcgui/presets, ~/.config/tcgui/presets, ./presets). \
                              Only presets from explicitly specified --preset-dir will be loaded."),
            )
    }

    /// Validate CLI configuration
    pub fn validate(&self) -> Result<()> {
        // Validate backend name
        if self.backend_name.is_empty() {
            return Err(anyhow::anyhow!("Backend name cannot be empty"));
        }

        // Validate backend name characters (alphanumeric, hyphens, underscores)
        if !self
            .backend_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Backend name can only contain alphanumeric characters, hyphens, and underscores"
            ));
        }

        // Validate zenoh mode
        match self.zenoh_mode.to_lowercase().as_str() {
            "peer" | "client" => {}
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid zenoh mode '{}'. Must be 'peer' or 'client'",
                    self.zenoh_mode
                ))
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_config_default_values() {
        let matches = CliConfig::build_cli()
            .try_get_matches_from(["tcgui-backend"])
            .unwrap();

        let config = CliConfig::from_matches(&matches).unwrap();

        assert!(!config.verbose);
        assert!(!config.exclude_loopback);
        assert!(!config.no_multicast);
        assert!(!config.no_default_scenarios);
        assert!(!config.no_default_presets);
        assert_eq!(config.backend_name, "default");
        assert_eq!(config.zenoh_mode, "peer");
        assert!(config.zenoh_connect.is_none());
        assert!(config.zenoh_listen.is_none());
        assert!(config.scenario_dirs.is_empty());
        assert!(config.preset_dirs.is_empty());
    }

    #[test]
    fn test_cli_config_custom_values() {
        let matches = CliConfig::build_cli()
            .try_get_matches_from([
                "tcgui-backend",
                "--verbose",
                "--exclude-loopback",
                "--name",
                "test-backend",
                "--zenoh-mode",
                "client",
                "--zenoh-connect",
                "tcp/192.168.1.1:7447,udp/192.168.1.2:7447",
                "--zenoh-listen",
                "tcp/0.0.0.0:7447",
                "--scenario-dir",
                "/custom/scenarios",
                "--scenario-dir",
                "/another/dir",
                "--no-default-scenarios",
                "--preset-dir",
                "/custom/presets",
                "--no-default-presets",
            ])
            .unwrap();

        let config = CliConfig::from_matches(&matches).unwrap();

        assert!(config.verbose);
        assert!(config.exclude_loopback);
        assert!(config.no_default_scenarios);
        assert!(config.no_default_presets);
        assert_eq!(config.backend_name, "test-backend");
        assert_eq!(config.zenoh_mode, "client");
        assert_eq!(
            config.zenoh_connect,
            Some("tcp/192.168.1.1:7447,udp/192.168.1.2:7447".to_string())
        );
        assert_eq!(config.zenoh_listen, Some("tcp/0.0.0.0:7447".to_string()));
        assert_eq!(
            config.scenario_dirs,
            vec!["/custom/scenarios", "/another/dir"]
        );
        assert_eq!(config.preset_dirs, vec!["/custom/presets"]);
    }

    #[test]
    fn test_cli_config_validation_success() {
        let config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "valid-name_123".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_cli_config_validation_empty_name() {
        let config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cli_config_validation_invalid_name_characters() {
        let config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "invalid@name!".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_cli_config_validation_invalid_zenoh_mode() {
        let config = CliConfig {
            verbose: false,
            exclude_loopback: false,
            backend_name: "valid-name".to_string(),
            zenoh_mode: "invalid-mode".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_err());
    }
}
