//! Preset types for network configuration presets.
//!
//! All presets (including built-in ones) are loaded from JSON5 files.
//! See docs/preset-format.md for the file format specification.

use serde::{Deserialize, Serialize};

// Re-export CustomPreset from preset_json for convenience
pub use crate::preset_json::CustomPreset;

/// Message sent from backend to frontend containing all available presets.
/// All presets (including built-in ones) are loaded from JSON5 files.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PresetList {
    /// All presets loaded from files (built-in and user-defined)
    pub presets: Vec<CustomPreset>,
}

impl PresetList {
    /// Create a new preset list from loaded presets
    pub fn new(presets: Vec<CustomPreset>) -> Self {
        Self { presets }
    }

    /// Get all presets
    pub fn all(&self) -> &[CustomPreset] {
        &self.presets
    }

    /// Find a preset by its ID
    pub fn find_by_id(&self, id: &str) -> Option<&CustomPreset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Check if a preset with the given ID exists
    pub fn contains(&self, id: &str) -> bool {
        self.presets.iter().any(|p| p.id == id)
    }

    /// Get the number of presets
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    /// Check if the preset list is empty
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TcNetemConfig;

    fn create_test_preset(id: &str, name: &str) -> CustomPreset {
        CustomPreset {
            id: id.to_string(),
            name: name.to_string(),
            description: format!("Test preset {}", name),
            config: TcNetemConfig::default(),
        }
    }

    #[test]
    fn test_preset_list_new() {
        let presets = vec![
            create_test_preset("p1", "Preset 1"),
            create_test_preset("p2", "Preset 2"),
        ];
        let list = PresetList::new(presets);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_preset_list_find_by_id() {
        let presets = vec![
            create_test_preset("satellite", "Satellite Link"),
            create_test_preset("cellular", "Cellular Network"),
        ];
        let list = PresetList::new(presets);

        assert!(list.find_by_id("satellite").is_some());
        assert_eq!(list.find_by_id("satellite").unwrap().name, "Satellite Link");
        assert!(list.find_by_id("nonexistent").is_none());
    }

    #[test]
    fn test_preset_list_contains() {
        let presets = vec![create_test_preset("test", "Test")];
        let list = PresetList::new(presets);

        assert!(list.contains("test"));
        assert!(!list.contains("other"));
    }

    #[test]
    fn test_preset_list_default_is_empty() {
        let list = PresetList::default();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }
}
