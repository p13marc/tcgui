//! Application configuration management for the TC GUI backend.
//!
//! This module handles application-specific configuration including logging,
//! backend behavior settings, and runtime parameters.

use anyhow::Result;
use std::env;
use tracing_subscriber;

use super::cli::CliConfig;

/// Log level enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// Convert to tracing level filter string
    pub fn to_filter_string(&self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

/// Application configuration structure
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub backend_name: String,
    pub exclude_loopback: bool,
    pub log_level: LogLevel,
    pub interface_monitor_interval_secs: u64,
    pub bandwidth_monitor_interval_secs: u64,
    pub scenario_dirs: Vec<String>,
    pub no_default_scenarios: bool,
    pub preset_dirs: Vec<String>,
    pub no_default_presets: bool,
}

impl AppConfig {
    /// Create application configuration from CLI config
    pub fn from_cli(cli_config: &CliConfig) -> Result<Self> {
        let log_level = if cli_config.verbose {
            LogLevel::Debug
        } else {
            // Check environment variable
            match env::var("RUST_LOG") {
                Ok(level_str) => Self::parse_log_level(&level_str),
                Err(_) => LogLevel::Info, // Default
            }
        };

        Ok(Self {
            backend_name: cli_config.backend_name.clone(),
            exclude_loopback: cli_config.exclude_loopback,
            log_level,
            interface_monitor_interval_secs: 5, // Default 5 seconds
            bandwidth_monitor_interval_secs: 2, // Default 2 seconds
            scenario_dirs: cli_config.scenario_dirs.clone(),
            no_default_scenarios: cli_config.no_default_scenarios,
            preset_dirs: cli_config.preset_dirs.clone(),
            no_default_presets: cli_config.no_default_presets,
        })
    }

    /// Parse log level from string
    fn parse_log_level(level_str: &str) -> LogLevel {
        // Extract the main log level from complex RUST_LOG format
        let main_level = level_str
            .split(',')
            .next()
            .unwrap_or(level_str)
            .split('=')
            .next()
            .unwrap_or(level_str)
            .to_lowercase();

        match main_level.as_str() {
            "error" => LogLevel::Error,
            "warn" => LogLevel::Warn,
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "trace" => LogLevel::Trace,
            _ => LogLevel::Info, // Default fallback
        }
    }

    /// Initialize logging based on configuration
    pub fn init_logging(&self) -> Result<()> {
        let log_filter = if self.log_level == LogLevel::Debug || self.log_level == LogLevel::Trace {
            // Verbose logging but filter out overly noisy crates
            format!(
                "{},zenoh_transport=warn,zenoh_runtime=warn,zenoh_protocol=warn,netlink_proto=warn",
                self.log_level.to_filter_string()
            )
        } else {
            // Check if RUST_LOG is already set with custom configuration
            match env::var("RUST_LOG") {
                Ok(existing_log) if !existing_log.is_empty() => {
                    // Respect existing RUST_LOG but still filter noisy crates
                    format!("{},zenoh_transport=warn,zenoh_runtime=warn,zenoh_protocol=warn,netlink_proto=warn", existing_log)
                }
                _ => {
                    // Default configuration
                    format!("{},zenoh_transport=warn,zenoh_runtime=warn,zenoh_protocol=warn,netlink_proto=warn",
                           self.log_level.to_filter_string())
                }
            }
        };

        env::set_var("RUST_LOG", &log_filter);

        // Initialize tracing with structured format
        tracing_subscriber::fmt()
            .with_target(false) // Don't show the module target
            .with_level(true) // Show log level
            .with_thread_ids(false) // Don't show thread IDs
            .with_thread_names(false) // Don't show thread names
            .with_file(false) // Don't show file names
            .with_line_number(false) // Don't show line numbers
            .with_ansi(true) // Enable colors
            .event_format(
                tracing_subscriber::fmt::format()
                    .with_target(false)
                    .compact(),
            )
            .init();

        tracing::info!("Logging initialized with level: {:?}", self.log_level);
        Ok(())
    }

