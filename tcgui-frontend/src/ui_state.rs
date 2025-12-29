//! UI state management for TC GUI frontend.
//!
//! This module handles UI visibility toggles, state management,
//! and provides utilities for managing the user interface state.

use std::collections::HashSet;

use crate::settings::FrontendSettings;
use crate::theme::{Theme, ThemeMode};

/// Available application tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTab {
    #[default]
    Interfaces,
    Scenarios,
}

/// View mode for interface display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InterfaceViewMode {
    /// Card view with full controls and bandwidth charts
    #[default]
    Cards,
    /// Compact table view for overview
    Table,
}

/// Interface selection dialog state
#[derive(Debug, Clone, Default)]
pub struct InterfaceSelectionDialog {
    /// Whether the dialog is visible
    pub visible: bool,
    /// Backend name for the execution
    pub backend_name: String,
    /// Scenario ID to execute
    pub scenario_id: String,
    /// Selected namespace for execution
    pub selected_namespace: Option<String>,
    /// Selected interfaces for execution (multiple selection)
    pub selected_interfaces: HashSet<String>,
    /// Whether to loop the scenario execution
    pub loop_execution: bool,
}

/// Zoom level constraints
pub const ZOOM_MIN: f32 = 0.5;
pub const ZOOM_MAX: f32 = 2.0;
pub const ZOOM_STEP: f32 = 0.1;
pub const ZOOM_DEFAULT: f32 = 1.0;

/// Filter settings for namespace types in the UI.
///
/// Controls which types of namespaces are visible in the interface list.
#[derive(Debug, Clone)]
pub struct NamespaceFilter {
    /// Show host/default namespace interfaces
    pub show_host: bool,
    /// Show traditional network namespace interfaces
    pub show_namespaces: bool,
    /// Show container namespace interfaces
    pub show_containers: bool,
}

impl Default for NamespaceFilter {
    fn default() -> Self {
        Self {
            show_host: true,
            show_namespaces: true,
            show_containers: true,
        }
    }
}

impl NamespaceFilter {
    /// Returns true if all filters are enabled
    #[allow(dead_code)]
    pub fn all_enabled(&self) -> bool {
        self.show_host && self.show_namespaces && self.show_containers
    }

    /// Returns true if no filters are enabled
    #[allow(dead_code)]
    pub fn none_enabled(&self) -> bool {
        !self.show_host && !self.show_namespaces && !self.show_containers
    }

    /// Enable all filters
    #[allow(dead_code)]
    pub fn enable_all(&mut self) {
        self.show_host = true;
        self.show_namespaces = true;
        self.show_containers = true;
    }
}

/// Manager for UI state and visibility toggles.
#[derive(Clone)]
pub struct UiStateManager {
    /// Set of backend names that are collapsed/hidden in the UI
    hidden_backends: HashSet<String>,
    /// Set of "backend_name/namespace_name" combinations that are collapsed/hidden in the UI
    hidden_namespaces: HashSet<String>,
    /// Currently selected tab
    current_tab: AppTab,
    /// Interface selection dialog state
    interface_selection_dialog: InterfaceSelectionDialog,
    /// Current zoom level (1.0 = 100%)
    zoom_level: f32,
    /// Current theme (light/dark)
    theme: Theme,
    /// Namespace type filter for interface visibility
    namespace_filter: NamespaceFilter,
    /// View mode for interface display (Cards or Table)
    interface_view_mode: InterfaceViewMode,
}

impl Default for UiStateManager {
    fn default() -> Self {
        Self {
            hidden_backends: HashSet::new(),
            hidden_namespaces: HashSet::new(),
            current_tab: AppTab::default(),
            interface_selection_dialog: InterfaceSelectionDialog::default(),
            zoom_level: ZOOM_DEFAULT,
            theme: Theme::default(),
            namespace_filter: NamespaceFilter::default(),
            interface_view_mode: InterfaceViewMode::default(),
        }
    }
}

impl UiStateManager {
    /// Creates a new UI state manager with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new UI state manager with settings loaded from disk.
    pub fn from_settings(settings: &FrontendSettings) -> Self {
        let theme_mode: ThemeMode = settings.theme_mode.into();
        let theme = match theme_mode {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
        };

        Self {
            hidden_backends: HashSet::new(),
            hidden_namespaces: HashSet::new(),
            current_tab: settings.current_tab.into(),
            interface_selection_dialog: InterfaceSelectionDialog::default(),
            zoom_level: settings.zoom_level,
            theme,
            namespace_filter: settings.namespace_filter.clone().into(),
            interface_view_mode: InterfaceViewMode::default(),
        }
    }

