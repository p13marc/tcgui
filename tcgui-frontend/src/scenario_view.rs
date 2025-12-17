//! Scenario view components for TC GUI frontend.
//!
//! This module provides UI components for displaying and managing scenarios,
//! including scenario lists, details view, and execution controls.

use iced::widget::{button, column, container, row, scrollable, space, text, text_input, Column};
use iced::{Color, Element, Length};

use tcgui_shared::scenario::{ExecutionState, NetworkScenario, ScenarioExecution};

use crate::backend_manager::BackendManager;
use crate::messages::TcGuiMessage;
use crate::scenario_manager::{ScenarioManager, ScenarioSortOption};
use crate::theme::Theme;
use crate::view::{scaled, scaled_padding, scaled_spacing};

/// Format a duration in milliseconds to a human-readable string
fn format_duration(duration_ms: u64) -> String {
    if duration_ms >= 60000 {
        let minutes = duration_ms / 60000;
        let seconds = (duration_ms % 60000) / 1000;
        if seconds > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}m", minutes)
        }
    } else if duration_ms >= 1000 {
        let seconds = duration_ms / 1000;
        let ms = duration_ms % 1000;
        if ms > 0 {
            format!("{}.{}s", seconds, ms / 100)
        } else {
            format!("{}s", seconds)
        }
    } else {
        format!("{}ms", duration_ms)
    }
}

/// Color palette for scenario UI styling
#[derive(Clone)]
pub struct ScenarioColorPalette {
    pub primary_blue: Color,
    pub success_green: Color,
    pub warning_orange: Color,
    pub error_red: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub background_card: Color,
    pub background_light: Color,
    pub border_color: Color,
}

impl ScenarioColorPalette {
    /// Create a color palette from a theme
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            primary_blue: theme.colors.info,
            success_green: theme.colors.success,
            warning_orange: theme.colors.warning,
            error_red: theme.colors.error,
            text_primary: theme.colors.text_primary,
            text_secondary: theme.colors.text_secondary,
            background_card: theme.colors.surface,
            background_light: theme.colors.background,
            border_color: theme.colors.border,
        }
    }
}

impl Default for ScenarioColorPalette {
    fn default() -> Self {
        Self {
            primary_blue: Color::from_rgb(0.2, 0.6, 1.0),
            success_green: Color::from_rgb(0.0, 0.8, 0.3),
            warning_orange: Color::from_rgb(1.0, 0.6, 0.0),
            error_red: Color::from_rgb(0.9, 0.2, 0.2),
            text_primary: Color::from_rgb(0.1, 0.1, 0.1),
            text_secondary: Color::from_rgb(0.5, 0.5, 0.5),
            background_card: Color::from_rgb(0.98, 0.99, 1.0),
            background_light: Color::from_rgb(0.97, 0.98, 1.0),
            border_color: Color::from_rgb(0.88, 0.92, 0.98),
        }
    }
}

/// Renders the main scenario management view
pub fn render_scenario_view<'a>(
    backend_manager: &'a BackendManager,
    scenario_manager: &'a ScenarioManager,
    zoom: f32,
    theme: &'a Theme,
) -> Element<'a, TcGuiMessage> {
    let colors = ScenarioColorPalette::from_theme(theme);

    // Check if we have connected backends
    let connected_backends: Vec<String> = backend_manager.connected_backend_names();

    if connected_backends.is_empty() {
        return render_no_backends(colors, zoom);
    }

    let mut content = column![];

    // Header
    content = content.push(
        container(
            column![
                text("📊 Network Scenarios")
                    .size(scaled(24, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                text("Manage and execute network condition scenarios")
                    .size(scaled(14, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    })
            ]
            .spacing(scaled_spacing(4, zoom)),
        )
        .padding(scaled_padding(16, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: colors.border_color,
            },
            ..container::Style::default()
        }),
    );

    // Show scenario details if selected
    if scenario_manager.is_showing_details() {
        if let Some(scenario) = scenario_manager.get_selected_scenario() {
            content = content.push(render_scenario_details(scenario, colors.clone(), zoom));
        }
    }

    // Scenario sections for each backend
    for backend_name in &connected_backends {
        content = content.push(render_backend_scenarios(
            backend_name,
            scenario_manager,
            backend_manager,
            colors.clone(),
            zoom,
        ));
    }

    container(scrollable(content.spacing(scaled_spacing(16, zoom))))
        .padding(scaled_padding(12, zoom))
        .into()
}