    /// Validate application configuration
    pub fn validate(&self) -> Result<()> {
        // Validate backend name
        if self.backend_name.is_empty() {
            return Err(anyhow::anyhow!("Backend name cannot be empty"));
        }

        // Validate monitoring intervals
        if self.interface_monitor_interval_secs == 0 {
            return Err(anyhow::anyhow!(
                "Interface monitor interval must be greater than 0"
            ));
        }

        if self.bandwidth_monitor_interval_secs == 0 {
            return Err(anyhow::anyhow!(
                "Bandwidth monitor interval must be greater than 0"
            ));
        }

        Ok(())
    }
}

/// Builder pattern for AppConfig
pub struct AppConfigBuilder {
    backend_name: Option<String>,
    exclude_loopback: Option<bool>,
    log_level: Option<LogLevel>,
    interface_monitor_interval_secs: Option<u64>,
    bandwidth_monitor_interval_secs: Option<u64>,
    scenario_dirs: Option<Vec<String>>,
    no_default_scenarios: Option<bool>,
    preset_dirs: Option<Vec<String>>,
    no_default_presets: Option<bool>,
}

impl AppConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            backend_name: None,
            exclude_loopback: None,
            log_level: None,
            interface_monitor_interval_secs: None,
            bandwidth_monitor_interval_secs: None,
            scenario_dirs: None,
            no_default_scenarios: None,
            preset_dirs: None,
            no_default_presets: None,
        }
    }

    /// Set backend name
    pub fn backend_name<S: Into<String>>(mut self, name: S) -> Self {
        self.backend_name = Some(name.into());
        self
    }

    /// Set loopback exclusion
    pub fn exclude_loopback(mut self, exclude: bool) -> Self {
        self.exclude_loopback = Some(exclude);
        self
    }

    /// Set log level
    pub fn log_level(mut self, level: LogLevel) -> Self {
        self.log_level = Some(level);
        self
    }

    /// Set interface monitor interval
    pub fn interface_monitor_interval(mut self, secs: u64) -> Self {
        self.interface_monitor_interval_secs = Some(secs);
        self
    }

    /// Set bandwidth monitor interval
    pub fn bandwidth_monitor_interval(mut self, secs: u64) -> Self {
        self.bandwidth_monitor_interval_secs = Some(secs);
        self
    }

    /// Set scenario directories
    pub fn scenario_dirs(mut self, dirs: Vec<String>) -> Self {
        self.scenario_dirs = Some(dirs);
        self
    }

    /// Set no default scenarios flag
    pub fn no_default_scenarios(mut self, no_defaults: bool) -> Self {
        self.no_default_scenarios = Some(no_defaults);
        self
    }

    /// Set preset directories
    pub fn preset_dirs(mut self, dirs: Vec<String>) -> Self {
        self.preset_dirs = Some(dirs);
        self
    }

    /// Set no default presets flag
    pub fn no_default_presets(mut self, no_defaults: bool) -> Self {
        self.no_default_presets = Some(no_defaults);
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<AppConfig> {
        let config = AppConfig {
            backend_name: self
                .backend_name
                .ok_or_else(|| anyhow::anyhow!("Backend name is required"))?,
            exclude_loopback: self.exclude_loopback.unwrap_or(false),
            log_level: self.log_level.unwrap_or(LogLevel::Info),
            interface_monitor_interval_secs: self.interface_monitor_interval_secs.unwrap_or(5),
            bandwidth_monitor_interval_secs: self.bandwidth_monitor_interval_secs.unwrap_or(2),
            scenario_dirs: self.scenario_dirs.unwrap_or_default(),
            no_default_scenarios: self.no_default_scenarios.unwrap_or(false),
            preset_dirs: self.preset_dirs.unwrap_or_default(),
            no_default_presets: self.no_default_presets.unwrap_or(false),
        };

        config.validate()?;
        Ok(config)
    }
}