    /// Extracts current settings for persistence.
    pub fn to_settings(&self) -> FrontendSettings {
        use crate::settings::{AppTabJson, NamespaceFilterJson, ThemeModeJson};

        FrontendSettings {
            theme_mode: ThemeModeJson::from(self.theme.mode),
            zoom_level: self.zoom_level,
            namespace_filter: NamespaceFilterJson::from(&self.namespace_filter),
            current_tab: AppTabJson::from(self.current_tab),
        }
    }

    /// Gets the current zoom level.
    pub fn zoom_level(&self) -> f32 {
        self.zoom_level
    }

    /// Increases the zoom level by one step.
    pub fn zoom_in(&mut self) {
        self.zoom_level = (self.zoom_level + ZOOM_STEP).min(ZOOM_MAX);
    }

    /// Decreases the zoom level by one step.
    pub fn zoom_out(&mut self) {
        self.zoom_level = (self.zoom_level - ZOOM_STEP).max(ZOOM_MIN);
    }

    /// Resets the zoom level to default (1.0).
    pub fn zoom_reset(&mut self) {
        self.zoom_level = ZOOM_DEFAULT;
    }

    /// Returns the zoom level as a percentage string (e.g., "100%").
    pub fn zoom_percentage(&self) -> String {
        format!("{}%", (self.zoom_level * 100.0).round() as i32)
    }

    /// Gets the current theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Toggles between light and dark theme.
    pub fn toggle_theme(&mut self) {
        self.theme = self.theme.toggle();
    }

    /// Gets the namespace filter settings.
    pub fn namespace_filter(&self) -> &NamespaceFilter {
        &self.namespace_filter
    }

    /// Toggles the host namespace filter.
    pub fn toggle_host_filter(&mut self) {
        self.namespace_filter.show_host = !self.namespace_filter.show_host;
    }

    /// Toggles the traditional namespace filter.
    pub fn toggle_namespace_filter(&mut self) {
        self.namespace_filter.show_namespaces = !self.namespace_filter.show_namespaces;
    }

    /// Toggles the container namespace filter.
    pub fn toggle_container_filter(&mut self) {
        self.namespace_filter.show_containers = !self.namespace_filter.show_containers;
    }

    /// Enables all namespace filters.
    #[allow(dead_code)]
    pub fn enable_all_namespace_filters(&mut self) {
        self.namespace_filter.enable_all();
    }

    /// Toggles the visibility of a backend in the UI.
    pub fn toggle_backend_visibility(&mut self, backend_name: &str) {
        if self.hidden_backends.contains(backend_name) {
            self.hidden_backends.remove(backend_name);
        } else {
            self.hidden_backends.insert(backend_name.to_string());
        }
    }

    /// Toggles the visibility of a namespace within a backend.
    pub fn toggle_namespace_visibility(&mut self, backend_name: &str, namespace_name: &str) {
        let namespace_key = format!("{}/{}", backend_name, namespace_name);
        if self.hidden_namespaces.contains(&namespace_key) {
            self.hidden_namespaces.remove(&namespace_key);
        } else {
            self.hidden_namespaces.insert(namespace_key);
        }
    }

    /// Checks if a backend is hidden in the UI.
    pub fn is_backend_hidden(&self, backend_name: &str) -> bool {
        self.hidden_backends.contains(backend_name)
    }

    /// Checks if a namespace is hidden in the UI.
    pub fn is_namespace_hidden(&self, backend_name: &str, namespace_name: &str) -> bool {
        let namespace_key = format!("{}/{}", backend_name, namespace_name);
        self.hidden_namespaces.contains(&namespace_key)
    }

    /// Removes all visibility state for a specific backend.
    /// This is useful when a backend is removed from the system.
    pub fn cleanup_backend_state(&mut self, backend_name: &str) {
        // Remove the backend from hidden backends
        self.hidden_backends.remove(backend_name);

        // Remove any namespaces from this backend from hidden sets
        let namespaces_to_remove: Vec<String> = self
            .hidden_namespaces
            .iter()
            .filter(|ns_key| ns_key.starts_with(&format!("{}/", backend_name)))
            .cloned()
            .collect();

        for ns_key in namespaces_to_remove {
            self.hidden_namespaces.remove(&ns_key);
        }
    }

    /// Gets the count of hidden backends.
    pub fn hidden_backend_count(&self) -> usize {
        self.hidden_backends.len()
    }

    /// Gets the count of hidden namespaces.
    pub fn hidden_namespace_count(&self) -> usize {
        self.hidden_namespaces.len()
    }

