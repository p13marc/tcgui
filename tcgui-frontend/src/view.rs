//! Rich view module for TC GUI frontend.
//!
//! This module provides a comprehensive UI that displays backend and interface information
//! with modern styling, bandwidth summaries, and full traffic control features.

use crate::backend_manager::{BackendGroup, BackendManager, NamespaceGroup};
use crate::bandwidth_history::BandwidthHistoryManager;
use crate::messages::TcGuiMessage;
use crate::scenario_manager::ScenarioManager;
use crate::scenario_view;
use crate::theme::{Theme, ThemeMode};
use crate::ui_state::UiStateManager;
use iced::widget::{button, checkbox, column, container, row, scrollable, space, text};
use iced::{Color, Element, Length};
use std::collections::HashMap;
use tcgui_shared::NamespaceType;

/// Helper function to scale a font size by the zoom factor.
/// Takes a base size and zoom level, returns the scaled size as f32 for Iced Pixels.
#[inline]
pub fn scaled(base: u16, zoom: f32) -> f32 {
    (base as f32) * zoom
}

/// Helper function to scale a padding value by the zoom factor.
#[inline]
pub fn scaled_padding(base: u16, zoom: f32) -> f32 {
    (base as f32) * zoom
}

/// Helper function to scale spacing values by the zoom factor.
#[inline]
pub fn scaled_spacing(base: u16, zoom: f32) -> f32 {
    (base as f32) * zoom
}

/// Color palette for consistent styling - derived from Theme
#[derive(Clone)]
pub struct ColorPalette {
    pub primary_blue: Color,
    pub success_green: Color,
    pub warning_orange: Color,
    pub error_red: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub background_primary: Color,
    pub background_card: Color,
}

impl ColorPalette {
    /// Create a color palette from a Theme
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            primary_blue: theme.colors.info,
            success_green: theme.colors.success,
            warning_orange: theme.colors.warning,
            error_red: theme.colors.error,
            text_primary: theme.colors.text_primary,
            text_secondary: theme.colors.text_secondary,
            background_primary: theme.colors.background,
            background_card: theme.colors.surface,
        }
    }
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::from_theme(&Theme::default())
    }
}

/// Renders the main application view
pub fn render_main_view<'a>(
    backend_manager: &'a BackendManager,
    bandwidth_history: &'a BandwidthHistoryManager,
    ui_state: &'a UiStateManager,
    _scenario_manager: &'a ScenarioManager,
) -> Element<'a, TcGuiMessage> {
    let theme = ui_state.theme();
    let colors = ColorPalette::from_theme(theme);
    let bg_color = colors.background_primary;
    let zoom = ui_state.zoom_level();

    // Check if any backend is connected
    let any_backend_connected = backend_manager
        .backends()
        .values()
        .any(|bg| bg.is_connected);

    let header = render_header(backend_manager, ui_state, theme, colors.clone(), zoom);
    let tabs = render_tabs(ui_state, colors.clone(), zoom);

    let content = match ui_state.current_tab() {
        crate::ui_state::AppTab::Interfaces => {
            if backend_manager.backends().is_empty() {
                render_empty_state(any_backend_connected, colors.clone(), zoom)
            } else {
                render_backend_content(
                    backend_manager,
                    bandwidth_history,
                    ui_state,
                    colors.clone(),
                    zoom,
                    theme,
                )
            }
        }
        crate::ui_state::AppTab::Scenarios => {
            scenario_view::render_scenario_view(backend_manager, _scenario_manager, zoom, theme)
        }
    };

    let main_content = container(column![header, tabs, content].spacing(scaled_spacing(12, zoom)))
        .padding(scaled_padding(12, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(bg_color)),
            ..container::Style::default()
        });

    // Add interface selection dialog overlay if visible
    if ui_state.interface_selection_dialog().visible {
        iced::widget::stack![
            main_content,
            render_interface_selection_dialog(backend_manager, ui_state, colors, zoom)
        ]
        .into()
    } else {
        main_content.into()
    }
}

/// Renders the tab navigation
fn render_tabs<'a>(
    ui_state: &'a UiStateManager,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let current_tab = ui_state.current_tab();

    let interfaces_button = button(
        text("🌐 Interfaces")
            .size(scaled(14, zoom))
            .style(move |_| text::Style {
                color: Some(if current_tab == crate::ui_state::AppTab::Interfaces {
                    Color::WHITE
                } else {
                    colors.text_primary
                }),
            }),
    )
    .on_press(TcGuiMessage::SwitchTab(crate::ui_state::AppTab::Interfaces))
    .style(move |_, _| button::Style {
        background: Some(iced::Background::Color(
            if current_tab == crate::ui_state::AppTab::Interfaces {
                colors.primary_blue
            } else {
                colors.background_card
            },
        )),
        text_color: if current_tab == crate::ui_state::AppTab::Interfaces {
            Color::WHITE
        } else {
            colors.text_primary
        },
        border: iced::Border {
            radius: 8.0.into(),
            width: 1.0,
            color: Color::from_rgb(0.88, 0.92, 0.98),
        },
        ..button::Style::default()
    });

    let scenarios_button =
        button(
            text("📊 Scenarios")
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(if current_tab == crate::ui_state::AppTab::Scenarios {
                        Color::WHITE
                    } else {
                        colors.text_primary
                    }),
                }),
        )
        .on_press(TcGuiMessage::SwitchTab(crate::ui_state::AppTab::Scenarios))
        .style(move |_, _| button::Style {
            background: Some(iced::Background::Color(
                if current_tab == crate::ui_state::AppTab::Scenarios {
                    colors.primary_blue
                } else {
                    colors.background_card
                },
            )),
            text_color: if current_tab == crate::ui_state::AppTab::Scenarios {
                Color::WHITE
            } else {
                colors.text_primary
            },
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.92, 0.98),
            },
            ..button::Style::default()
        });

    container(row![interfaces_button, scenarios_button].spacing(0))
        .padding([scaled_padding(8, zoom), scaled_padding(12, zoom)])
        .into()
}

