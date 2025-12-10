//! Scenario Loader - File-based scenario loading from directories.
//!
//! This module provides functionality to scan directories for `.json5` scenario files,
//! parse them, and load them into the system. It supports multiple source directories
//! with priority ordering (user scenarios can override system ones).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use tcgui_shared::scenario::NetworkScenario;
use tcgui_shared::scenario_json::parse_scenario_file;

/// Default system scenario directory (installed via package)
pub const SYSTEM_SCENARIO_DIR: &str = "/usr/share/tcgui/scenarios";

/// Default user scenario directory
pub const USER_SCENARIO_DIR: &str = ".config/tcgui/scenarios";

/// Scenario loader that scans directories for .json5 scenario files.
///
/// Directories are scanned in priority order - later directories can override
/// scenarios with the same ID from earlier directories.
#[derive(Debug, Clone)]
pub struct ScenarioLoader {
    /// Directories to scan, in priority order (later overrides earlier)
    directories: Vec<PathBuf>,
}

impl Default for ScenarioLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ScenarioLoader {
    /// Create a new scenario loader with default directories.
    ///
    /// Default directories (in priority order):
    /// 1. System: `/usr/share/tcgui/scenarios`
    /// 2. User: `~/.config/tcgui/scenarios`
    /// 3. Local: `./scenarios`
    pub fn new() -> Self {
        let mut directories = Vec::new();

        // System directory (lowest priority)
        directories.push(PathBuf::from(SYSTEM_SCENARIO_DIR));

        // User directory
        if let Some(home) = dirs::home_dir() {
            directories.push(home.join(USER_SCENARIO_DIR));
        }

        // Local directory (highest priority)
        directories.push(PathBuf::from("./scenarios"));

        Self { directories }
    }

    /// Create a scenario loader with custom directories only.
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

    /// Load all scenarios from configured directories.
    ///
    /// Scenarios are loaded in directory order, with later directories
    /// overriding scenarios with the same ID from earlier directories.
    pub fn load_all(&self) -> Vec<NetworkScenario> {
        let mut scenarios: HashMap<String, NetworkScenario> = HashMap::new();

        for dir in &self.directories {
            if !dir.exists() {
                debug!("Scenario directory does not exist, skipping: {:?}", dir);
                continue;
            }

            match self.load_from_directory(dir) {
                Ok(loaded) => {
                    let count = loaded.len();
                    for scenario in loaded {
                        let id = scenario.id.clone();
                        if scenarios.contains_key(&id) {
                            debug!(
                                "Scenario '{}' from {:?} overrides previous definition",
                                id, dir
                            );
                        }
                        scenarios.insert(id, scenario);
                    }
                    if count > 0 {
                        info!("Loaded {} scenarios from {:?}", count, dir);
                    }
                }
                Err(e) => {
                    warn!("Failed to load scenarios from {:?}: {}", dir, e);
                }
            }
        }

        scenarios.into_values().collect()
    }

    /// Load scenarios from a single directory.
    fn load_from_directory(&self, dir: &Path) -> Result<Vec<NetworkScenario>> {
        let mut scenarios = Vec::new();

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

            match self.load_scenario_file(&path) {
                Ok(scenario) => {
                    scenarios.push(scenario);
                }
                Err(e) => {
                    warn!("Failed to load scenario from {:?}: {}", path, e);
                    // Continue loading other files
                }
            }
        }

