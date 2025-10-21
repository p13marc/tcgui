//! Feature toggle system for runtime behavior control.
//!
//! This module provides a comprehensive feature flag system that allows
//! enabling/disabling features at runtime without recompilation. Features
//! can be controlled via configuration files, environment variables, or
//! runtime API calls.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

/// Feature flag configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureFlags {
    /// Enable/disable bandwidth monitoring
    pub bandwidth_monitoring: bool,
    /// Enable/disable interface hot-plugging detection
    pub interface_hotplug: bool,
    /// Enable/disable TC command caching
    pub tc_command_caching: bool,
    /// Enable/disable Zenoh advanced features
    pub zenoh_advanced_features: bool,
    /// Enable/disable metrics collection
    pub metrics_collection: bool,
    /// Enable/disable namespace monitoring
    pub namespace_monitoring: bool,
    /// Enable/disable TC parameter validation
    pub tc_parameter_validation: bool,
    /// Enable/disable experimental features
    pub experimental_features: bool,
    /// Enable/disable A/B testing framework
    pub ab_testing: bool,
    /// Custom feature flags for extensions
    pub custom: HashMap<String, bool>,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            bandwidth_monitoring: true,
            interface_hotplug: true,
            tc_command_caching: false,
            zenoh_advanced_features: true,
            metrics_collection: false,
            namespace_monitoring: true,
            tc_parameter_validation: true,
            experimental_features: false,
            ab_testing: false,
            custom: HashMap::new(),
        }
    }
}

/// Environment-specific feature flag profiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureProfile {
    /// Development profile - most features enabled for testing
    Development,
    /// Staging profile - production-like with some debugging features
    Staging,
    /// Production profile - minimal features for performance and stability
    Production,
    /// Testing profile - features optimized for automated testing
    Testing,
    /// Custom profile with explicit configuration
    Custom(FeatureFlags),
}

impl FeatureProfile {
    /// Get feature flags for this profile
    pub fn to_feature_flags(&self) -> FeatureFlags {
        match self {
            FeatureProfile::Development => FeatureFlags {
                bandwidth_monitoring: true,
                interface_hotplug: true,
                tc_command_caching: true,
                zenoh_advanced_features: true,
                metrics_collection: true,
                namespace_monitoring: true,
                tc_parameter_validation: true,
                experimental_features: true,
                ab_testing: true,
                custom: HashMap::new(),
            },
            FeatureProfile::Staging => FeatureFlags {
                bandwidth_monitoring: true,
                interface_hotplug: true,
                tc_command_caching: true,
                zenoh_advanced_features: true,
                metrics_collection: true,
                namespace_monitoring: true,
                tc_parameter_validation: true,
                experimental_features: false,
                ab_testing: false,
                custom: HashMap::new(),
            },
            FeatureProfile::Production => FeatureFlags {
                bandwidth_monitoring: true,
                interface_hotplug: true,
                tc_command_caching: false,
                zenoh_advanced_features: true,
                metrics_collection: false,
                namespace_monitoring: true,
                tc_parameter_validation: true,
                experimental_features: false,
                ab_testing: false,
                custom: HashMap::new(),
            },
            FeatureProfile::Testing => FeatureFlags {
                bandwidth_monitoring: false,
                interface_hotplug: false,
                tc_command_caching: false,
                zenoh_advanced_features: false,
                metrics_collection: true,
                namespace_monitoring: true,
                tc_parameter_validation: true,
                experimental_features: false,
                ab_testing: false,
                custom: HashMap::new(),
            },
            FeatureProfile::Custom(flags) => flags.clone(),
        }
    }
}

/// Thread-safe feature toggle manager
#[derive(Debug, Clone)]
pub struct FeatureToggleManager {
    /// Current feature flags
    flags: Arc<RwLock<FeatureFlags>>,
    /// Environment profile
    profile: FeatureProfile,
}

impl FeatureToggleManager {
    /// Create a new feature toggle manager with default flags
    pub fn new() -> Self {
        Self {
            flags: Arc::new(RwLock::new(FeatureFlags::default())),
            profile: FeatureProfile::Production,
        }
    }