/// Renders the no backends available message
fn render_no_backends<'a>(colors: ScenarioColorPalette, zoom: f32) -> Element<'a, TcGuiMessage> {
    container(
        column![
            text("⚠️ No Backends Connected")
                .size(scaled(20, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.warning_orange)
                }),
            text("Connect to a backend to manage scenarios")
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                })
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_x(iced::Alignment::Center),
    )
    .padding(scaled_padding(40, zoom))
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(colors.background_card)),
        border: iced::Border {
            radius: 8.0.into(),
            width: 1.0,
            color: colors.border_color,
        },
        ..container::Style::default()
    })
    .width(Length::Fill)
    .center_x(Length::Fill)
    .into()
}

/// Renders scenarios for a specific backend
fn render_backend_scenarios<'a>(
    backend_name: &str,
    scenario_manager: &'a ScenarioManager,
    _backend_manager: &'a BackendManager,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let mut backend_content = column![];
    let is_loading = scenario_manager.is_loading(backend_name);

    // Backend header with refresh button
    let refresh_button = if is_loading {
        button(text("⏳ Loading...").size(scaled(12, zoom))).style(button::secondary)
    } else {
        button(text("🔄 Refresh").size(scaled(12, zoom)))
            .on_press(TcGuiMessage::ListScenarios {
                backend_name: backend_name.to_string(),
            })
            .style(button::secondary)
    };

    backend_content = backend_content.push(
        row![
            text(format!("🖥️ Backend: {}", backend_name))
                .size(scaled(18, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_primary)
                }),
            space().width(Length::Fill),
            refresh_button
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center),
    );

    // Search and sort controls
    let raw_count = scenario_manager.get_raw_scenario_count(backend_name);
    if raw_count > 0 || !scenario_manager.get_search_filter().is_empty() {
        let current_sort = scenario_manager.get_sort_option();
        let sort_ascending = scenario_manager.is_sort_ascending();

        // Sort buttons
        let mut sort_buttons =
            row![text("Sort:")
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                })]
            .spacing(scaled_spacing(4, zoom))
            .align_y(iced::Alignment::Center);

        for option in ScenarioSortOption::all() {
            let is_active = current_sort == *option;
            let label = if is_active {
                let arrow = if sort_ascending { "↑" } else { "↓" };
                format!("{} {}", option.label(), arrow)
            } else {
                option.label().to_string()
            };

            let btn = button(text(label).size(scaled(11, zoom)))
                .on_press(TcGuiMessage::ScenarioSortOptionChanged(*option))
                .style(if is_active {
                    button::primary
                } else {
                    button::secondary
                });
            sort_buttons = sort_buttons.push(btn);
        }

        // Search input
        let search_filter = scenario_manager.get_search_filter().to_string();
        let search_input = text_input("Search scenarios...", &search_filter)
            .on_input(TcGuiMessage::ScenarioSearchFilterChanged)
            .size(scaled(13, zoom))
            .width(200);

        backend_content = backend_content.push(
            row![search_input, space().width(Length::Fill), sort_buttons]
                .spacing(scaled_spacing(12, zoom))
                .align_y(iced::Alignment::Center),
        );
    }

    // Available scenarios section
    let available_scenarios = scenario_manager.get_available_scenarios(backend_name);
    let raw_scenario_count = scenario_manager.get_raw_scenario_count(backend_name);

    if !available_scenarios.is_empty() {
        // Show count info if filtering
        let header_text = if !scenario_manager.get_search_filter().is_empty() {
            format!(
                "📋 Scenarios ({} of {})",
                available_scenarios.len(),
                raw_scenario_count
            )
        } else {
            format!("📋 Scenarios ({})", available_scenarios.len())
        };

        backend_content = backend_content.push(
            column![
                text(header_text)
                    .size(scaled(16, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                render_scenario_list(&available_scenarios, backend_name, colors.clone(), zoom)
            ]
            .spacing(scaled_spacing(8, zoom)),
        );
    }

    // Show load errors if any
    if let Some(load_errors) = scenario_manager.get_load_errors(backend_name) {
        if !load_errors.is_empty() {
            let mut error_items: Column<'_, TcGuiMessage> =
                column![].spacing(scaled_spacing(4, zoom));
            for load_error in load_errors {
                error_items = error_items.push(
                    container(
                        column![
                            text(format!("File: {}", load_error.file_path))
                                .size(scaled(11, zoom))
                                .style(move |_| text::Style {
                                    color: Some(colors.text_primary),
                                }),
                            text(format!("  {}", load_error.error.message))
                                .size(scaled(10, zoom))
                                .style(move |_| text::Style {
                                    color: Some(colors.error_red),
                                }),
                        ]
                        .spacing(scaled_spacing(2, zoom)),
                    )
                    .padding([scaled_padding(4, zoom), scaled_padding(8, zoom)])
                    .style(move |_| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgba(
                            0.9, 0.2, 0.2, 0.05,
                        ))),
                        border: iced::Border {
                            radius: 4.0.into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    }),
                );
            }

            backend_content = backend_content.push(
                column![
                    text(format!(
                        "⚠️ Failed to load {} scenario files",
                        load_errors.len()
                    ))
                    .size(scaled(14, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.warning_orange)
                    }),
                    error_items
                ]
                .spacing(scaled_spacing(6, zoom)),
            );
        }
    }

    // Active executions section
    let active_executions = scenario_manager.get_active_executions(backend_name);
    if !active_executions.is_empty() {
        backend_content = backend_content.push(
            column![
                text("🎮 Active Executions")
                    .size(scaled(16, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.success_green)
                    }),
                render_active_executions(
                    &active_executions,
                    backend_name,
                    scenario_manager,
                    colors.clone(),
                    zoom
                )
            ]
            .spacing(scaled_spacing(8, zoom)),
        );
    }

    // Show appropriate message when no scenarios
    if available_scenarios.is_empty() {
        let message = if is_loading {
            "Loading scenarios..."
        } else if !scenario_manager.get_search_filter().is_empty() && raw_scenario_count > 0 {
            "No scenarios match your search"
        } else {
            "Click 'Refresh' to load scenarios"
        };

        backend_content = backend_content.push(
            container(
                text(message)
                    .size(scaled(14, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary),
                    }),
            )
            .padding(scaled_padding(16, zoom))
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(colors.background_light)),
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: Color::from_rgb(0.9, 0.93, 0.98),
                },
                ..container::Style::default()
            }),
        );
    }

    container(backend_content.spacing(scaled_spacing(12, zoom)))
        .padding(scaled_padding(16, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: colors.border_color,
            },
            ..container::Style::default()
        })
        .into()
}