/// Renders the application header with status and active interfaces
fn render_header<'a>(
    backend_manager: &'a BackendManager,
    ui_state: &'a UiStateManager,
    theme: &'a Theme,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let namespace_summaries = get_namespace_bandwidth_summaries(backend_manager);
    let overall_summary = get_bandwidth_summary(backend_manager);

    let status_line = render_status_line(backend_manager, colors.clone(), zoom);
    let active_interfaces_display =
        render_active_interfaces(namespace_summaries, overall_summary, colors.clone(), zoom);

    // Theme toggle button
    let theme_icon = match theme.mode {
        ThemeMode::Light => "🌙", // Moon to switch to dark
        ThemeMode::Dark => "☀️",  // Sun to switch to light
    };
    let theme_button = button(text(theme_icon).size(scaled(14, zoom)))
        .padding([scaled_padding(4, zoom), scaled_padding(8, zoom)])
        .on_press(TcGuiMessage::ToggleTheme)
        .style(move |_, _| button::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            text_color: colors.text_primary,
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: colors.text_secondary,
            },
            ..button::Style::default()
        });

    // Zoom indicator (display only - use Ctrl+Scroll or Ctrl+/- to zoom)
    let zoom_indicator = text(format!("🔍 {}", ui_state.zoom_percentage()))
        .size(scaled(12, zoom))
        .style(move |_| text::Style {
            color: Some(colors.text_secondary),
        });

    // Namespace type filter checkboxes
    let filter = ui_state.namespace_filter();
    let filter_colors = colors.clone();
    let host_checkbox = checkbox(filter.show_host)
        .label("Host")
        .on_toggle(|_| TcGuiMessage::ToggleHostFilter)
        .size(scaled(14, zoom))
        .text_size(scaled(12, zoom))
        .style(move |_, status| {
            let is_checked = match status {
                checkbox::Status::Active { is_checked }
                | checkbox::Status::Hovered { is_checked }
                | checkbox::Status::Disabled { is_checked } => is_checked,
            };
            checkbox::Style {
                background: iced::Background::Color(if is_checked {
                    filter_colors.primary_blue
                } else {
                    filter_colors.background_card
                }),
                icon_color: filter_colors.background_card,
                border: iced::Border {
                    radius: 3.0.into(),
                    width: 1.0,
                    color: filter_colors.text_secondary,
                },
                text_color: Some(filter_colors.text_primary),
            }
        });

    let filter_colors = colors.clone();
    let ns_checkbox = checkbox(filter.show_namespaces)
        .label("NS")
        .on_toggle(|_| TcGuiMessage::ToggleNamespaceTypeFilter)
        .size(scaled(14, zoom))
        .text_size(scaled(12, zoom))
        .style(move |_, status| {
            let is_checked = match status {
                checkbox::Status::Active { is_checked }
                | checkbox::Status::Hovered { is_checked }
                | checkbox::Status::Disabled { is_checked } => is_checked,
            };
            checkbox::Style {
                background: iced::Background::Color(if is_checked {
                    filter_colors.primary_blue
                } else {
                    filter_colors.background_card
                }),
                icon_color: filter_colors.background_card,
                border: iced::Border {
                    radius: 3.0.into(),
                    width: 1.0,
                    color: filter_colors.text_secondary,
                },
                text_color: Some(filter_colors.text_primary),
            }
        });

    let filter_colors = colors.clone();
    let container_checkbox = checkbox(filter.show_containers)
        .label("Containers")
        .on_toggle(|_| TcGuiMessage::ToggleContainerFilter)
        .size(scaled(14, zoom))
        .text_size(scaled(12, zoom))
        .style(move |_, status| {
            let is_checked = match status {
                checkbox::Status::Active { is_checked }
                | checkbox::Status::Hovered { is_checked }
                | checkbox::Status::Disabled { is_checked } => is_checked,
            };
            checkbox::Style {
                background: iced::Background::Color(if is_checked {
                    filter_colors.primary_blue
                } else {
                    filter_colors.background_card
                }),
                icon_color: filter_colors.background_card,
                border: iced::Border {
                    radius: 3.0.into(),
                    width: 1.0,
                    color: filter_colors.text_secondary,
                },
                text_color: Some(filter_colors.text_primary),
            }
        });

    let filter_row = row![host_checkbox, ns_checkbox, container_checkbox,]
        .spacing(scaled_spacing(12, zoom))
        .align_y(iced::Alignment::Center);

    let header_content = column![
        row![
            status_line,
            space::horizontal(),
            filter_row,
            theme_button,
            zoom_indicator,
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center),
        container(
            column![
                text("Most Active Interfaces")
                    .size(scaled(12, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                active_interfaces_display
            ]
            .spacing(scaled_spacing(4, zoom))
        )
        .padding([scaled_padding(6, zoom), 0.0])
    ]
    .spacing(scaled_spacing(10, zoom));

    container(header_content)
        .padding(scaled_padding(12, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.92, 0.98),
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.02),
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 4.0,
            },
            ..container::Style::default()
        })
        .into()
}