    /// Gets all hidden backend names.
    pub fn hidden_backends(&self) -> Vec<String> {
        self.hidden_backends.iter().cloned().collect()
    }

    /// Gets all hidden namespace keys.
    pub fn hidden_namespaces(&self) -> Vec<String> {
        self.hidden_namespaces.iter().cloned().collect()
    }

    /// Shows all backends (clears hidden backend list).
    pub fn show_all_backends(&mut self) {
        self.hidden_backends.clear();
    }

    /// Shows all namespaces (clears hidden namespace list).
    pub fn show_all_namespaces(&mut self) {
        self.hidden_namespaces.clear();
    }

    /// Resets all UI state to default (everything visible).
    pub fn reset_all(&mut self) {
        self.hidden_backends.clear();
        self.hidden_namespaces.clear();
    }

    /// Get the current tab
    pub fn current_tab(&self) -> AppTab {
        self.current_tab
    }

    /// Set the current tab
    pub fn set_current_tab(&mut self, tab: AppTab) {
        self.current_tab = tab;
    }

    /// Get the current interface view mode
    pub fn interface_view_mode(&self) -> InterfaceViewMode {
        self.interface_view_mode
    }

    /// Set the interface view mode
    pub fn set_interface_view_mode(&mut self, mode: InterfaceViewMode) {
        self.interface_view_mode = mode;
    }

    /// Toggle between Cards and Table view modes
    pub fn toggle_interface_view_mode(&mut self) {
        self.interface_view_mode = match self.interface_view_mode {
            InterfaceViewMode::Cards => InterfaceViewMode::Table,
            InterfaceViewMode::Table => InterfaceViewMode::Cards,
        };
    }

    /// Show the interface selection dialog
    pub fn show_interface_selection_dialog(&mut self, backend_name: String, scenario_id: String) {
        self.interface_selection_dialog = InterfaceSelectionDialog {
            visible: true,
            backend_name,
            scenario_id,
            selected_namespace: None,
            selected_interfaces: HashSet::new(),
            loop_execution: false,
        };
    }

    /// Hide the interface selection dialog
    pub fn hide_interface_selection_dialog(&mut self) {
        self.interface_selection_dialog = InterfaceSelectionDialog::default();
    }

    /// Get the interface selection dialog state
    pub fn interface_selection_dialog(&self) -> &InterfaceSelectionDialog {
        &self.interface_selection_dialog
    }

    /// Select namespace in the dialog
    pub fn select_execution_namespace(&mut self, namespace: String) {
        self.interface_selection_dialog.selected_namespace = Some(namespace);
        // Reset interface selection when namespace changes
        self.interface_selection_dialog.selected_interfaces.clear();
    }

    /// Toggle interface selection in the dialog (for multi-select)
    pub fn toggle_execution_interface(&mut self, interface: String) {
        if self
            .interface_selection_dialog
            .selected_interfaces
            .contains(&interface)
        {
            self.interface_selection_dialog
                .selected_interfaces
                .remove(&interface);
        } else {
            self.interface_selection_dialog
                .selected_interfaces
                .insert(interface);
        }
    }

    /// Check if execution can be confirmed (namespace selected and at least one interface)
    pub fn can_confirm_execution(&self) -> bool {
        self.interface_selection_dialog.selected_namespace.is_some()
            && !self
                .interface_selection_dialog
                .selected_interfaces
                .is_empty()
    }

    /// Toggle loop execution in the dialog
    pub fn toggle_loop_execution(&mut self) {
        self.interface_selection_dialog.loop_execution =
            !self.interface_selection_dialog.loop_execution;
    }

    /// Gets visibility statistics for debugging/logging.
    pub fn get_visibility_stats(&self) -> UiVisibilityStats {
        UiVisibilityStats {
            hidden_backend_count: self.hidden_backends.len(),
            hidden_namespace_count: self.hidden_namespaces.len(),
            total_hidden_items: self.hidden_backends.len() + self.hidden_namespaces.len(),
        }
    }
}

/// Statistics about UI visibility state.
#[derive(Debug, Clone)]
pub struct UiVisibilityStats {
    pub hidden_backend_count: usize,
    pub hidden_namespace_count: usize,
    pub total_hidden_items: usize,
}