    /// Create a new manager with a specific profile
    pub fn with_profile(profile: FeatureProfile) -> Self {
        let flags = profile.to_feature_flags();
        Self {
            flags: Arc::new(RwLock::new(flags)),
            profile,
        }
    }

    /// Create manager from environment variable
    pub fn from_env() -> Result<Self> {
        let profile = match std::env::var("TCGUI_FEATURE_PROFILE") {
            Ok(profile_str) => match profile_str.to_lowercase().as_str() {
                "development" | "dev" => FeatureProfile::Development,
                "staging" | "stage" => FeatureProfile::Staging,
                "production" | "prod" => FeatureProfile::Production,
                "testing" | "test" => FeatureProfile::Testing,
                _ => {
                    warn!(
                        "Unknown feature profile '{}', using Production",
                        profile_str
                    );
                    FeatureProfile::Production
                }
            },
            Err(_) => FeatureProfile::Production, // Default
        };

        info!("Initializing feature flags with profile: {:?}", profile);
        Ok(Self::with_profile(profile))
    }

    /// Check if a feature is enabled
    pub fn is_enabled(&self, feature: &Feature) -> bool {
        let flags = self.flags.read().unwrap();
        match feature {
            Feature::BandwidthMonitoring => flags.bandwidth_monitoring,
            Feature::InterfaceHotplug => flags.interface_hotplug,
            Feature::TcCommandCaching => flags.tc_command_caching,
            Feature::ZenohAdvancedFeatures => flags.zenoh_advanced_features,
            Feature::MetricsCollection => flags.metrics_collection,
            Feature::NamespaceMonitoring => flags.namespace_monitoring,
            Feature::TcParameterValidation => flags.tc_parameter_validation,
            Feature::ExperimentalFeatures => flags.experimental_features,
            Feature::AbTesting => flags.ab_testing,
            Feature::Custom(name) => flags.custom.get(name).copied().unwrap_or(false),
        }
    }

    /// Enable a feature at runtime
    pub fn enable_feature(&self, feature: &Feature) -> Result<()> {
        let mut flags = self.flags.write().unwrap();
        self.set_feature_flag(&mut flags, feature, true);
        debug!("Enabled feature: {:?}", feature);
        Ok(())
    }

    /// Disable a feature at runtime
    pub fn disable_feature(&self, feature: &Feature) -> Result<()> {
        let mut flags = self.flags.write().unwrap();
        self.set_feature_flag(&mut flags, feature, false);
        debug!("Disabled feature: {:?}", feature);
        Ok(())
    }

    /// Toggle a feature state
    pub fn toggle_feature(&self, feature: &Feature) -> Result<bool> {
        let mut flags = self.flags.write().unwrap();
        let current_state = self.get_feature_flag(&flags, feature);
        let new_state = !current_state;
        self.set_feature_flag(&mut flags, feature, new_state);
        debug!("Toggled feature: {:?} -> {}", feature, new_state);
        Ok(new_state)
    }

    /// Get all current feature flags
    pub fn get_all_flags(&self) -> FeatureFlags {
        self.flags.read().unwrap().clone()
    }

    /// Update multiple features at once
    pub fn update_features(&self, updates: HashMap<Feature, bool>) -> Result<()> {
        let mut flags = self.flags.write().unwrap();
        for (feature, enabled) in updates {
            self.set_feature_flag(&mut flags, &feature, enabled);
        }
        debug!("Updated multiple features");
        Ok(())
    }

    /// Reset flags to profile defaults
    pub fn reset_to_profile(&self) -> Result<()> {
        let mut flags = self.flags.write().unwrap();
        *flags = self.profile.to_feature_flags();
        info!(
            "Reset feature flags to profile defaults: {:?}",
            self.profile
        );
        Ok(())
    }

    /// Get current environment profile
    pub fn get_profile(&self) -> &FeatureProfile {
        &self.profile
    }

    /// Set a custom feature flag
    pub fn set_custom_feature(&self, name: String, enabled: bool) -> Result<()> {
        let mut flags = self.flags.write().unwrap();
        flags.custom.insert(name.clone(), enabled);
        debug!("Set custom feature '{}': {}", name, enabled);
        Ok(())
    }