/// Renders the backend connection status line
fn render_status_line<'a>(
    backend_manager: &'a BackendManager,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let mut backend_statuses = Vec::new();
    let mut backend_names: Vec<_> = backend_manager.backends().keys().cloned().collect();
    backend_names.sort(); // Sort backend names for consistent display

    if backend_names.is_empty() {
        backend_statuses.push(
            text("⚠️ No backends")
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.warning_orange),
                })
                .into(),
        );
    } else {
        for (i, backend_name) in backend_names.iter().enumerate() {
            if let Some(backend_group) = backend_manager.backends().get(backend_name) {
                let (indicator, color) = if backend_group.is_connected {
                    ("🔗", colors.success_green)
                } else {
                    ("⚠️", colors.error_red)
                };

                backend_statuses.push(text(indicator).size(scaled(14, zoom)).into());
                backend_statuses.push(
                    text(backend_name.clone())
                        .size(scaled(14, zoom))
                        .style(move |_| text::Style { color: Some(color) })
                        .into(),
                );

                // Add separator if not the last backend
                if i < backend_names.len() - 1 {
                    backend_statuses.push(
                        text(" • ")
                            .size(scaled(14, zoom))
                            .style(move |_| text::Style {
                                color: Some(colors.text_secondary),
                            })
                            .into(),
                    );
                }
            }
        }
    }

    let overall_summary = get_bandwidth_summary(backend_manager);
    let connected_backends = backend_manager.connected_backend_names();
    let total_interfaces = backend_manager.total_interface_count();

    let stats_text =
        if let Some((namespace, interface, _total_rate, rate_display)) = overall_summary {
            format!(
                " • Top: {}/{} ({}) • {} connected • {} interfaces",
                namespace,
                interface,
                rate_display,
                connected_backends.len(),
                total_interfaces,
            )
        } else if backend_manager.backend_count() > 0 {
            format!(
                " • No traffic • {} connected • {} interfaces",
                connected_backends.len(),
                total_interfaces,
            )
        } else {
            " • No interfaces".to_string()
        };

    backend_statuses.push(
        text(stats_text)
            .size(scaled(14, zoom))
            .style(move |_| text::Style {
                color: Some(colors.text_primary),
            })
            .into(),
    );

    row(backend_statuses)
        .spacing(scaled_spacing(4, zoom))
        .align_y(iced::Alignment::Center)
        .into()
}

/// Renders the most active interfaces display
fn render_active_interfaces(
    namespace_summaries: HashMap<
        String,
        (String, tcgui_shared::NetworkBandwidthStats, f64, String),
    >,
    _overall_summary: Option<(String, String, f64, String)>,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'static, TcGuiMessage> {
    let mut active_interfaces: Vec<(String, String, String, f64)> = namespace_summaries
        .iter()
        .map(|(ns_key, (iface, _stats, rate, rate_display))| {
            (ns_key.clone(), iface.clone(), rate_display.clone(), *rate)
        })
        .collect();

    // Sort by rate descending (primary), then by namespace/interface name (secondary) for consistency
    active_interfaces.sort_by(|a, b| {
        // First, sort by rate descending
        let rate_cmp = b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal);
        if rate_cmp != std::cmp::Ordering::Equal {
            rate_cmp
        } else {
            // If rates are equal, sort by namespace/interface name
            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
        }
    });

    // Take top 5 most active interfaces - always show section to prevent layout changes
    if !active_interfaces.is_empty() {
        let top_interfaces: Vec<Element<TcGuiMessage>> = active_interfaces
            .into_iter()
            .take(5)
            .map(|(ns_key, iface, rate_display, _)| {
                render_active_interface_item(ns_key, iface, rate_display, colors.clone(), zoom)
            })
            .collect();

        row(top_interfaces).spacing(scaled_spacing(6, zoom)).into()
    } else {
        // Show placeholder when no active interfaces to maintain consistent layout
        row![container(
            text("📊 No active traffic")
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                })
        )
        .padding([scaled_padding(4, zoom), scaled_padding(8, zoom)])
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.4, 0.4, 0.4, 0.08,
            ))),
            border: iced::Border {
                radius: 3.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..container::Style::default()
        })]
        .into()
    }
}

/// Renders a single active interface item
fn render_active_interface_item(
    ns_key: String,
    iface: String,
    rate_display: String,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'static, TcGuiMessage> {
    // Parse backend/namespace from key
    let parts: Vec<&str> = ns_key.splitn(2, '/').collect();
    let display_ns = if parts.len() == 2 {
        if parts[1] == "default" {
            format!("{}/default", parts[0])
        } else {
            format!("{}/{}", parts[0], parts[1])
        }
    } else {
        ns_key.clone()
    };

    container(
        row![
            text("🚀").size(scaled(11, zoom)),
            text(format!("{}: {}", display_ns, iface))
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_primary)
                }),
            space::horizontal(),
            text(rate_display)
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.primary_blue)
                })
        ]
        .spacing(scaled_spacing(4, zoom))
        .align_y(iced::Alignment::Center),
    )
    .padding([scaled_padding(4, zoom), scaled_padding(8, zoom)])
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(Color::from_rgba(
            0.2, 0.6, 1.0, 0.08,
        ))),
        border: iced::Border {
            radius: 3.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..container::Style::default()
    })
    .into()
}

/// Renders the empty state when no backends are available
fn render_empty_state(
    any_backend_connected: bool,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'static, TcGuiMessage> {
    if any_backend_connected {
        container(
            column![
                text("📡").size(scaled(48, zoom)),
                text("No Network Interfaces")
                    .size(scaled(20, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                text("No network interfaces are currently available")
                    .size(scaled(14, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
            ]
            .spacing(scaled_spacing(12, zoom))
            .align_x(iced::Alignment::Center),
        )
        .padding(scaled_padding(40, zoom))
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 12.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.92, 0.98),
            },
            ..container::Style::default()
        })
        .into()
    } else {
        container(
            container(
                column![
                    text("🔄").size(scaled(48, zoom)),
                    text("Connecting to Backend")
                        .size(scaled(20, zoom))
                        .style(move |_| text::Style {
                            color: Some(colors.text_primary)
                        }),
                    text("Waiting for backend connection to discover interfaces...")
                        .size(scaled(14, zoom))
                        .style(move |_| text::Style {
                            color: Some(colors.text_secondary)
                        }),
                ]
                .spacing(scaled_spacing(12, zoom))
                .align_x(iced::Alignment::Center),
            )
            .padding(scaled_padding(40, zoom))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    1.0, 0.6, 0.0, 0.05,
                ))),
                border: iced::Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.95, 0.85, 0.7),
                },
                ..container::Style::default()
            }),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}