/// Renders a list of scenarios
fn render_scenario_list<'a>(
    scenarios: &[NetworkScenario],
    backend_name: &str,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let mut list_content = column![];

    for scenario in scenarios {
        list_content = list_content.push(render_scenario_card(
            scenario,
            backend_name,
            colors.clone(),
            zoom,
        ));
    }

    list_content.spacing(scaled_spacing(8, zoom)).into()
}

/// Renders a single scenario card
fn render_scenario_card<'a>(
    scenario: &NetworkScenario,
    backend_name: &str,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    container(
        column![
            row![
                text(format!("📋 {}", scenario.name))
                    .size(scaled(16, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                space().width(Length::Fill),
                button(text("▶️ Execute").size(scaled(12, zoom)))
                    .on_press(TcGuiMessage::ShowInterfaceSelectionDialog {
                        backend_name: backend_name.to_string(),
                        scenario_id: scenario.id.clone(),
                    })
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(colors.success_green)),
                        text_color: Color::WHITE,
                        ..button::Style::default()
                    }),
                button(text("👁 Details").size(scaled(12, zoom)))
                    .on_press(TcGuiMessage::ShowScenarioDetails {
                        scenario: scenario.clone()
                    })
                    .style(button::secondary),
            ]
            .spacing(scaled_spacing(8, zoom))
            .align_y(iced::Alignment::Center),
            row![
                text(format!("ID: {}", scenario.id))
                    .size(scaled(12, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                text("•")
                    .size(scaled(12, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                text(format!("{} steps", scenario.steps.len()))
                    .size(scaled(12, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                text("•")
                    .size(scaled(12, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                text(format!(
                    "~{:.1}s",
                    scenario.estimated_total_duration_ms() as f64 / 1000.0
                ))
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                })
            ]
            .spacing(scaled_spacing(4, zoom))
            .align_y(iced::Alignment::Center),
            if !scenario.description.is_empty() {
                Element::<'_, TcGuiMessage>::from(
                    text(scenario.description.clone())
                        .size(scaled(13, zoom))
                        .style(move |_: &iced::Theme| text::Style {
                            color: Some(colors.text_secondary),
                        }),
                )
            } else {
                space().height(0).into()
            }
        ]
        .spacing(scaled_spacing(6, zoom)),
    )
    .padding(scaled_padding(12, zoom))
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(colors.background_light)),
        border: iced::Border {
            radius: 6.0.into(),
            width: 1.0,
            color: Color::from_rgb(0.9, 0.93, 0.98),
        },
        ..container::Style::default()
    })
    .into()
}

