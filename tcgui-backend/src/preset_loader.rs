//! Preset Loader - File-based preset loading from directories.
//!
//! This module provides functionality to scan directories for `.json5` preset files,
//! parse them, and load them into the system. It supports multiple source directories
//! with priority ordering (user presets can override system ones).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use tcgui_shared::TcNetemConfig;
use tcgui_shared::preset_json::{CustomPreset, parse_preset_file};
use tcgui_shared::presets::PresetList;
use tcgui_shared::scenario_json::PresetResolver;

/// Default system preset directory (installed via package)
pub const SYSTEM_PRESET_DIR: &str = "/usr/share/tcgui/presets";

/// Default user preset directory
pub const USER_PRESET_DIR: &str = ".config/tcgui/presets";

/// Error information for a preset that failed to load
#[derive(Debug, Clone)]
pub struct PresetLoadError {
    /// Path to the file that failed to load
    pub file_path: String,
    /// Error message
    pub error: String,
}

impl std::fmt::Display for PresetLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Failed to load preset from {}: {}",
            self.file_path, self.error
        )
    }
}

/// Preset loader that scans directories for .json5 preset files.
///
/// Directories are scanned in priority order - later directories can override
/// presets with the same ID from earlier directories.
#[derive(Debug, Clone)]
pub struct PresetLoader {
    /// Directories to scan, in priority order (later overrides earlier)
    directories: Vec<PathBuf>,
}

impl Default for PresetLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetLoader {
    /// Create a new preset loader with default directories.
    ///
    /// Default directories (in priority order):
    /// 1. System: `/usr/share/tcgui/presets`
    /// 2. User: `~/.config/tcgui/presets`
    /// 3. Local: `./presets`
    pub fn new() -> Self {
        Self::with_defaults(true)
    }

    /// Create a new preset loader, optionally including default directories.
    ///
    /// If `include_defaults` is true, includes:
    /// 1. System: `/usr/share/tcgui/presets`
    /// 2. User: `~/.config/tcgui/presets`
    /// 3. Local: `./presets`
    ///
    /// If `include_defaults` is false, starts with an empty directory list.
    pub fn with_defaults(include_defaults: bool) -> Self {
        let mut directories = Vec::new();

        if include_defaults {
            // System directory (lowest priority)
            directories.push(PathBuf::from(SYSTEM_PRESET_DIR));

            // User directory
            if let Some(home) = dirs::home_dir() {
                directories.push(home.join(USER_PRESET_DIR));
            }

            // Local directory (highest priority)
            directories.push(PathBuf::from("./presets"));
        }

        Self { directories }
    }

    /// Create a preset loader with custom directories only.
    pub fn with_directories(directories: Vec<PathBuf>) -> Self {
        Self { directories }
    }

    /// Add additional directories to scan (appended with highest priority).
    pub fn add_directories(&mut self, dirs: impl IntoIterator<Item = PathBuf>) {
        self.directories.extend(dirs);
    }

    /// Add a single directory to scan (appended with highest priority).
    pub fn add_directory(&mut self, dir: PathBuf) {
        self.directories.push(dir);
    }