/// Renders the main backend content with namespaces and interfaces
fn render_backend_content<'a>(
    backend_manager: &'a BackendManager,
    bandwidth_history: &'a BandwidthHistoryManager,
    ui_state: &'a UiStateManager,
    colors: ColorPalette,
    zoom: f32,
    theme: &'a Theme,
) -> Element<'a, TcGuiMessage> {
    let namespace_sections = render_namespace_sections(
        backend_manager,
        bandwidth_history,
        ui_state,
        colors.clone(),
        zoom,
        theme,
    );
    let all_namespaces_column: Element<_> = column(namespace_sections)
        .spacing(scaled_spacing(8, zoom))
        .into();

    // Add UI stats footer
    let ui_stats_footer = render_ui_stats_footer(ui_state, colors.clone(), zoom);

    let content_with_footer =
        column![all_namespaces_column, ui_stats_footer,].spacing(scaled_spacing(8, zoom));

    // Enhanced scrollable content with modern styling
    let scrollable_content = scrollable(content_with_footer)
        .height(Length::Fill)
        .width(Length::Fill);

    scrollable_content.into()
}

/// Renders all namespace sections
fn render_namespace_sections<'a>(
    backend_manager: &'a BackendManager,
    bandwidth_history: &'a BandwidthHistoryManager,
    ui_state: &'a UiStateManager,
    colors: ColorPalette,
    zoom: f32,
    theme: &'a Theme,
) -> Vec<Element<'a, TcGuiMessage>> {
    let mut namespace_sections: Vec<Element<TcGuiMessage>> = Vec::new();

    // Sort backends for consistent display order
    let mut sorted_backends: Vec<_> = backend_manager.backends().iter().collect();
    sorted_backends.sort_by_key(|(name, _)| {
        // Put "default" backend first, then alphabetical
        if *name == "default" {
            (0, (*name).clone())
        } else {
            (1, (*name).clone())
        }
    });

    let namespace_bandwidth_summaries = get_namespace_bandwidth_summaries(backend_manager);

    for (backend_name, backend_group) in sorted_backends {
        // Skip hidden backends
        if ui_state.is_backend_hidden(backend_name) {
            continue;
        }

        let backend_namespace_sections = render_backend_namespaces(
            backend_name,
            backend_group,
            bandwidth_history,
            ui_state,
            namespace_bandwidth_summaries.clone(),
            colors.clone(),
            zoom,
            theme,
        );
        namespace_sections.extend(backend_namespace_sections);
    }

    namespace_sections
}

/// Renders namespaces for a specific backend
#[allow(clippy::too_many_arguments)]
fn render_backend_namespaces<'a>(
    backend_name: &'a str,
    backend_group: &'a BackendGroup,
    bandwidth_history: &'a BandwidthHistoryManager,
    ui_state: &'a UiStateManager,
    namespace_bandwidth_summaries: HashMap<
        String,
        (String, tcgui_shared::NetworkBandwidthStats, f64, String),
    >,
    colors: ColorPalette,
    zoom: f32,
    theme: &'a Theme,
) -> Vec<Element<'a, TcGuiMessage>> {
    let mut sections = Vec::new();

    // Sort namespaces within each backend
    let mut sorted_namespaces: Vec<_> = backend_group.namespaces.iter().collect();
    sorted_namespaces.sort_by_key(|(name, _)| {
        // Put "default" namespace first, then alphabetical
        if *name == "default" {
            (0, (*name).clone())
        } else {
            (1, (*name).clone())
        }
    });

    for (namespace_name, namespace_group) in sorted_namespaces {
        if !namespace_group.tc_interfaces.is_empty() {
            // Apply namespace type filter
            let ns_type = &namespace_group.namespace.namespace_type;
            let filter = ui_state.namespace_filter();
            let should_show = match ns_type {
                NamespaceType::Default => filter.show_host,
                NamespaceType::Traditional => filter.show_namespaces,
                NamespaceType::Container { .. } => filter.show_containers,
            };

            if !should_show {
                continue;
            }

            let namespace_key = format!("{}/{}", backend_name, namespace_name);
            let is_hidden = ui_state.is_namespace_hidden(backend_name, namespace_name);

            let section = render_namespace_section(
                backend_name,
                namespace_name,
                namespace_group,
                &backend_group.preset_list,
                bandwidth_history,
                namespace_key,
                is_hidden,
                ns_type,
                namespace_bandwidth_summaries.clone(),
                colors.clone(),
                zoom,
                theme,
            );
            sections.push(section);
        }
    }

    sections
}

/// Renders a single namespace section
#[allow(clippy::too_many_arguments)]
fn render_namespace_section<'a>(
    backend_name: &'a str,
    namespace_name: &'a str,
    namespace_group: &'a NamespaceGroup,
    preset_list: &'a tcgui_shared::presets::PresetList,
    bandwidth_history: &'a BandwidthHistoryManager,
    namespace_key: String,
    is_hidden: bool,
    namespace_type: &'a NamespaceType,
    namespace_bandwidth_summaries: HashMap<
        String,
        (String, tcgui_shared::NetworkBandwidthStats, f64, String),
    >,
    colors: ColorPalette,
    zoom: f32,
    theme: &'a Theme,
) -> Element<'a, TcGuiMessage> {
    let namespace_header = render_namespace_header(
        backend_name,
        namespace_name,
        namespace_key.clone(),
        is_hidden,
        namespace_type,
        namespace_bandwidth_summaries.clone(),
        colors.clone(),
        zoom,
    );

    if is_hidden {
        // Compact namespace header when hidden
        container(namespace_header)
            .padding(scaled_padding(16, zoom))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(Color::from_rgba(
                    0.4, 0.4, 0.4, 0.05,
                ))),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.85, 0.85, 0.85),
                },
                ..container::Style::default()
            })
            .into()
    } else {
        // Full namespace view with interface cards
        let interfaces = render_namespace_interfaces(
            backend_name,
            namespace_name,
            namespace_group,
            preset_list,
            theme,
            zoom,
            bandwidth_history,
        );
        let interfaces_column: Element<_> =
            column(interfaces).spacing(scaled_spacing(4, zoom)).into();

        // Modern namespace container with card styling
        container(column![namespace_header, interfaces_column].spacing(scaled_spacing(16, zoom)))
            .padding(scaled_padding(20, zoom))
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(colors.background_card)),
                border: iced::Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.85, 0.9, 0.98),
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.04),
                    offset: iced::Vector::new(0.0, 2.0),
                    blur_radius: 8.0,
                },
                ..container::Style::default()
            })
            .into()
    }
}

