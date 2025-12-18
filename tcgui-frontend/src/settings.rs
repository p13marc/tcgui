//! Persistent settings for TC GUI frontend.
//!
//! This module handles loading and saving user preferences to a JSON5 configuration file.
//! Settings are stored in `~/.config/tcgui/frontend.json5` following XDG conventions.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::theme::ThemeMode;
use crate::ui_state::{AppTab, NamespaceFilter, ZOOM_DEFAULT, ZOOM_MAX, ZOOM_MIN};

/// Configuration directory name
const CONFIG_DIR: &str = "tcgui";
/// Settings file name
const SETTINGS_FILE: &str = "frontend.json5";

/// Persistent frontend settings.
///
/// These settings are saved to disk and restored on application startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendSettings {
    /// Theme mode (light or dark)
    #[serde(default)]
    pub theme_mode: ThemeModeJson,

    /// Zoom level (0.5 to 2.0)
    #[serde(default = "default_zoom")]
    pub zoom_level: f32,

    /// Namespace filter settings
    #[serde(default)]
    pub namespace_filter: NamespaceFilterJson,

    /// Last active tab
    #[serde(default)]
    pub current_tab: AppTabJson,
}

fn default_zoom() -> f32 {
    ZOOM_DEFAULT
}

impl Default for FrontendSettings {
    fn default() -> Self {
        Self {
            theme_mode: ThemeModeJson::Light,
            zoom_level: ZOOM_DEFAULT,
            namespace_filter: NamespaceFilterJson::default(),
            current_tab: AppTabJson::Interfaces,
        }
    }
}

/// JSON-serializable theme mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeModeJson {
    #[default]
    Light,
    Dark,
}

impl From<ThemeMode> for ThemeModeJson {
    fn from(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Light => ThemeModeJson::Light,
            ThemeMode::Dark => ThemeModeJson::Dark,
        }
    }
}

impl From<ThemeModeJson> for ThemeMode {
    fn from(mode: ThemeModeJson) -> Self {
        match mode {
            ThemeModeJson::Light => ThemeMode::Light,
            ThemeModeJson::Dark => ThemeMode::Dark,
        }
    }
}

/// JSON-serializable namespace filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceFilterJson {
    /// Show host/default namespace interfaces
    #[serde(default = "default_true")]
    pub show_host: bool,
    /// Show traditional network namespace interfaces
    #[serde(default = "default_true")]
    pub show_namespaces: bool,
    /// Show container namespace interfaces
    #[serde(default = "default_true")]
    pub show_containers: bool,
}

fn default_true() -> bool {
    true
}

impl Default for NamespaceFilterJson {
    fn default() -> Self {
        Self {
            show_host: true,
            show_namespaces: true,
            show_containers: true,
        }
    }
}

impl From<&NamespaceFilter> for NamespaceFilterJson {
    fn from(filter: &NamespaceFilter) -> Self {
        Self {
            show_host: filter.show_host,
            show_namespaces: filter.show_namespaces,
            show_containers: filter.show_containers,
        }
    }
}

impl From<NamespaceFilterJson> for NamespaceFilter {
    fn from(json: NamespaceFilterJson) -> Self {
        Self {
            show_host: json.show_host,
            show_namespaces: json.show_namespaces,
            show_containers: json.show_containers,
        }
    }
}

/// JSON-serializable app tab
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppTabJson {
    #[default]
    Interfaces,
    Scenarios,
}

impl From<AppTab> for AppTabJson {
    fn from(tab: AppTab) -> Self {
        match tab {
            AppTab::Interfaces => AppTabJson::Interfaces,
            AppTab::Scenarios => AppTabJson::Scenarios,
        }
    }
}

impl From<AppTabJson> for AppTab {
    fn from(tab: AppTabJson) -> Self {
        match tab {
            AppTabJson::Interfaces => AppTab::Interfaces,
            AppTabJson::Scenarios => AppTab::Scenarios,
        }
    }
}

impl FrontendSettings {
    /// Gets the path to the settings file.
    ///
    /// Returns `~/.config/tcgui/frontend.json5` on Linux.
    pub fn settings_path() -> Option<PathBuf> {
        dirs::config_dir().map(|config| config.join(CONFIG_DIR).join(SETTINGS_FILE))
    }

    /// Loads settings from the configuration file.
    ///
    /// Returns default settings if the file doesn't exist or can't be parsed.
    pub fn load() -> Self {
        let Some(path) = Self::settings_path() else {
            warn!("Could not determine config directory, using defaults");
            return Self::default();
        };

        if !path.exists() {
            debug!("Settings file does not exist, using defaults");
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => match json5::from_str(&content) {
                Ok(mut settings) => {
                    info!("Loaded settings from {}", path.display());
                    // Validate and clamp values
                    Self::validate(&mut settings);
                    settings
                }
                Err(e) => {
                    error!("Failed to parse settings file: {}", e);
                    Self::default()
                }
            },
            Err(e) => {
                error!("Failed to read settings file: {}", e);
                Self::default()
            }
        }
    }

    /// Validates and clamps settings to valid ranges.
    fn validate(settings: &mut FrontendSettings) {
        // Clamp zoom level to valid range
        if settings.zoom_level < ZOOM_MIN || settings.zoom_level > ZOOM_MAX {
            warn!(
                "Zoom level {} out of range [{}, {}], clamping",
                settings.zoom_level, ZOOM_MIN, ZOOM_MAX
            );
            settings.zoom_level = settings.zoom_level.clamp(ZOOM_MIN, ZOOM_MAX);
        }
    }

