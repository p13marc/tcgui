//! Scenario view components for TC GUI frontend.
//!
//! This module provides UI components for displaying and managing scenarios,
//! including scenario lists, details view, and execution controls.

use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Color, Element, Length};

use tcgui_shared::scenario::{ExecutionState, NetworkScenario, ScenarioExecution};

use crate::backend_manager::BackendManager;
use crate::messages::TcGuiMessage;
use crate::scenario_manager::ScenarioManager;

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
        }
    }
}

/// Renders the main scenario management view
pub fn render_scenario_view<'a>(
    backend_manager: &'a BackendManager,
    scenario_manager: &'a ScenarioManager,
) -> Element<'a, TcGuiMessage> {
    let colors = ScenarioColorPalette::default();

    // Check if we have connected backends
    let connected_backends: Vec<String> = backend_manager.connected_backend_names();

    if connected_backends.is_empty() {
        return render_no_backends(colors);
    }

    let mut content = column![];

    // Header
    content = content.push(
        container(
            column![
                text("📊 Network Scenarios")
                    .size(24)
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                text("Manage and execute network condition scenarios")
                    .size(14)
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    })
            ]
            .spacing(4),
        )
        .padding(16)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.92, 0.98),
            },
            ..container::Style::default()
        }),
    );

    // Show scenario details if selected
    if scenario_manager.is_showing_details() {
        if let Some(scenario) = scenario_manager.get_selected_scenario() {
            content = content.push(render_scenario_details(scenario, colors.clone()));
        }
    }

    // Scenario sections for each backend
    for backend_name in &connected_backends {
        content = content.push(render_backend_scenarios(
            backend_name,
            scenario_manager,
            backend_manager,
            colors.clone(),
        ));
    }

    container(scrollable(content.spacing(16)))
        .padding(12)
        .into()
}

/// Renders the no backends available message
fn render_no_backends<'a>(colors: ScenarioColorPalette) -> Element<'a, TcGuiMessage> {
    container(
        column![
            text("⚠️ No Backends Connected")
                .size(20)
                .style(move |_| text::Style {
                    color: Some(colors.warning_orange)
                }),
            text("Connect to a backend to manage scenarios")
                .size(14)
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                })
        ]
        .spacing(8)
        .align_x(iced::Alignment::Center),
    )
    .padding(40)
    .style(move |_| container::Style {
        background: Some(iced::Background::Color(colors.background_card)),
        border: iced::Border {
            radius: 8.0.into(),
            width: 1.0,
            color: Color::from_rgb(0.88, 0.92, 0.98),
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
) -> Element<'a, TcGuiMessage> {
    let mut backend_content = column![];

    // Backend header
    backend_content = backend_content.push(
        row![
            text(format!("🖥️ Backend: {}", backend_name))
                .size(18)
                .style(move |_| text::Style {
                    color: Some(colors.text_primary)
                }),
            space().width(Length::Fill),
            button(text("🔄 Refresh Scenarios").size(12))
                .on_press(TcGuiMessage::ListScenarios {
                    backend_name: backend_name.to_string()
                })
                .style(button::secondary)
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    // Available scenarios section
    let available_scenarios = scenario_manager.get_available_scenarios(backend_name);
    if !available_scenarios.is_empty() {
        backend_content = backend_content.push(
            column![
                text("📋 Scenarios").size(16).style(move |_| text::Style {
                    color: Some(colors.text_primary)
                }),
                render_scenario_list(&available_scenarios, backend_name, colors.clone())
            ]
            .spacing(8),
        );
    }

    // Active executions section
    let active_executions = scenario_manager.get_active_executions(backend_name);
    if !active_executions.is_empty() {
        backend_content = backend_content.push(
            column![
                text("🎮 Active Executions")
                    .size(16)
                    .style(move |_| text::Style {
                        color: Some(colors.success_green)
                    }),
                render_active_executions(&active_executions, backend_name, colors.clone())
            ]
            .spacing(8),
        );
    }

    // If no scenarios available, show loading message
    if available_scenarios.is_empty() {
        backend_content = backend_content.push(
            container(
                text("Click 'Refresh Scenarios' to load scenarios")
                    .size(14)
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary),
                    }),
            )
            .padding(16)
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

    container(backend_content.spacing(12))
        .padding(16)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(colors.background_card)),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: Color::from_rgb(0.88, 0.92, 0.98),
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
) -> Element<'a, TcGuiMessage> {
    let mut list_content = column![];

    for scenario in scenarios {
        list_content =
            list_content.push(render_scenario_card(scenario, backend_name, colors.clone()));
    }

    list_content.spacing(8).into()
}