/// Renders the namespace header with title and controls
#[allow(clippy::too_many_arguments)]
fn render_namespace_header<'a>(
    backend_name: &'a str,
    namespace_name: &'a str,
    namespace_key: String,
    is_hidden: bool,
    namespace_type: &'a NamespaceType,
    namespace_bandwidth_summaries: HashMap<
        String,
        (String, tcgui_shared::NetworkBandwidthStats, f64, String),
    >,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    // Determine icon and display info based on namespace type
    let (namespace_icon, display_name, subtitle) = match namespace_type {
        NamespaceType::Default => ("🏠", namespace_name.to_string(), None),
        NamespaceType::Traditional => ("📁", namespace_name.to_string(), None),
        NamespaceType::Container {
            runtime,
            container_id,
            image,
        } => {
            // Use container-specific icons
            let icon = match runtime.to_lowercase().as_str() {
                "docker" => "🐳",
                "podman" => "🦭",
                _ => "📦",
            };
            // Strip "container:" prefix from display name
            let name = namespace_name
                .strip_prefix("container:")
                .unwrap_or(namespace_name)
                .to_string();
            let sub = format!("{} | {} | {}", runtime, container_id, image);
            (icon, name, Some(sub))
        }
    };

    let namespace_title = if let Some(sub) = subtitle {
        column![
            text(format!(
                "{} {} ({})",
                namespace_icon, display_name, backend_name
            ))
            .size(scaled(20, zoom))
            .style(move |_| text::Style {
                color: Some(colors.text_primary),
            }),
            text(sub)
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary),
                })
        ]
        .spacing(scaled_spacing(2, zoom))
    } else {
        column![text(format!(
            "{} {} ({})",
            namespace_icon, display_name, backend_name
        ))
        .size(scaled(20, zoom))
        .style(move |_| text::Style {
            color: Some(colors.text_primary),
        })]
    };

    // Enhanced toggle button with modern styling
    let toggle_button = render_toggle_button(
        backend_name,
        namespace_name,
        is_hidden,
        colors.clone(),
        zoom,
    );

    // Enhanced per-namespace bandwidth summary
    let namespace_bandwidth_summary = render_namespace_bandwidth_summary(
        namespace_key.clone(),
        namespace_bandwidth_summaries.clone(),
        colors.clone(),
        zoom,
    );

    row![
        namespace_title,
        space::horizontal(),
        namespace_bandwidth_summary,
        toggle_button,
    ]
    .spacing(scaled_spacing(12, zoom))
    .align_y(iced::Alignment::Center)
    .into()
}

/// Renders the toggle button for namespace visibility
fn render_toggle_button<'a>(
    backend_name: &'a str,
    namespace_name: &'a str,
    is_hidden: bool,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let backend_clone = backend_name.to_string();
    let namespace_clone = namespace_name.to_string();

    if is_hidden {
        button(
            row![
                text("👁").size(scaled(12, zoom)),
                text("Show").size(scaled(12, zoom))
            ]
            .spacing(scaled_spacing(4, zoom))
            .align_y(iced::Alignment::Center),
        )
        .padding(scaled_padding(8, zoom))
        .on_press(TcGuiMessage::ToggleNamespaceVisibility(
            backend_clone,
            namespace_clone,
        ))
        .style(move |_, _| button::Style {
            background: Some(iced::Background::Color(colors.success_green)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.1),
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 2.0,
            },
            snap: false,
        })
        .into()
    } else {
        button(
            row![
                text("🙈").size(scaled(12, zoom)),
                text("Hide").size(scaled(12, zoom))
            ]
            .spacing(scaled_spacing(4, zoom))
            .align_y(iced::Alignment::Center),
        )
        .padding(scaled_padding(8, zoom))
        .on_press(TcGuiMessage::ToggleNamespaceVisibility(
            backend_clone,
            namespace_clone,
        ))
        .style(move |_, _| button::Style {
            background: Some(iced::Background::Color(colors.text_secondary)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            shadow: iced::Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.1),
                offset: iced::Vector::new(0.0, 1.0),
                blur_radius: 2.0,
            },
            snap: false,
        })
        .into()
    }
}

/// Renders the namespace bandwidth summary
fn render_namespace_bandwidth_summary<'a>(
    namespace_key: String,
    namespace_bandwidth_summaries: HashMap<
        String,
        (String, tcgui_shared::NetworkBandwidthStats, f64, String),
    >,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    if let Some((interface_name, _stats, _total_rate, rate_display)) =
        namespace_bandwidth_summaries.get(&namespace_key)
    {
        container(
            row![
                text("📈").size(scaled(14, zoom)),
                text(format!("Top: {} ({})", interface_name, rate_display))
                    .size(scaled(13, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.primary_blue)
                    })
            ]
            .spacing(scaled_spacing(6, zoom))
            .align_y(iced::Alignment::Center),
        )
        .padding(scaled_padding(8, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.2, 0.6, 1.0, 0.1,
            ))),
            border: iced::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..container::Style::default()
        })
        .into()
    } else {
        container(
            row![
                text("📊").size(scaled(14, zoom)),
                text("No active traffic")
                    .size(scaled(13, zoom))
                    .style(move |_| {
                        text::Style {
                            color: Some(colors.text_secondary),
                        }
                    })
            ]
            .spacing(scaled_spacing(6, zoom))
            .align_y(iced::Alignment::Center),
        )
        .padding(scaled_padding(8, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.4, 0.4, 0.4, 0.1,
            ))),
            border: iced::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: Color::TRANSPARENT,
            },
            ..container::Style::default()
        })
        .into()
    }
}