/// Renders active scenario executions
fn render_active_executions<'a>(
    executions: &[ScenarioExecution],
    backend_name: &str,
    scenario_manager: &ScenarioManager,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let mut list_content = column![];

    for execution in executions {
        let is_collapsed = scenario_manager.is_timeline_collapsed(
            backend_name,
            &execution.target_namespace,
            &execution.target_interface,
        );
        list_content = list_content.push(render_execution_card(
            execution,
            backend_name,
            is_collapsed,
            colors.clone(),
            zoom,
        ));
    }

    list_content.spacing(scaled_spacing(8, zoom)).into()
}

/// Renders a single execution status card with enhanced progress UI
fn render_execution_card<'a>(
    execution: &ScenarioExecution,
    backend_name: &str,
    is_timeline_collapsed: bool,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let (state_icon, state_color) = match &execution.state {
        ExecutionState::Running => ("▶️", colors.success_green),
        ExecutionState::Paused { .. } => ("⏸️", colors.warning_orange),
        ExecutionState::Stopped => ("⏹️", colors.error_red),
        ExecutionState::Completed => ("✅", colors.primary_blue),
        ExecutionState::Failed { .. } => ("❌", colors.error_red),
    };

    let progress_text = format!(
        "Step {}/{} ({:.1}%)",
        execution.current_step + 1,
        execution.scenario.steps.len(),
        execution.stats.progress_percent
    );

    // Calculate estimated time remaining
    let time_remaining_text = if matches!(execution.state, ExecutionState::Running) {
        let remaining_duration: u64 = execution
            .scenario
            .steps
            .iter()
            .skip(execution.current_step)
            .map(|s| s.duration_ms)
            .sum();
        format!("~{} remaining", format_duration(remaining_duration))
    } else {
        match &execution.state {
            ExecutionState::Completed => "Completed".to_string(),
            ExecutionState::Stopped => "Stopped".to_string(),
            ExecutionState::Failed { .. } => "Failed".to_string(),
            ExecutionState::Paused { .. } => "Paused".to_string(),
            _ => String::new(),
        }
    };

    // Get current step details
    let current_step_text = execution
        .scenario
        .steps
        .get(execution.current_step)
        .map(|step| step.description.clone())
        .unwrap_or_else(|| "No step".to_string());

    // Progress bar colors
    let bar_color = match &execution.state {
        ExecutionState::Running => colors.success_green,
        ExecutionState::Paused { .. } => colors.warning_orange,
        ExecutionState::Completed => colors.primary_blue,
        ExecutionState::Stopped | ExecutionState::Failed { .. } => colors.error_red,
    };
    let bar_bg_color = Color::from_rgb(0.85, 0.88, 0.92);
    let progress_width = (execution.stats.progress_percent / 100.0).clamp(0.0, 1.0);

    // Build step timeline
    let mut timeline_content = column![].spacing(scaled_spacing(2, zoom));
    for (i, step) in execution.scenario.steps.iter().enumerate() {
        let (icon, step_color) = if i < execution.current_step {
            ("✓", colors.success_green)
        } else if i == execution.current_step {
            match &execution.state {
                ExecutionState::Running => ("▶", colors.primary_blue),
                ExecutionState::Paused { .. } => ("⏸", colors.warning_orange),
                ExecutionState::Stopped => ("⏹", colors.error_red),
                ExecutionState::Completed => ("✓", colors.success_green),
                ExecutionState::Failed { .. } => ("✗", colors.error_red),
            }
        } else {
            ("○", colors.text_secondary)
        };

        let is_current = i == execution.current_step;
        let step_bg = if is_current {
            Color::from_rgba(0.2, 0.6, 1.0, 0.1)
        } else {
            Color::TRANSPARENT
        };

        timeline_content = timeline_content.push(
            container(
                row![
                    container(
                        text(icon)
                            .size(scaled(11, zoom))
                            .style(move |_| text::Style {
                                color: Some(step_color)
                            })
                    )
                    .width(scaled(16, zoom)),
                    column![
                        text(format!("Step {}: {}", i + 1, step.description))
                            .size(scaled(11, zoom))
                            .style(move |_| text::Style {
                                color: Some(if is_current {
                                    colors.text_primary
                                } else {
                                    colors.text_secondary
                                })
                            }),
                        text(format_duration(step.duration_ms))
                            .size(scaled(10, zoom))
                            .style(move |_| text::Style {
                                color: Some(colors.text_secondary)
                            })
                    ]
                    .spacing(scaled_spacing(1, zoom))
                ]
                .spacing(scaled_spacing(6, zoom))
                .align_y(iced::Alignment::Center),
            )
            .padding([scaled_padding(3, zoom), scaled_padding(6, zoom)])
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(step_bg)),
                border: iced::Border {
                    radius: 3.0.into(),
                    ..iced::Border::default()
                },
                ..container::Style::default()
            }),
        );
    }

    // Build the main card content
    let mut card_content = column![].spacing(scaled_spacing(8, zoom));

    // Header row with name, loop indicator, and controls
    let loop_info = if execution.loop_execution {
        format!(" 🔁 Loop #{}", execution.loop_iteration + 1)
    } else {
        String::new()
    };

    card_content = card_content.push(
        row![
            text(format!(
                "{} {}{}",
                state_icon, execution.scenario.name, loop_info
            ))
            .size(scaled(16, zoom))
            .style(move |_| text::Style {
                color: Some(colors.text_primary)
            }),
            space().width(Length::Fill),
            render_execution_controls(execution, backend_name, colors.clone(), zoom)
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center),
    );

    // Target info and time remaining
    card_content = card_content.push(
        row![
            text(format!(
                "{}:{}",
                execution.target_namespace, execution.target_interface
            ))
            .size(scaled(12, zoom))
            .style(move |_| text::Style {
                color: Some(colors.text_secondary)
            }),
            text("•")
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
            text(progress_text.clone())
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(state_color)
                }),
            space().width(Length::Fill),
            text(time_remaining_text)
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(state_color)
                })
        ]
        .spacing(scaled_spacing(4, zoom))
        .align_y(iced::Alignment::Center),
    );

    // Visual progress bar
    card_content = card_content.push(
        container(
            container(space().width(Length::Fill).height(Length::Fill))
                .width(Length::FillPortion((progress_width * 100.0) as u16))
                .height(scaled(6, zoom))
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(bar_color)),
                    border: iced::Border {
                        radius: 3.0.into(),
                        ..iced::Border::default()
                    },
                    ..container::Style::default()
                }),
        )
        .width(Length::Fill)
        .height(scaled(6, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(bar_bg_color)),
            border: iced::Border {
                radius: 3.0.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        }),
    );

    // Current step details
    card_content = card_content.push(
        row![
            text("Current:")
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
            text(current_step_text)
                .size(scaled(11, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_primary)
                })
        ]
        .spacing(scaled_spacing(4, zoom)),
    );

    // Show error message if failed
    if let ExecutionState::Failed { error } = &execution.state {
        let mut error_content: Column<'_, TcGuiMessage> = column![text(format!(
            "Error [{}]: {}",
            error.category_str(),
            error.message
        ))
        .size(scaled(11, zoom))
        .style(move |_| text::Style {
            color: Some(colors.error_red),
        }),];

        // Show step info if available
        if let Some(step_idx) = error.step_index {
            error_content = error_content.push(
                text(format!("  At step {}", step_idx + 1))
                    .size(scaled(10, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.error_red),
                    }),
            );
        }

        // Show suggestion if available
        if let Some(suggestion) = &error.suggestion {
            error_content = error_content.push(
                text(format!("  Suggestion: {}", suggestion))
                    .size(scaled(10, zoom))
                    .style(move |_| text::Style {
                        color: Some(colors.warning_orange),
                    }),
            );
        }

        card_content = card_content.push(
            container(error_content)
                .padding([scaled_padding(4, zoom), scaled_padding(8, zoom)])
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(Color::from_rgba(
                        0.9, 0.2, 0.2, 0.1,
                    ))),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..iced::Border::default()
                    },
                    ..container::Style::default()
                }),
        );
    }

    // Divider
    card_content =
        card_content.push(container(space().width(Length::Fill).height(1)).style(|_| {
            container::Style {
                background: Some(iced::Background::Color(Color::from_rgb(0.88, 0.90, 0.94))),
                ..container::Style::default()
            }
        }));

    // Collapsible step timeline header
    let toggle_icon = if is_timeline_collapsed { "▶" } else { "▼" };
    let step_count = execution.scenario.steps.len();
    let timeline_header_text = if is_timeline_collapsed {
        format!("{} Steps Timeline ({} steps)", toggle_icon, step_count)
    } else {
        format!("{} Steps Timeline", toggle_icon)
    };

    let backend_name_owned = backend_name.to_string();
    let namespace_owned = execution.target_namespace.clone();
    let interface_owned = execution.target_interface.clone();

    card_content = card_content.push(
        button(
            text(timeline_header_text)
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary),
                }),
        )
        .padding([scaled_padding(4, zoom), 0.0])
        .style(move |_, _| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            text_color: colors.text_secondary,
            ..button::Style::default()
        })
        .on_press(TcGuiMessage::ToggleExecutionTimeline {
            backend_name: backend_name_owned,
            namespace: namespace_owned,
            interface: interface_owned,
        }),
    );

    // Step timeline (only show if not collapsed)
    if !is_timeline_collapsed {
        card_content = card_content.push(timeline_content);
    }

    container(card_content)
        .padding(scaled_padding(12, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_light)),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.9, 0.93, 0.98),
            },
            ..container::Style::default()
        })
        .into()
}