        Ok(scenarios)
    }

    /// Load a single scenario file.
    fn load_scenario_file(&self, path: &Path) -> Result<NetworkScenario> {
        debug!("Loading scenario from {:?}", path);

        parse_scenario_file(path)
            .with_context(|| format!("Failed to parse scenario file: {:?}", path))
    }

    /// Get a specific scenario by ID from loaded scenarios.
    ///
    /// This is a convenience method that loads all scenarios and finds one by ID.
    /// For repeated lookups, consider caching the result of `load_all()`.
    pub fn get_scenario(&self, id: &str) -> Option<NetworkScenario> {
        self.load_all().into_iter().find(|s| s.id == id)
    }

    /// Check if any scenario directories exist and contain files.
    pub fn has_scenarios(&self) -> bool {
        for dir in &self.directories {
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if entry.path().extension().and_then(|e| e.to_str()) == Some("json5") {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_scenario(id: &str, name: &str) -> String {
        format!(
            r#"{{
    id: "{}",
    name: "{}",
    description: "Test scenario",
    steps: [
        {{
            duration: "10s",
            description: "Start",
            tc_config: {{
                loss: {{ percentage: 5 }}
            }}
        }}
    ]
}}"#,
            id, name
        )
    }

    #[test]
    fn test_loader_creation() {
        let loader = ScenarioLoader::new();
        assert!(!loader.directories().is_empty());
    }

    #[test]
    fn test_loader_with_custom_directories() {
        let dirs = vec![PathBuf::from("/tmp/test1"), PathBuf::from("/tmp/test2")];
        let loader = ScenarioLoader::with_directories(dirs.clone());
        assert_eq!(loader.directories(), &dirs);
    }

    #[test]
    fn test_load_from_directory() {
        let temp_dir = TempDir::new().unwrap();
        let scenario_content = create_test_scenario("test-1", "Test Scenario 1");

        let file_path = temp_dir.path().join("test-scenario.json5");
        fs::write(&file_path, scenario_content).unwrap();

        let loader = ScenarioLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let scenarios = loader.load_all();

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].id, "test-1");
        assert_eq!(scenarios[0].name, "Test Scenario 1");
    }

    #[test]
    fn test_load_multiple_scenarios() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("scenario1.json5"),
            create_test_scenario("scenario-1", "Scenario 1"),
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("scenario2.json5"),
            create_test_scenario("scenario-2", "Scenario 2"),
        )
        .unwrap();

        let loader = ScenarioLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let scenarios = loader.load_all();

        assert_eq!(scenarios.len(), 2);

        let ids: Vec<_> = scenarios.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"scenario-1"));
        assert!(ids.contains(&"scenario-2"));
    }

    #[test]
    fn test_directory_priority_override() {
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        // Same ID in both directories, different names
        fs::write(
            dir1.path().join("scenario.json5"),
            create_test_scenario("same-id", "From Dir 1"),
        )
        .unwrap();

        fs::write(
            dir2.path().join("scenario.json5"),
            create_test_scenario("same-id", "From Dir 2"),
        )
        .unwrap();

        // dir2 has higher priority (comes later)
        let loader = ScenarioLoader::with_directories(vec![
            dir1.path().to_path_buf(),
            dir2.path().to_path_buf(),
        ]);
        let scenarios = loader.load_all();

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].id, "same-id");
        assert_eq!(scenarios[0].name, "From Dir 2");
    }

    #[test]
    fn test_skip_non_json5_files() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("valid.json5"),
            create_test_scenario("valid", "Valid"),
        )
        .unwrap();

        fs::write(temp_dir.path().join("readme.txt"), "This is not a scenario").unwrap();

        fs::write(
            temp_dir.path().join("config.json"),
            r#"{"not": "a scenario"}"#,
        )
        .unwrap();

        let loader = ScenarioLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let scenarios = loader.load_all();

        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].id, "valid");
    }

    #[test]
    fn test_skip_invalid_scenarios() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("valid.json5"),
            create_test_scenario("valid", "Valid"),
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("invalid.json5"),
            "{ this is not valid json5 at all }}}",
        )
        .unwrap();

        let loader = ScenarioLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        let scenarios = loader.load_all();

        // Should still load the valid one
        assert_eq!(scenarios.len(), 1);
        assert_eq!(scenarios[0].id, "valid");
    }

    #[test]
    fn test_nonexistent_directory() {
        let loader = ScenarioLoader::with_directories(vec![PathBuf::from(
            "/nonexistent/directory/that/should/not/exist",
        )]);
        let scenarios = loader.load_all();

        assert!(scenarios.is_empty());
    }

    #[test]
    fn test_get_scenario_by_id() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("scenario1.json5"),
            create_test_scenario("find-me", "Find Me"),
        )
        .unwrap();

        fs::write(
            temp_dir.path().join("scenario2.json5"),
            create_test_scenario("other", "Other"),
        )
        .unwrap();

        let loader = ScenarioLoader::with_directories(vec![temp_dir.path().to_path_buf()]);

        let found = loader.get_scenario("find-me");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Find Me");

        let not_found = loader.get_scenario("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_has_scenarios() {
        let temp_dir = TempDir::new().unwrap();

        let loader = ScenarioLoader::with_directories(vec![temp_dir.path().to_path_buf()]);
        assert!(!loader.has_scenarios());

        fs::write(
            temp_dir.path().join("scenario.json5"),
            create_test_scenario("test", "Test"),
        )
        .unwrap();

        assert!(loader.has_scenarios());
    }

    #[test]
    fn test_add_directory() {
        let mut loader = ScenarioLoader::with_directories(vec![PathBuf::from("/dir1")]);
        loader.add_directory(PathBuf::from("/dir2"));

        assert_eq!(loader.directories().len(), 2);
        assert_eq!(loader.directories()[1], PathBuf::from("/dir2"));
    }

    #[test]
    fn test_add_directories() {
        let mut loader = ScenarioLoader::with_directories(vec![PathBuf::from("/dir1")]);
        loader.add_directories(vec![PathBuf::from("/dir2"), PathBuf::from("/dir3")]);

        assert_eq!(loader.directories().len(), 3);
    }
}