impl Default for AppConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_parsing() {
        assert_eq!(AppConfig::parse_log_level("info"), LogLevel::Info);
        assert_eq!(AppConfig::parse_log_level("debug"), LogLevel::Debug);
        assert_eq!(AppConfig::parse_log_level("warn"), LogLevel::Warn);
        assert_eq!(AppConfig::parse_log_level("error"), LogLevel::Error);
        assert_eq!(AppConfig::parse_log_level("trace"), LogLevel::Trace);
        assert_eq!(AppConfig::parse_log_level("invalid"), LogLevel::Info); // Default fallback
    }

    #[test]
    fn test_log_level_complex_parsing() {
        // Complex RUST_LOG format should extract main level
        assert_eq!(
            AppConfig::parse_log_level("info,zenoh_transport=warn,netlink_proto=warn"),
            LogLevel::Info
        );
        assert_eq!(
            AppConfig::parse_log_level("debug,some_crate=info"),
            LogLevel::Debug
        );
    }

    #[test]
    fn test_app_config_from_cli() {
        let cli_config = CliConfig {
            verbose: false,
            exclude_loopback: true,
            backend_name: "test-backend".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec!["/custom/scenarios".to_string()],
            no_default_scenarios: true,
            preset_dirs: vec!["/custom/presets".to_string()],
            no_default_presets: true,
        };

        let app_config = AppConfig::from_cli(&cli_config).unwrap();
        assert_eq!(app_config.backend_name, "test-backend");
        assert!(app_config.exclude_loopback);
        assert_eq!(app_config.log_level, LogLevel::Info); // Not verbose
        assert_eq!(app_config.scenario_dirs, vec!["/custom/scenarios"]);
        assert!(app_config.no_default_scenarios);
        assert_eq!(app_config.preset_dirs, vec!["/custom/presets"]);
        assert!(app_config.no_default_presets);
    }

    #[test]
    fn test_app_config_from_cli_verbose() {
        let cli_config = CliConfig {
            verbose: true,
            exclude_loopback: false,
            backend_name: "test-backend".to_string(),
            zenoh_mode: "peer".to_string(),
            zenoh_connect: None,
            zenoh_listen: None,
            no_multicast: false,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        let app_config = AppConfig::from_cli(&cli_config).unwrap();
        assert_eq!(app_config.log_level, LogLevel::Debug); // Verbose enabled
        assert!(app_config.scenario_dirs.is_empty());
        assert!(!app_config.no_default_scenarios);
        assert!(app_config.preset_dirs.is_empty());
        assert!(!app_config.no_default_presets);
    }

    #[test]
    fn test_app_config_validation_success() {
        let config = AppConfig {
            backend_name: "valid-name".to_string(),
            exclude_loopback: false,
            log_level: LogLevel::Info,
            interface_monitor_interval_secs: 5,
            bandwidth_monitor_interval_secs: 2,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_app_config_validation_empty_name() {
        let config = AppConfig {
            backend_name: "".to_string(),
            exclude_loopback: false,
            log_level: LogLevel::Info,
            interface_monitor_interval_secs: 5,
            bandwidth_monitor_interval_secs: 2,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_app_config_validation_zero_intervals() {
        let config = AppConfig {
            backend_name: "test".to_string(),
            exclude_loopback: false,
            log_level: LogLevel::Info,
            interface_monitor_interval_secs: 0,
            bandwidth_monitor_interval_secs: 2,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_err());

        let config = AppConfig {
            backend_name: "test".to_string(),
            exclude_loopback: false,
            log_level: LogLevel::Info,
            interface_monitor_interval_secs: 5,
            bandwidth_monitor_interval_secs: 0,
            scenario_dirs: vec![],
            no_default_scenarios: false,
            preset_dirs: vec![],
            no_default_presets: false,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_app_config_builder() {
        let config = AppConfigBuilder::new()
            .backend_name("test-backend")
            .exclude_loopback(true)
            .log_level(LogLevel::Debug)
            .interface_monitor_interval(10)
            .bandwidth_monitor_interval(3)
            .build()
            .unwrap();

        assert_eq!(config.backend_name, "test-backend");
        assert!(config.exclude_loopback);
        assert_eq!(config.log_level, LogLevel::Debug);
        assert_eq!(config.interface_monitor_interval_secs, 10);
        assert_eq!(config.bandwidth_monitor_interval_secs, 3);
    }

    #[test]
    fn test_app_config_builder_defaults() {
        let config = AppConfigBuilder::new()
            .backend_name("test")
            .build()
            .unwrap();

        assert_eq!(config.backend_name, "test");
        assert!(!config.exclude_loopback);
        assert_eq!(config.log_level, LogLevel::Info);
        assert_eq!(config.interface_monitor_interval_secs, 5);
        assert_eq!(config.bandwidth_monitor_interval_secs, 2);
    }

    #[test]
    fn test_app_config_builder_missing_required() {
        let result = AppConfigBuilder::new().build();
        assert!(result.is_err()); // Missing backend_name
    }
}