/// Renders execution control buttons
fn render_execution_controls<'a>(
    execution: &ScenarioExecution,
    backend_name: &str,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    match &execution.state {
        ExecutionState::Running => row![
            button(text("⏸️").size(scaled(14, zoom)))
                .on_press(TcGuiMessage::PauseScenarioExecution {
                    backend_name: backend_name.to_string(),
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                })
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.warning_orange)),
                    text_color: Color::WHITE,
                    ..button::Style::default()
                }),
            button(text("⏹️").size(scaled(14, zoom)))
                .on_press(TcGuiMessage::StopScenarioExecution {
                    backend_name: backend_name.to_string(),
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                })
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.error_red)),
                    text_color: Color::WHITE,
                    ..button::Style::default()
                })
        ]
        .spacing(scaled_spacing(4, zoom))
        .into(),
        ExecutionState::Paused { .. } => row![
            button(text("▶️").size(scaled(14, zoom)))
                .on_press(TcGuiMessage::ResumeScenarioExecution {
                    backend_name: backend_name.to_string(),
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                })
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.success_green)),
                    text_color: Color::WHITE,
                    ..button::Style::default()
                }),
            button(text("⏹️").size(scaled(14, zoom)))
                .on_press(TcGuiMessage::StopScenarioExecution {
                    backend_name: backend_name.to_string(),
                    namespace: execution.target_namespace.clone(),
                    interface: execution.target_interface.clone(),
                })
                .style(move |_, _| button::Style {
                    background: Some(iced::Background::Color(colors.error_red)),
                    text_color: Color::WHITE,
                    ..button::Style::default()
                })
        ]
        .spacing(scaled_spacing(4, zoom))
        .into(),
        ExecutionState::Stopped | ExecutionState::Completed | ExecutionState::Failed { .. } => {
            text("Finished")
                .size(scaled(12, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary),
                })
                .into()
        }
    }
}