    /// Remove a custom feature flag
    pub fn remove_custom_feature(&self, name: &str) -> Result<bool> {
        let mut flags = self.flags.write().unwrap();
        let was_present = flags.custom.remove(name).is_some();
        debug!("Removed custom feature '{}': {}", name, was_present);
        Ok(was_present)
    }

    // Helper methods for internal flag manipulation
    fn get_feature_flag(&self, flags: &FeatureFlags, feature: &Feature) -> bool {
        match feature {
            Feature::BandwidthMonitoring => flags.bandwidth_monitoring,
            Feature::InterfaceHotplug => flags.interface_hotplug,
            Feature::TcCommandCaching => flags.tc_command_caching,
            Feature::ZenohAdvancedFeatures => flags.zenoh_advanced_features,
            Feature::MetricsCollection => flags.metrics_collection,
            Feature::NamespaceMonitoring => flags.namespace_monitoring,
            Feature::TcParameterValidation => flags.tc_parameter_validation,
            Feature::ExperimentalFeatures => flags.experimental_features,
            Feature::AbTesting => flags.ab_testing,
            Feature::Custom(name) => flags.custom.get(name).copied().unwrap_or(false),
        }
    }

    fn set_feature_flag(&self, flags: &mut FeatureFlags, feature: &Feature, enabled: bool) {
        match feature {
            Feature::BandwidthMonitoring => flags.bandwidth_monitoring = enabled,
            Feature::InterfaceHotplug => flags.interface_hotplug = enabled,
            Feature::TcCommandCaching => flags.tc_command_caching = enabled,
            Feature::ZenohAdvancedFeatures => flags.zenoh_advanced_features = enabled,
            Feature::MetricsCollection => flags.metrics_collection = enabled,
            Feature::NamespaceMonitoring => flags.namespace_monitoring = enabled,
            Feature::TcParameterValidation => flags.tc_parameter_validation = enabled,
            Feature::ExperimentalFeatures => flags.experimental_features = enabled,
            Feature::AbTesting => flags.ab_testing = enabled,
            Feature::Custom(name) => {
                flags.custom.insert(name.clone(), enabled);
            }
        }
    }
}

impl Default for FeatureToggleManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Enumeration of available features
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Feature {
    BandwidthMonitoring,
    InterfaceHotplug,
    TcCommandCaching,
    ZenohAdvancedFeatures,
    MetricsCollection,
    NamespaceMonitoring,
    TcParameterValidation,
    ExperimentalFeatures,
    AbTesting,
    Custom(String),
}

/// Macro for easy feature checking
#[macro_export]
macro_rules! feature_enabled {
    ($manager:expr, $feature:expr) => {
        $manager.is_enabled($feature)
    };
}

/// Macro for conditional execution based on feature flags
#[macro_export]
macro_rules! if_feature_enabled {
    ($manager:expr, $feature:expr, $code:block) => {
        if $manager.is_enabled($feature) {
            $code
        }
    };
}