    /// Saves settings to the configuration file.
    ///
    /// Creates the configuration directory if it doesn't exist.
    pub fn save(&self) -> Result<(), SettingsError> {
        let path = Self::settings_path().ok_or(SettingsError::NoConfigDir)?;

        // Create config directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| SettingsError::CreateDir(e.to_string()))?;
        }

        // Serialize to pretty JSON5 format
        let content = self.to_json5_string();

        fs::write(&path, content).map_err(|e| SettingsError::Write(e.to_string()))?;

        debug!("Saved settings to {}", path.display());
        Ok(())
    }

    /// Converts settings to a pretty-printed JSON5 string.
    fn to_json5_string(&self) -> String {
        // JSON5 doesn't have a pretty-print option, so we use serde_json for formatting
        // and the output is valid JSON5 (JSON is a subset of JSON5)
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Error type for settings operations
#[derive(Debug)]
pub enum SettingsError {
    /// Could not determine config directory
    NoConfigDir,
    /// Failed to create config directory
    CreateDir(String),
    /// Failed to write settings file
    Write(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::NoConfigDir => write!(f, "Could not determine config directory"),
            SettingsError::CreateDir(e) => write!(f, "Failed to create config directory: {}", e),
            SettingsError::Write(e) => write!(f, "Failed to write settings file: {}", e),
        }
    }
}

impl std::error::Error for SettingsError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_settings() {
        let settings = FrontendSettings::default();
        assert!(matches!(settings.theme_mode, ThemeModeJson::Light));
        assert_eq!(settings.zoom_level, ZOOM_DEFAULT);
        assert!(settings.namespace_filter.show_host);
        assert!(settings.namespace_filter.show_namespaces);
        assert!(settings.namespace_filter.show_containers);
        assert!(matches!(settings.current_tab, AppTabJson::Interfaces));
    }

    #[test]
    fn test_serialize_deserialize() {
        let settings = FrontendSettings {
            theme_mode: ThemeModeJson::Dark,
            zoom_level: 1.5,
            namespace_filter: NamespaceFilterJson {
                show_host: true,
                show_namespaces: false,
                show_containers: true,
            },
            current_tab: AppTabJson::Scenarios,
        };

        let json = settings.to_json5_string();
        let parsed: FrontendSettings = json5::from_str(&json).unwrap();

        assert!(matches!(parsed.theme_mode, ThemeModeJson::Dark));
        assert_eq!(parsed.zoom_level, 1.5);
        assert!(parsed.namespace_filter.show_host);
        assert!(!parsed.namespace_filter.show_namespaces);
        assert!(parsed.namespace_filter.show_containers);
        assert!(matches!(parsed.current_tab, AppTabJson::Scenarios));
    }

    #[test]
    fn test_parse_minimal_json5() {
        let json5 = "{}";
        let settings: FrontendSettings = json5::from_str(json5).unwrap();

        // All defaults should apply
        assert!(matches!(settings.theme_mode, ThemeModeJson::Light));
        assert_eq!(settings.zoom_level, ZOOM_DEFAULT);
    }

    #[test]
    fn test_parse_partial_json5() {
        let json5 = r#"{ theme_mode: "dark" }"#;
        let settings: FrontendSettings = json5::from_str(json5).unwrap();

        assert!(matches!(settings.theme_mode, ThemeModeJson::Dark));
        assert_eq!(settings.zoom_level, ZOOM_DEFAULT); // Default
    }

    #[test]
    fn test_validate_clamps_zoom() {
        let mut settings = FrontendSettings {
            zoom_level: 5.0, // Out of range
            ..Default::default()
        };

        FrontendSettings::validate(&mut settings);
        assert_eq!(settings.zoom_level, ZOOM_MAX);

        settings.zoom_level = 0.1; // Too low
        FrontendSettings::validate(&mut settings);
        assert_eq!(settings.zoom_level, ZOOM_MIN);
    }

    #[test]
    fn test_theme_mode_conversion() {
        assert!(matches!(
            ThemeMode::from(ThemeModeJson::Light),
            ThemeMode::Light
        ));
        assert!(matches!(
            ThemeMode::from(ThemeModeJson::Dark),
            ThemeMode::Dark
        ));
        assert!(matches!(
            ThemeModeJson::from(ThemeMode::Light),
            ThemeModeJson::Light
        ));
        assert!(matches!(
            ThemeModeJson::from(ThemeMode::Dark),
            ThemeModeJson::Dark
        ));
    }

    #[test]
    fn test_app_tab_conversion() {
        assert!(matches!(
            AppTab::from(AppTabJson::Interfaces),
            AppTab::Interfaces
        ));
        assert!(matches!(
            AppTab::from(AppTabJson::Scenarios),
            AppTab::Scenarios
        ));
        assert!(matches!(
            AppTabJson::from(AppTab::Interfaces),
            AppTabJson::Interfaces
        ));
        assert!(matches!(
            AppTabJson::from(AppTab::Scenarios),
            AppTabJson::Scenarios
        ));
    }

    #[test]
    fn test_settings_path() {
        let path = FrontendSettings::settings_path();
        // Should return Some on most systems
        if let Some(p) = path {
            assert!(p.ends_with("tcgui/frontend.json5"));
        }
    }
}