/// Renders a single scenario card
fn render_scenario_card<'a>(
    scenario: &NetworkScenario,
    backend_name: &str,
    colors: ScenarioColorPalette,
) -> Element<'a, TcGuiMessage> {
    container(
        column![
            row![
                text(format!("📋 {}", scenario.name))
                    .size(16)
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                space().width(Length::Fill),
                button(text("▶️ Execute").size(12))
                    .on_press(TcGuiMessage::ShowInterfaceSelectionDialog {
                        backend_name: backend_name.to_string(),
                        scenario_id: scenario.id.clone(),
                    })
                    .style(move |_, _| button::Style {
                        background: Some(iced::Background::Color(colors.success_green)),
                        text_color: Color::WHITE,
                        ..button::Style::default()
                    }),
                button(text("👁 Details").size(12))
                    .on_press(TcGuiMessage::ShowScenarioDetails {
                        scenario: scenario.clone()
                    })
                    .style(button::secondary),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![
                text(format!("ID: {}", scenario.id))
                    .size(12)
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                text("•").size(12).style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
                text(format!("{} steps", scenario.steps.len()))
                    .size(12)
                    .style(move |_| text::Style {
                        color: Some(colors.text_secondary)
                    }),
                text("•").size(12).style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
                text(format!(
                    "~{:.1}s",
                    scenario.estimated_total_duration_ms() as f64 / 1000.0
                ))
                .size(12)
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                })
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center),
            if !scenario.description.is_empty() {
                Element::<'_, TcGuiMessage>::from(
                    text(scenario.description.clone())
                        .size(13)
                        .style(move |_: &iced::Theme| text::Style {
                            color: Some(colors.text_secondary),
                        }),
                )
            } else {
                space().height(0).into()
            }
        ]
        .spacing(6),
    )
    .padding(12)
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
    colors: ScenarioColorPalette,
) -> Element<'a, TcGuiMessage> {
    let mut list_content = column![];

    for execution in executions {
        list_content = list_content.push(render_execution_card(
            execution,
            backend_name,
            colors.clone(),
        ));
    }

    list_content.spacing(8).into()
}

/// Renders a single execution status card
fn render_execution_card<'a>(
    execution: &ScenarioExecution,
    backend_name: &str,
    colors: ScenarioColorPalette,
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

    container(
        column![
            row![
                text(format!("{} {}", state_icon, execution.scenario.name))
                    .size(16)
                    .style(move |_| text::Style {
                        color: Some(colors.text_primary)
                    }),
                space().width(Length::Fill),
                render_execution_controls(execution, backend_name, colors.clone())
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
            row![
                text(format!(
                    "{}:{}",
                    execution.target_namespace, execution.target_interface
                ))
                .size(12)
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
                text("•").size(12).style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
                text(progress_text.clone())
                    .size(12)
                    .style(move |_| text::Style {
                        color: Some(state_color)
                    }),
                space().width(Length::Fill),
                text(format!("{:?}", execution.state))
                    .size(11)
                    .style(move |_| text::Style {
                        color: Some(state_color)
                    })
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        ]
        .spacing(6),
    )
    .padding(12)
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
) -> Element<'a, TcGuiMessage> {
    match &execution.state {
        ExecutionState::Running => row![
            button(text("⏸️").size(14))
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
            button(text("⏹️").size(14))
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
        .spacing(4)
        .into(),
        ExecutionState::Paused { .. } => row![
            button(text("▶️").size(14))
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
            button(text("⏹️").size(14))
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
        .spacing(4)
        .into(),
        ExecutionState::Stopped | ExecutionState::Completed | ExecutionState::Failed { .. } => {
            text("Finished")
                .size(12)
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
) -> Element<'a, TcGuiMessage> {
    let mut details_content = column![];

    // Header with close button
    details_content = details_content.push(
        row![
            text(format!("📋 Scenario Details: {}", scenario.name))
                .size(20)
                .style(move |_| text::Style {
                    color: Some(colors.text_primary)
                }),
            space().width(Length::Fill),
            button(text("✕ Close").size(12))
                .on_press(TcGuiMessage::HideScenarioDetails)
                .style(button::secondary)
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    );

    // Basic info
    details_content = details_content.push(
        column![
            text(format!("ID: {}", scenario.id))
                .size(14)
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary)
                }),
            if !scenario.description.is_empty() {
                Element::<'_, TcGuiMessage>::from(
                    text(scenario.description.clone())
                        .size(14)
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
            .size(13)
            .style(move |_| text::Style {
                color: Some(colors.text_secondary)
            })
        ]
        .spacing(6),
    );

    // Steps breakdown
    if !scenario.steps.is_empty() {
        let mut steps_content = column![];
        steps_content = steps_content.push(text("Steps:").size(16).style(move |_| text::Style {
            color: Some(colors.text_primary),
        }));

        for (i, step) in scenario.steps.iter().enumerate() {
            let timing_info = format_duration(step.duration_ms);

            steps_content = steps_content.push(
                container(
                    column![
                        text(format!("{}. {}", i + 1, step.description))
                            .size(14)
                            .style(move |_| text::Style {
                                color: Some(colors.text_primary)
                            }),
                        text(format!("Timing: {}", timing_info))
                            .size(12)
                            .style(move |_| text::Style {
                                color: Some(colors.text_secondary)
                            })
                    ]
                    .spacing(2),
                )
                .padding([8, 12])
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

        details_content = details_content.push(steps_content.spacing(6));
    }

    container(details_content.spacing(16))
        .padding(16)
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