/// Renders the interfaces within a namespace
fn render_namespace_interfaces<'a>(
    backend_name: &'a str,
    namespace_name: &'a str,
    namespace_group: &'a NamespaceGroup,
    preset_list: &'a tcgui_shared::presets::PresetList,
    theme: &'a Theme,
    zoom: f32,
    bandwidth_history: &'a BandwidthHistoryManager,
) -> Vec<Element<'a, TcGuiMessage>> {
    // Sort interfaces alphabetically for consistent order
    let mut sorted_interfaces: Vec<_> = namespace_group.tc_interfaces.iter().collect();
    sorted_interfaces.sort_by_key(|(name, _)| (*name).clone());

    sorted_interfaces
        .into_iter()
        .map(|(name, interface)| {
            let name_clone = name.clone();
            let backend_clone = backend_name.to_string();
            let namespace_clone = namespace_name.to_string();
            // Get bandwidth history for this interface
            let history = bandwidth_history.get(backend_name, namespace_name, name);
            interface
                .view(preset_list, theme, zoom, history)
                .map(move |msg| {
                    TcGuiMessage::TcInterfaceMessage(
                        backend_clone.clone(),
                        namespace_clone.clone(),
                        name_clone.clone(),
                        msg,
                    )
                })
        })
        .collect()
}

/// Renders UI statistics footer to show visibility stats
fn render_ui_stats_footer(
    ui_state: &UiStateManager,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'_, TcGuiMessage> {
    let visibility_stats = ui_state.get_visibility_stats();
    let hidden_backend_count = ui_state.hidden_backend_count();
    let hidden_namespace_count = ui_state.hidden_namespace_count();
    let hidden_backends = ui_state.hidden_backends();
    let hidden_namespaces = ui_state.hidden_namespaces();

    if hidden_backend_count > 0 || hidden_namespace_count > 0 {
        let stats_text = format!(
            "🙈 UI Stats: {} hidden backends, {} hidden namespaces (Total: {})",
            visibility_stats.hidden_backend_count,
            visibility_stats.hidden_namespace_count,
            visibility_stats.total_hidden_items
        );

        let details_text = if !hidden_backends.is_empty() || !hidden_namespaces.is_empty() {
            let mut details = vec![];
            if !hidden_backends.is_empty() {
                details.push(format!("Backends: {}", hidden_backends.join(", ")));
            }
            if !hidden_namespaces.is_empty() {
                details.push(format!("Namespaces: {}", hidden_namespaces.join(", ")));
            }
            format!(" ({})", details.join("; "))
        } else {
            String::new()
        };

        container(column![row![
            text(format!("{}{}", stats_text, details_text))
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
            space::horizontal(),
            button(text("Show All").size(scaled(11, zoom)))
                .padding([scaled_padding(3, zoom), scaled_padding(6, zoom)])
                .on_press(TcGuiMessage::ShowAllNamespaces)
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.primary_blue)),
                    text_color: Color::WHITE,
                    border: iced::Border {
                        radius: 3.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..button::Style::default()
                }),
            button(text("Show All Backends").size(scaled(11, zoom)))
                .padding([scaled_padding(3, zoom), scaled_padding(6, zoom)])
                .on_press(TcGuiMessage::ShowAllBackends)
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.success_green)),
                    text_color: Color::WHITE,
                    border: iced::Border {
                        radius: 3.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..button::Style::default()
                }),
            button(text("Reset").size(scaled(11, zoom)))
                .padding([scaled_padding(3, zoom), scaled_padding(6, zoom)])
                .on_press(TcGuiMessage::ResetUiState)
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.warning_orange)),
                    text_color: Color::WHITE,
                    border: iced::Border {
                        radius: 3.0.into(),
                        width: 0.0,
                        color: Color::TRANSPARENT,
                    },
                    ..button::Style::default()
                }),
        ]
        .spacing(scaled_spacing(6, zoom))
        .align_y(iced::Alignment::Center)])
        .padding(scaled_padding(10, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.4, 0.4, 0.4, 0.05,
            ))),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.85, 0.85, 0.85),
            },
            ..container::Style::default()
        })
        .into()
    } else {
        // Empty element when no hidden items
        container(text("")).height(Length::Fixed(0.0)).into()
    }
}

// Helper functions for bandwidth summaries

fn get_namespace_bandwidth_summaries(
    backend_manager: &BackendManager,
) -> HashMap<String, (String, tcgui_shared::NetworkBandwidthStats, f64, String)> {
    let mut summaries = HashMap::new();

    for (backend_name, backend_group) in backend_manager.backends() {
        for (namespace_name, namespace_group) in &backend_group.namespaces {
            let namespace_key = format!("{}/{}", backend_name, namespace_name);

            // Find the interface with highest combined traffic
            let mut max_rate = 0.0;
            let mut top_interface: Option<(&str, &tcgui_shared::NetworkBandwidthStats)> = None;

            for (interface_name, tc_interface) in &namespace_group.tc_interfaces {
                if let Some(stats) = tc_interface.bandwidth_stats() {
                    let total_rate = stats.rx_bytes_per_sec + stats.tx_bytes_per_sec;
                    if total_rate > max_rate {
                        max_rate = total_rate;
                        top_interface = Some((interface_name, stats));
                    }
                }
            }

            if let Some((interface_name, stats)) = top_interface {
                let rate_display = format_bandwidth_rate(max_rate);
                summaries.insert(
                    namespace_key,
                    (
                        interface_name.to_string(),
                        stats.clone(),
                        max_rate,
                        rate_display,
                    ),
                );
            }
        }
    }

    summaries
}