impl std::fmt::Display for UiVisibilityStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "UI Visibility: {} hidden backends, {} hidden namespaces ({} total hidden items)",
            self.hidden_backend_count, self.hidden_namespace_count, self.total_hidden_items
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_state_manager_default() {
        let manager = UiStateManager::new();
        assert_eq!(manager.current_tab(), AppTab::Interfaces);
        assert_eq!(manager.hidden_backend_count(), 0);
        assert_eq!(manager.hidden_namespace_count(), 0);
        assert!(!manager.interface_selection_dialog().visible);
    }

    #[test]
    fn test_zoom_controls() {
        let mut manager = UiStateManager::new();

        // Default zoom
        assert_eq!(manager.zoom_level(), ZOOM_DEFAULT);
        assert_eq!(manager.zoom_percentage(), "100%");

        // Zoom in
        manager.zoom_in();
        assert!((manager.zoom_level() - 1.1).abs() < 0.001);
        assert_eq!(manager.zoom_percentage(), "110%");

        // Zoom out
        manager.zoom_out();
        assert!((manager.zoom_level() - ZOOM_DEFAULT).abs() < 0.001);

        // Zoom reset
        manager.zoom_in();
        manager.zoom_in();
        manager.zoom_reset();
        assert_eq!(manager.zoom_level(), ZOOM_DEFAULT);
    }

    #[test]
    fn test_zoom_bounds() {
        let mut manager = UiStateManager::new();

        // Zoom in to max
        for _ in 0..20 {
            manager.zoom_in();
        }
        assert!((manager.zoom_level() - ZOOM_MAX).abs() < 0.001);

        // Zoom out to min
        for _ in 0..30 {
            manager.zoom_out();
        }
        assert!((manager.zoom_level() - ZOOM_MIN).abs() < 0.001);
    }

    #[test]
    fn test_backend_visibility_toggle() {
        let mut manager = UiStateManager::new();

        // Initially not hidden
        assert!(!manager.is_backend_hidden("backend1"));

        // Toggle to hidden
        manager.toggle_backend_visibility("backend1");
        assert!(manager.is_backend_hidden("backend1"));
        assert_eq!(manager.hidden_backend_count(), 1);

        // Toggle back to visible
        manager.toggle_backend_visibility("backend1");
        assert!(!manager.is_backend_hidden("backend1"));
        assert_eq!(manager.hidden_backend_count(), 0);
    }

    #[test]
    fn test_namespace_visibility_toggle() {
        let mut manager = UiStateManager::new();

        // Initially not hidden
        assert!(!manager.is_namespace_hidden("backend1", "ns1"));

        // Toggle to hidden
        manager.toggle_namespace_visibility("backend1", "ns1");
        assert!(manager.is_namespace_hidden("backend1", "ns1"));
        assert_eq!(manager.hidden_namespace_count(), 1);

        // Different namespace should not be affected
        assert!(!manager.is_namespace_hidden("backend1", "ns2"));

        // Toggle back
        manager.toggle_namespace_visibility("backend1", "ns1");
        assert!(!manager.is_namespace_hidden("backend1", "ns1"));
    }

    #[test]
    fn test_cleanup_backend_state() {
        let mut manager = UiStateManager::new();

        // Setup: hide a backend and some of its namespaces
        manager.toggle_backend_visibility("backend1");
        manager.toggle_namespace_visibility("backend1", "ns1");
        manager.toggle_namespace_visibility("backend1", "ns2");
        manager.toggle_namespace_visibility("backend2", "ns1");

        assert!(manager.is_backend_hidden("backend1"));
        assert!(manager.is_namespace_hidden("backend1", "ns1"));
        assert!(manager.is_namespace_hidden("backend1", "ns2"));
        assert!(manager.is_namespace_hidden("backend2", "ns1"));

        // Cleanup backend1
        manager.cleanup_backend_state("backend1");

        // backend1 and its namespaces should be cleaned up
        assert!(!manager.is_backend_hidden("backend1"));
        assert!(!manager.is_namespace_hidden("backend1", "ns1"));
        assert!(!manager.is_namespace_hidden("backend1", "ns2"));

        // backend2's namespace should remain
        assert!(manager.is_namespace_hidden("backend2", "ns1"));
    }

    #[test]
    fn test_show_all() {
        let mut manager = UiStateManager::new();

        // Hide multiple items
        manager.toggle_backend_visibility("backend1");
        manager.toggle_backend_visibility("backend2");
        manager.toggle_namespace_visibility("backend1", "ns1");

        assert_eq!(manager.hidden_backend_count(), 2);
        assert_eq!(manager.hidden_namespace_count(), 1);

        // Show all backends
        manager.show_all_backends();
        assert_eq!(manager.hidden_backend_count(), 0);
        assert_eq!(manager.hidden_namespace_count(), 1);

        // Show all namespaces
        manager.show_all_namespaces();
        assert_eq!(manager.hidden_namespace_count(), 0);
    }

    #[test]
    fn test_reset_all() {
        let mut manager = UiStateManager::new();

        // Hide items and change tab
        manager.toggle_backend_visibility("backend1");
        manager.toggle_namespace_visibility("backend1", "ns1");
        manager.set_current_tab(AppTab::Scenarios);

        // Reset
        manager.reset_all();
        assert_eq!(manager.hidden_backend_count(), 0);
        assert_eq!(manager.hidden_namespace_count(), 0);
        // Note: reset_all doesn't reset the tab
        assert_eq!(manager.current_tab(), AppTab::Scenarios);
    }

    #[test]
    fn test_tab_switching() {
        let mut manager = UiStateManager::new();

        assert_eq!(manager.current_tab(), AppTab::Interfaces);

        manager.set_current_tab(AppTab::Scenarios);
        assert_eq!(manager.current_tab(), AppTab::Scenarios);

        manager.set_current_tab(AppTab::Interfaces);
        assert_eq!(manager.current_tab(), AppTab::Interfaces);
    }

    #[test]
    fn test_interface_selection_dialog() {
        let mut manager = UiStateManager::new();

        // Initially hidden
        assert!(!manager.interface_selection_dialog().visible);
        assert!(!manager.can_confirm_execution());

        // Show dialog
        manager.show_interface_selection_dialog("backend1".to_string(), "scenario1".to_string());
        assert!(manager.interface_selection_dialog().visible);
        assert_eq!(
            manager.interface_selection_dialog().backend_name,
            "backend1"
        );
        assert_eq!(
            manager.interface_selection_dialog().scenario_id,
            "scenario1"
        );
        assert!(!manager.can_confirm_execution());

        // Select namespace
        manager.select_execution_namespace("ns1".to_string());
        assert_eq!(
            manager.interface_selection_dialog().selected_namespace,
            Some("ns1".to_string())
        );
        assert!(!manager.can_confirm_execution()); // Still need interface

        // Select interface
        manager.toggle_execution_interface("eth0".to_string());
        assert!(manager.can_confirm_execution());
        assert!(
            manager
                .interface_selection_dialog()
                .selected_interfaces
                .contains("eth0")
        );

        // Toggle loop execution
        assert!(!manager.interface_selection_dialog().loop_execution);
        manager.toggle_loop_execution();
        assert!(manager.interface_selection_dialog().loop_execution);

        // Hide dialog
        manager.hide_interface_selection_dialog();
        assert!(!manager.interface_selection_dialog().visible);
    }

    #[test]
    fn test_interface_selection_clears_on_namespace_change() {
        let mut manager = UiStateManager::new();

        manager.show_interface_selection_dialog("backend1".to_string(), "scenario1".to_string());
        manager.select_execution_namespace("ns1".to_string());
        manager.toggle_execution_interface("eth0".to_string());
        manager.toggle_execution_interface("eth1".to_string());

        assert_eq!(
            manager
                .interface_selection_dialog()
                .selected_interfaces
                .len(),
            2
        );

        // Change namespace - interfaces should be cleared
        manager.select_execution_namespace("ns2".to_string());
        assert!(
            manager
                .interface_selection_dialog()
                .selected_interfaces
                .is_empty()
        );
    }

    #[test]
    fn test_visibility_stats() {
        let mut manager = UiStateManager::new();

        manager.toggle_backend_visibility("backend1");
        manager.toggle_namespace_visibility("backend1", "ns1");
        manager.toggle_namespace_visibility("backend2", "ns2");

        let stats = manager.get_visibility_stats();
        assert_eq!(stats.hidden_backend_count, 1);
        assert_eq!(stats.hidden_namespace_count, 2);
        assert_eq!(stats.total_hidden_items, 3);

        // Test display
        let display = format!("{}", stats);
        assert!(display.contains("1 hidden backends"));
        assert!(display.contains("2 hidden namespaces"));
    }

    #[test]
    fn test_hidden_items_lists() {
        let mut manager = UiStateManager::new();

        manager.toggle_backend_visibility("backend1");
        manager.toggle_backend_visibility("backend2");
        manager.toggle_namespace_visibility("backend1", "ns1");

        let backends = manager.hidden_backends();
        assert_eq!(backends.len(), 2);
        assert!(backends.contains(&"backend1".to_string()));
        assert!(backends.contains(&"backend2".to_string()));

        let namespaces = manager.hidden_namespaces();
        assert_eq!(namespaces.len(), 1);
        assert!(namespaces.contains(&"backend1/ns1".to_string()));
    }
}
