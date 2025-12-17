//! UI state management for TC GUI frontend.
//!
//! This module handles UI visibility toggles, state management,
//! and provides utilities for managing the user interface state.

use std::collections::HashSet;

/// Available application tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppTab {
    #[default]
    Interfaces,
    Scenarios,
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
}

impl Default for UiStateManager {
    fn default() -> Self {
        Self {
            hidden_backends: HashSet::new(),
            hidden_namespaces: HashSet::new(),
            current_tab: AppTab::default(),
            interface_selection_dialog: InterfaceSelectionDialog::default(),
            zoom_level: ZOOM_DEFAULT,
        }
    }
}

impl UiStateManager {
    /// Creates a new UI state manager.
    pub fn new() -> Self {
        Self::default()
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

    /// Sets the zoom level directly (clamped to valid range).
    #[allow(dead_code)]
    pub fn set_zoom_level(&mut self, level: f32) {
        self.zoom_level = level.clamp(ZOOM_MIN, ZOOM_MAX);
    }

    /// Returns the zoom level as a percentage string (e.g., "100%").
    pub fn zoom_percentage(&self) -> String {
        format!("{}%", (self.zoom_level * 100.0).round() as i32)
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