fn get_bandwidth_summary(
    backend_manager: &BackendManager,
) -> Option<(String, String, f64, String)> {
    let mut max_rate = 0.0;
    let mut top_interface: Option<(String, String, f64)> = None;

    for (backend_name, backend_group) in backend_manager.backends() {
        for (namespace_name, namespace_group) in &backend_group.namespaces {
            for (interface_name, tc_interface) in &namespace_group.tc_interfaces {
                if let Some(stats) = tc_interface.bandwidth_stats() {
                    let total_rate = stats.rx_bytes_per_sec + stats.tx_bytes_per_sec;
                    if total_rate > max_rate {
                        max_rate = total_rate;
                        let namespace_key = if namespace_name == "default" {
                            backend_name.to_string()
                        } else {
                            format!("{}/{}", backend_name, namespace_name)
                        };
                        top_interface =
                            Some((namespace_key, interface_name.to_string(), total_rate));
                    }
                }
            }
        }
    }

    top_interface.map(|(namespace, interface, rate)| {
        (namespace, interface, rate, format_bandwidth_rate(rate))
    })
}

fn format_bandwidth_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_073_741_824.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    } else if bytes_per_sec > 0.0 {
        format!("{:.0} B/s", bytes_per_sec)
    } else {
        "0 B/s".to_string()
    }
}