    /// Get the list of directories being scanned.
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }

    /// Load all presets from configured directories.
    ///
    /// Presets are loaded in directory order, with later directories
    /// overriding presets with the same ID from earlier directories.
    pub fn load_all(&self) -> Vec<CustomPreset> {
        self.load_all_with_errors().0
    }

    /// Load all presets from configured directories, also returning load errors.
    ///
    /// Returns a tuple of (successful presets, load errors).
    /// Presets are loaded in directory order, with later directories
    /// overriding presets with the same ID from earlier directories.
    pub fn load_all_with_errors(&self) -> (Vec<CustomPreset>, Vec<PresetLoadError>) {
        let mut presets: HashMap<String, CustomPreset> = HashMap::new();
        let mut errors: Vec<PresetLoadError> = Vec::new();

        for dir in &self.directories {
            if !dir.exists() {
                debug!("Preset directory does not exist, skipping: {:?}", dir);
                continue;
            }

            match self.load_from_directory_with_errors(dir) {
                Ok((loaded, dir_errors)) => {
                    let count = loaded.len();
                    for preset in loaded {
                        let id = preset.id.clone();
                        if presets.contains_key(&id) {
                            debug!(
                                "Preset '{}' from {:?} overrides previous definition",
                                id, dir
                            );
                        }
                        presets.insert(id, preset);
                    }
                    errors.extend(dir_errors);
                    if count > 0 {
                        info!("Loaded {} presets from {:?}", count, dir);
                    }
                }
                Err(e) => {
                    warn!("Failed to load presets from {:?}: {}", dir, e);
                }
            }
        }

        (presets.into_values().collect(), errors)
    }

    /// Load presets from a single directory, returning both presets and errors.
    fn load_from_directory_with_errors(
        &self,
        dir: &Path,
    ) -> Result<(Vec<CustomPreset>, Vec<PresetLoadError>)> {
        let mut presets = Vec::new();
        let mut errors = Vec::new();

        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("Failed to read directory: {:?}", dir))?;

        for entry in entries {
            let entry =
                entry.with_context(|| format!("Failed to read directory entry in {:?}", dir))?;
            let path = entry.path();

            // Only process .json5 files
            if path.extension().and_then(|e| e.to_str()) != Some("json5") {
                continue;
            }

            match self.load_preset_file(&path) {
                Ok(preset) => {
                    presets.push(preset);
                }
                Err(e) => {
                    warn!("Failed to load preset from {:?}: {}", path, e);
                    errors.push(PresetLoadError {
                        file_path: path.display().to_string(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok((presets, errors))
    }

    /// Load a single preset file.
    fn load_preset_file(&self, path: &Path) -> Result<CustomPreset> {
        debug!("Loading preset from {:?}", path);

        parse_preset_file(path).with_context(|| format!("Failed to parse preset file: {:?}", path))
    }

    /// Get a specific preset by ID from loaded presets.
    ///
    /// This is a convenience method that loads all presets and finds one by ID.
    /// For repeated lookups, consider caching the result of `load_all()`.
    pub fn get_preset(&self, id: &str) -> Option<CustomPreset> {
        self.load_all().into_iter().find(|p| p.id == id)
    }

    /// Check if any preset directories exist and contain files.
    pub fn has_presets(&self) -> bool {
        for dir in &self.directories {
            if dir.exists()
                && let Ok(entries) = std::fs::read_dir(dir)
            {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("json5") {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Preset resolver that combines built-in presets with custom presets from a PresetLoader.
///
/// This resolver can look up presets by their ID:
/// - Built-in presets: Use the enum variant name in lowercase (e.g., "satellite-link", "cellular-network")
/// - Custom presets: Use the preset's ID as defined in the JSON5 file
pub struct CombinedPresetResolver<'a> {
    preset_list: &'a PresetList,
}

impl<'a> CombinedPresetResolver<'a> {
    /// Create a new resolver from a PresetList
    pub fn new(preset_list: &'a PresetList) -> Self {
        Self { preset_list }
    }
}

impl PresetResolver for CombinedPresetResolver<'_> {
    fn resolve(&self, preset_id: &str) -> Option<TcNetemConfig> {
        // Look up preset by ID - all presets (including built-in ones) are loaded from files
        self.preset_list
            .find_by_id(preset_id)
            .map(|preset| preset.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_preset(id: &str, name: &str) -> String {
        format!(
            r#"{{
    id: "{}",
    name: "{}",
    description: "Test preset",
    loss: {{ percentage: 5 }},
    delay: {{ base_ms: 100 }}
}}"#,
            id, name
        )
    }

    #[test]
    fn test_loader_creation() {
        let loader = PresetLoader::new();
        assert!(!loader.directories().is_empty());
    }

    #[test]
    fn test_loader_with_custom_directories() {
        let dirs = vec![PathBuf::from("/tmp/test1"), PathBuf::from("/tmp/test2")];
        let loader = PresetLoader::with_directories(dirs.clone());
        assert_eq!(loader.directories(), &dirs);
    }

    #[test]
    fn test_load_from_directory() {
        let temp_dir = TempDir::new().unwrap();
        let preset_content = create_test_preset("test-1", "Test Preset 1");

        let file_path = temp_dir.path().join("test-preset.json5");
        fs::write(&file_path, preset_content).unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let presets = loader.load_all();

        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, "test-1");
        assert_eq!(presets[0].name, "Test Preset 1");
    }

    #[test]
    fn test_load_multiple_presets() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("preset1.json5"),
            create_test_preset("preset-1", "Preset 1"),
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("preset2.json5"),
            create_test_preset("preset-2", "Preset 2"),
        )
        .unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let presets = loader.load_all();

        assert_eq!(presets.len(), 2);

        let ids: Vec<_> = presets.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"preset-1"));
        assert!(ids.contains(&"preset-2"));
    }

    #[test]
    fn test_directory_priority_override() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        // Same ID in both directories, different names
        fs::write(
            dir1.path().join("preset.json5"),
            create_test_preset("same-id", "From Dir 1"),
        )
        .unwrap();

        fs::write(
            dir2.path().join("preset.json5"),
            create_test_preset("same-id", "From Dir 2"),
        )
        .unwrap();

        // dir2 has higher priority (comes later)
        let loader = PresetLoader::with_directories(vec![
            dir1.path().to_path_buf(),
            dir2.path().to_path_buf(),
        ]);
        let presets = loader.load_all();

        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, "same-id");
        assert_eq!(presets[0].name, "From Dir 2");
    }

    #[test]
    fn test_skip_non_json5_files() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("valid.json5"),
            create_test_preset("valid", "Valid"),
        )
        .unwrap();

        fs::write(temp_dir.path().join("readme.txt"), "This is not a preset").unwrap();

        fs::write(
            temp_dir.path().join("config.json"),
            r#"{"not": "a preset"}"#,
        )
        .unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let presets = loader.load_all();

        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, "valid");
    }

    #[test]
    fn test_skip_invalid_presets() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("valid.json5"),
            create_test_preset("valid", "Valid"),
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("invalid.json5"),
            "{ this is not valid json5 at all }}}",
        )
        .unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let presets = loader.load_all();

        // Should still load the valid one
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].id, "valid");
    }

    #[test]
    fn test_nonexistent_directory() {
        let loader = PresetLoader::with_directories(vec![PathBuf::from(
            "/nonexistent/directory/that/should/not/exist",
        )]);
        let presets = loader.load_all();

        assert!(presets.is_empty());
    }

    #[test]
    fn test_get_preset_by_id() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("preset1.json5"),
            create_test_preset("find-me", "Find Me"),
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("preset2.json5"),
            create_test_preset("other", "Other"),
        )
        .unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);

        let found = loader.get_preset("find-me");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Find Me");

        let not_found = loader.get_preset("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_has_presets() {
        let temp_dir = TempDir::new().unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        assert!(!loader.has_presets());

        fs::write(
            temp_dir.path().join("preset.json5"),
            create_test_preset("test", "Test"),
        )
        .unwrap();

        assert!(loader.has_presets());
    }

    #[test]
    fn test_add_directory() {
        let mut loader = PresetLoader::with_directories(vec![PathBuf::from("/dir1")]);
        loader.add_directory(PathBuf::from("/dir2"));

        assert_eq!(loader.directories().len(), 2);
        assert_eq!(loader.directories()[1], PathBuf::from("/dir2"));
    }

    #[test]
    fn test_preset_config_values() {
        let temp_dir = TempDir::new().unwrap();

        let json5 = r#"{
            id: "detailed",
            name: "Detailed Preset",
            description: "A detailed preset",
            loss: { percentage: 5, correlation: 10 },
            delay: { base_ms: 100, jitter_ms: 20 },
            rate_limit: { rate_kbps: 1000 }
        }"#;

        fs::write(temp_dir.path().join("detailed.json5"), json5).unwrap();

        let loader = PresetLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let presets = loader.load_all();

        assert_eq!(presets.len(), 1);
        let preset = &presets[0];

        assert!(preset.config.loss.enabled);
        assert_eq!(preset.config.loss.percentage, 5.0);
        assert_eq!(preset.config.loss.correlation, 10.0);

        assert!(preset.config.delay.enabled);
        assert_eq!(preset.config.delay.base_ms, 100.0);
        assert_eq!(preset.config.delay.jitter_ms, 20.0);

        assert!(preset.config.rate_limit.enabled);
        assert_eq!(preset.config.rate_limit.rate_kbps, 1000);

        assert!(!preset.config.duplicate.enabled);
        assert!(!preset.config.reorder.enabled);
        assert!(!preset.config.corrupt.enabled);
    }
}