/// Macro for conditional execution with else branch
#[macro_export]
macro_rules! if_feature_enabled_else {
    ($manager:expr, $feature:expr, $if_code:block, $else_code:block) => {
        if $manager.is_enabled($feature) {
            $if_code
        } else {
            $else_code
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_feature_flags() {
        let flags = FeatureFlags::default();
        assert!(flags.bandwidth_monitoring);
        assert!(flags.interface_hotplug);
        assert!(!flags.tc_command_caching);
        assert!(flags.zenoh_advanced_features);
        assert!(!flags.metrics_collection);
        assert!(flags.namespace_monitoring);
        assert!(flags.tc_parameter_validation);
        assert!(!flags.experimental_features);
        assert!(!flags.ab_testing);
    }

    #[test]
    fn test_feature_profile_development() {
        let profile = FeatureProfile::Development;
        let flags = profile.to_feature_flags();
        // Development should have most features enabled
        assert!(flags.bandwidth_monitoring);
        assert!(flags.interface_hotplug);
        assert!(flags.tc_command_caching);
        assert!(flags.experimental_features);
        assert!(flags.ab_testing);
    }

    #[test]
    fn test_feature_profile_production() {
        let profile = FeatureProfile::Production;
        let flags = profile.to_feature_flags();
        // Production should be more conservative
        assert!(flags.bandwidth_monitoring);
        assert!(!flags.tc_command_caching);
        assert!(!flags.experimental_features);
        assert!(!flags.ab_testing);
        assert!(!flags.metrics_collection);
    }

    #[test]
    fn test_feature_toggle_manager() {
        let manager = FeatureToggleManager::new();

        // Test default state
        assert!(manager.is_enabled(&Feature::BandwidthMonitoring));
        assert!(!manager.is_enabled(&Feature::TcCommandCaching));

        // Test toggling
        manager.enable_feature(&Feature::TcCommandCaching).unwrap();
        assert!(manager.is_enabled(&Feature::TcCommandCaching));

        manager
            .disable_feature(&Feature::BandwidthMonitoring)
            .unwrap();
        assert!(!manager.is_enabled(&Feature::BandwidthMonitoring));
    }

    #[test]
    fn test_toggle_feature() {
        let manager = FeatureToggleManager::new();
        let initial_state = manager.is_enabled(&Feature::TcCommandCaching);

        let new_state = manager.toggle_feature(&Feature::TcCommandCaching).unwrap();
        assert_eq!(new_state, !initial_state);
        assert_eq!(manager.is_enabled(&Feature::TcCommandCaching), new_state);
    }

    #[test]
    fn test_custom_features() {
        let manager = FeatureToggleManager::new();

        // Custom feature should be false by default
        assert!(!manager.is_enabled(&Feature::Custom("test_feature".to_string())));

        // Set custom feature
        manager
            .set_custom_feature("test_feature".to_string(), true)
            .unwrap();
        assert!(manager.is_enabled(&Feature::Custom("test_feature".to_string())));

        // Remove custom feature
        let was_present = manager.remove_custom_feature("test_feature").unwrap();
        assert!(was_present);
        assert!(!manager.is_enabled(&Feature::Custom("test_feature".to_string())));
    }

    #[test]
    fn test_update_multiple_features() {
        let manager = FeatureToggleManager::new();

        let mut updates = HashMap::new();
        updates.insert(Feature::TcCommandCaching, true);
        updates.insert(Feature::MetricsCollection, true);
        updates.insert(Feature::BandwidthMonitoring, false);

        manager.update_features(updates).unwrap();

        assert!(manager.is_enabled(&Feature::TcCommandCaching));
        assert!(manager.is_enabled(&Feature::MetricsCollection));
        assert!(!manager.is_enabled(&Feature::BandwidthMonitoring));
    }

    #[test]
    fn test_reset_to_profile() {
        let manager = FeatureToggleManager::with_profile(FeatureProfile::Development);

        // Modify some flags
        manager
            .disable_feature(&Feature::ExperimentalFeatures)
            .unwrap();

        // Reset should restore to profile defaults
        manager.reset_to_profile().unwrap();

        // Development profile should have these enabled
        assert!(manager.is_enabled(&Feature::ExperimentalFeatures));
    }

    #[test]
    fn test_feature_macros() {
        let manager = FeatureToggleManager::new();

        // Test basic feature check macro
        let is_enabled = feature_enabled!(manager, &Feature::BandwidthMonitoring);
        assert!(is_enabled);

        // Test conditional execution macro
        let mut executed = false;
        if_feature_enabled!(manager, &Feature::BandwidthMonitoring, {
            executed = true;
        });
        assert!(executed);

        // Test conditional with else macro
        let mut if_executed = false;
        let mut else_executed = false;
        if_feature_enabled_else!(
            manager,
            &Feature::TcCommandCaching, // Should be false by default
            {
                if_executed = true;
            },
            {
                else_executed = true;
            }
        );
        assert!(!if_executed);
        assert!(else_executed);
    }

    #[test]
    fn test_serialization() {
        let flags = FeatureFlags {
            bandwidth_monitoring: true,
            tc_command_caching: false,
            custom: {
                let mut map = HashMap::new();
                map.insert("test_feature".to_string(), true);
                map
            },
            ..FeatureFlags::default()
        };

        // Test serialization to JSON
        let json = serde_json::to_string(&flags).unwrap();
        assert!(json.contains("bandwidth_monitoring"));
        assert!(json.contains("test_feature"));

        // Test deserialization from JSON
        let deserialized: FeatureFlags = serde_json::from_str(&json).unwrap();
        assert_eq!(flags, deserialized);
    }
}