/// Renders the interface selection dialog overlay
fn render_interface_selection_dialog<'a>(
    backend_manager: &'a BackendManager,
    ui_state: &'a UiStateManager,
    colors: ColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let dialog = ui_state.interface_selection_dialog();

    // Get the backend
    let backend_group = backend_manager.backends().get(&dialog.backend_name);

    if let Some(backend) = backend_group {
        let mut content = column![
            // Dialog header
            row![
                text(format!(
                    "🎯 Select Interface for Scenario: {}",
                    dialog.scenario_id
                ))
                .size(scaled(18, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_primary),
                }),
                space().width(Length::Fill),
                button(text("✕").size(scaled(14, zoom)))
                    .on_press(TcGuiMessage::HideInterfaceSelectionDialog)
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(colors.error_red)),
                        text_color: Color::WHITE,
                        border: iced::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        ..button::Style::default()
                    })
            ]
            .spacing(scaled_spacing(12, zoom))
            .align_y(iced::Alignment::Center),
            // Instructions
            text("Please select a network namespace and interface to execute the scenario on:")
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary),
                }),
        ]
        .spacing(scaled_spacing(16, zoom));

        // Namespace and interface selection
        let mut namespaces_column = column![].spacing(scaled_spacing(12, zoom));

        // Sort namespaces (default first)
        let mut sorted_namespaces: Vec<_> = backend.namespaces.iter().collect();
        sorted_namespaces.sort_by_key(|(name, _)| {
            if *name == "default" {
                (0, (*name).clone())
            } else {
                (1, (*name).clone())
            }
        });

        for (namespace_name, namespace_group) in sorted_namespaces {
            if namespace_group.tc_interfaces.is_empty() {
                continue;
            }

            let is_selected_namespace = dialog.selected_namespace.as_ref() == Some(namespace_name);

            // Namespace button
            let namespace_button = button(
                text(format!(
                    "🏷️ {} ({} interfaces)",
                    if namespace_name == "default" {
                        "default (host)"
                    } else {
                        namespace_name
                    },
                    namespace_group.tc_interfaces.len()
                ))
                .size(scaled(14, zoom)),
            )
            .width(Length::Fill)
            .on_press(TcGuiMessage::SelectExecutionNamespace(
                namespace_name.clone(),
            ))
            .style(move |_, _| button::Style {
                background: Some(iced::Background::Color(if is_selected_namespace {
                    colors.primary_blue
                } else {
                    colors.background_card
                })),
                text_color: if is_selected_namespace {
                    Color::WHITE
                } else {
                    colors.text_primary
                },
                border: iced::Border {
                    radius: 6.0.into(),
                    width: 1.0,
                    color: if is_selected_namespace {
                        colors.primary_blue
                    } else {
                        Color::from_rgb(0.88, 0.92, 0.98)
                    },
                },
                ..button::Style::default()
            });

            namespaces_column = namespaces_column.push(namespace_button);

            // Show interfaces for selected namespace
            if is_selected_namespace {
                let mut interfaces_row = row![].spacing(scaled_spacing(8, zoom));

                // Sort interfaces alphabetically
                let mut sorted_interfaces: Vec<_> = namespace_group.tc_interfaces.iter().collect();
                sorted_interfaces.sort_by_key(|(name, _)| *name);

                for (interface_name, _) in sorted_interfaces {
                    let is_selected_interface = dialog.selected_interfaces.contains(interface_name);

                    let interface_button = button(
                        row![
                            text(if is_selected_interface { "☑" } else { "☐" })
                                .size(scaled(14, zoom)),
                            text(interface_name).size(scaled(12, zoom))
                        ]
                        .spacing(scaled_spacing(6, zoom)),
                    )
                    .padding([scaled_padding(6, zoom), scaled_padding(12, zoom)])
                    .on_press(TcGuiMessage::ToggleExecutionInterface(
                        interface_name.clone(),
                    ))
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(if is_selected_interface {
                            colors.success_green
                        } else {
                            Color::from_rgb(0.95, 0.97, 1.0)
                        })),
                        text_color: if is_selected_interface {
                            Color::WHITE
                        } else {
                            colors.text_primary
                        },
                        border: iced::Border {
                            radius: 4.0.into(),
                            width: 1.0,
                            color: if is_selected_interface {
                                colors.success_green
                            } else {
                                Color::from_rgb(0.9, 0.93, 0.98)
                            },
                        },
                        ..button::Style::default()
                    });

                    interfaces_row = interfaces_row.push(interface_button);
                }

                // Show selected count
                let selected_count = dialog.selected_interfaces.len();
                let selection_info = if selected_count == 0 {
                    "Select one or more interfaces:".to_string()
                } else {
                    format!(
                        "Selected {} interface{}:",
                        selected_count,
                        if selected_count == 1 { "" } else { "s" }
                    )
                };

                namespaces_column = namespaces_column.push(
                    container(
                        column![
                            text(selection_info).size(scaled(12, zoom)).style(move |_| {
                                text::Style {
                                    color: Some(colors.text_secondary),
                                }
                            }),
                            interfaces_row.wrap()
                        ]
                        .spacing(scaled_spacing(8, zoom)),
                    )
                    .padding([0.0, scaled_padding(20, zoom)]),
                );
            }
        }

        content = content.push(namespaces_column);

        // Loop execution toggle
        let loop_enabled = dialog.loop_execution;
        let loop_toggle = row![
            button(text(if loop_enabled { "🔁" } else { "➡️" }).size(scaled(16, zoom)))
                .padding([scaled_padding(6, zoom), scaled_padding(10, zoom)])
                .on_press(TcGuiMessage::ToggleLoopExecution)
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(if loop_enabled {
                        colors.primary_blue
                    } else {
                        Color::from_rgb(0.95, 0.97, 1.0)
                    })),
                    text_color: if loop_enabled {
                        Color::WHITE
                    } else {
                        colors.text_primary
                    },
                    border: iced::Border {
                        radius: 6.0.into(),
                        width: 1.0,
                        color: if loop_enabled {
                            colors.primary_blue
                        } else {
                            Color::from_rgb(0.9, 0.93, 0.98)
                        },
                    },
                    ..button::Style::default()
                }),
            text(if loop_enabled {
                "Loop enabled - scenario will repeat"
            } else {
                "Run once"
            })
            .size(scaled(13, zoom))
            .style(move |_| text::Style {
                color: Some(if loop_enabled {
                    colors.primary_blue
                } else {
                    colors.text_secondary
                }),
            })
        ]
        .spacing(scaled_spacing(10, zoom))
        .align_y(iced::Alignment::Center);

        content = content.push(loop_toggle);

        // Action buttons
        let can_confirm = ui_state.can_confirm_execution();
        let action_row = row![
            button(text("Cancel").size(scaled(14, zoom)))
                .padding([scaled_padding(8, zoom), scaled_padding(16, zoom)])
                .on_press(TcGuiMessage::HideInterfaceSelectionDialog)
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(Color::from_rgb(0.9, 0.9, 0.9))),
                    text_color: colors.text_primary,
                    border: iced::Border {
                        radius: 6.0.into(),
                        width: 1.0,
                        color: Color::from_rgb(0.8, 0.8, 0.8),
                    },
                    ..button::Style::default()
                }),
            space().width(Length::Fill),
            {
                let mut btn = button(text("Execute Scenario").size(scaled(14, zoom)))
                    .padding([scaled_padding(8, zoom), scaled_padding(16, zoom)])
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(if can_confirm {
                            colors.success_green
                        } else {
                            Color::from_rgb(0.8, 0.8, 0.8)
                        })),
                        text_color: Color::WHITE,
                        border: iced::Border {
                            radius: 6.0.into(),
                            width: 0.0,
                            color: Color::TRANSPARENT,
                        },
                        ..button::Style::default()
                    });
                if can_confirm {
                    btn = btn.on_press(TcGuiMessage::ConfirmScenarioExecution);
                }
                btn
            }
        ]
        .spacing(scaled_spacing(12, zoom));

        content = content.push(action_row);

        // Dialog container with backdrop
        container(
            container(content)
                .padding(scaled_padding(24, zoom))
                .max_width(600)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(Color::WHITE)),
                    border: iced::Border {
                        radius: 12.0.into(),
                        width: 1.0,
                        color: Color::from_rgb(0.8, 0.85, 0.95),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.3),
                        offset: iced::Vector::new(0.0, 8.0),
                        blur_radius: 16.0,
                    },
                    ..container::Style::default()
                }),
        )
        .padding(scaled_padding(40, zoom))
        .center(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.0, 0.0, 0.0, 0.5,
            ))),
            ..container::Style::default()
        })
        .into()
    } else {
        // Backend not found, show error
        container(
            container(
                column![
                    text(format!("⚠️ Backend '{}' not found", dialog.backend_name))
                        .size(scaled(16, zoom))
                        .style(move |_| text::Style {
                            color: Some(colors.error_red),
                        }),
                    button(text("Close").size(scaled(14, zoom)))
                        .padding([scaled_padding(8, zoom), scaled_padding(16, zoom)])
                        .on_press(TcGuiMessage::HideInterfaceSelectionDialog)
                        .style(move |_, _| button::Style {
                            background: Some(iced::Background::Color(colors.error_red)),
                            text_color: Color::WHITE,
                            border: iced::Border {
                                radius: 6.0.into(),
                                width: 0.0,
                                color: Color::TRANSPARENT,
                            },
                            ..button::Style::default()
                        })
                ]
                .spacing(scaled_spacing(16, zoom)),
            )
            .padding(scaled_padding(24, zoom))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(Color::WHITE)),
                border: iced::Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: colors.error_red,
                },
                ..container::Style::default()
            }),
        )
        .padding(scaled_padding(40, zoom))
        .center(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(Color::from_rgba(
                0.0, 0.0, 0.0, 0.5,
            ))),
            ..container::Style::default()
        })
        .into()
    }
}