/// Renders detailed scenario information
fn render_scenario_details<'a>(
    scenario: &NetworkScenario,
    colors: ScenarioColorPalette,
    zoom: f32,
) -> Element<'a, TcGuiMessage> {
    let mut details_content = column![];

    // Header with close button
    details_content = details_content.push(
        row![
            text(format!("📋 Scenario Details: {}", scenario.name))
                .size(scaled(20, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_primary)
                }),
            space().width(Length::Fill),
            button(text("✕ Close").size(scaled(12, zoom)))
                .on_press(TcGuiMessage::HideScenarioDetails)
                .style(button::secondary)
        ]
        .spacing(scaled_spacing(8, zoom))
        .align_y(iced::Alignment::Center),
    );

    // Basic info
    details_content = details_content.push(
        column![
            text(format!("ID: {}", scenario.id))
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
            if !scenario.description.is_empty() {
                Element::<'_, TcGuiMessage>::from(
                    text(scenario.description.clone())
                        .size(scaled(14, zoom))
                        .style(move |_: &iced::Theme| text::Style {
                            color: Some(colors.text_primary),
                        }),
                )
            } else {
                space().height(0).into()
            },
            text(format!(
                "Steps: {} | Duration: ~{:.1}s",
                scenario.steps.len(),
                scenario.estimated_total_duration_ms() as f64 / 1000.0
            ))
            .size(scaled(13, zoom))
            .style(move |_| text::Style {
                color: Some(colors.text_secondary)
            })
        ]
        .spacing(scaled_spacing(6, zoom)),
    );

    // Steps breakdown
    if !scenario.steps.is_empty() {
        let mut steps_content = column![];
        steps_content = steps_content.push(text("Steps:").size(scaled(16, zoom)).style(
            move |_| text::Style {
                color: Some(colors.text_primary),
            },
        ));

        for (i, step) in scenario.steps.iter().enumerate() {
            let timing_info = format_duration(step.duration_ms);

            steps_content = steps_content.push(
                container(
                    column![
                        text(format!("{}. {}", i + 1, step.description))
                            .size(scaled(14, zoom))
                            .style(move |_| text::Style {
                                color: Some(colors.text_primary)
                            }),
                        text(format!("Timing: {}", timing_info))
                            .size(scaled(12, zoom))
                            .style(move |_| text::Style {
                                color: Some(colors.text_secondary)
                            })
                    ]
                    .spacing(scaled_spacing(2, zoom)),
                )
                .padding([scaled_padding(8, zoom), scaled_padding(12, zoom)])
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(colors.background_light)),
                    border: iced::Border {
                        radius: 4.0.into(),
                        width: 1.0,
                        color: Color::from_rgb(0.9, 0.93, 0.98),
                    },
                    ..container::Style::default()
                }),
            );
        }

        details_content = details_content.push(steps_content.spacing(scaled_spacing(6, zoom)));
    }

    container(details_content.spacing(scaled_spacing(16, zoom)))
        .padding(scaled_padding(16, zoom))
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 2.0,
                color: colors.primary_blue,
            },
            ..container::Style::default()
        })
        .into()
}
