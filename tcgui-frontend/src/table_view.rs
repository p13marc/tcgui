//! Table view module for TC GUI frontend.
//!
//! This module provides a compact table view of network interfaces as an alternative
//! to the card-based view. It uses the Iced 0.14 table widget for efficient rendering
//! of interface data in a grid format.

use crate::backend_manager::BackendManager;
use crate::messages::TcGuiMessage;
use crate::theme::Theme;
use crate::view::{ColorPalette, scaled, scaled_spacing};
use iced::widget::{container, scrollable, table, text};
use iced::{Color, Element, Length};

/// Data structure representing a row in the interface table
#[derive(Clone)]
pub struct InterfaceTableRow {
    /// Backend name
    pub backend_name: String,
    /// Namespace name
    pub namespace: String,
    /// Interface name
    pub interface_name: String,
    /// Whether the interface is up
    pub is_up: bool,
    /// Whether TC qdisc is configured
    pub has_tc: bool,
    /// Receive bandwidth rate (bytes/sec)
    pub rx_rate: Option<f64>,
    /// Transmit bandwidth rate (bytes/sec)
    pub tx_rate: Option<f64>,
}

impl InterfaceTableRow {
    /// Format bandwidth rate for display
    fn format_rate(rate: Option<f64>) -> String {
        match rate {
            Some(bytes_per_sec) => {
                if bytes_per_sec >= 1_000_000_000.0 {
                    format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
                } else if bytes_per_sec >= 1_000_000.0 {
                    format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
                } else if bytes_per_sec >= 1_000.0 {
                    format!("{:.1} KB/s", bytes_per_sec / 1_000.0)
                } else {
                    format!("{:.0} B/s", bytes_per_sec)
                }
            }
            None => "-".to_string(),
        }
    }
}

/// Collect all interfaces from the backend manager into table rows
pub fn collect_interface_rows(backend_manager: &BackendManager) -> Vec<InterfaceTableRow> {
    let mut rows = Vec::new();

    for (backend_name, backend_group) in backend_manager.backends() {
        if !backend_group.is_connected {
            continue;
        }

        for (namespace_name, namespace_group) in &backend_group.namespaces {
            for (interface_name, tc_interface) in &namespace_group.tc_interfaces {
                let bandwidth = tc_interface.bandwidth_stats();

                rows.push(InterfaceTableRow {
                    backend_name: backend_name.clone(),
                    namespace: namespace_name.clone(),
                    interface_name: interface_name.clone(),
                    is_up: tc_interface.is_up(),
                    has_tc: tc_interface.has_tc_qdisc(),
                    rx_rate: bandwidth.map(|b| b.rx_bytes_per_sec),
                    tx_rate: bandwidth.map(|b| b.tx_bytes_per_sec),
                });
            }
        }
    }

    // Sort by backend, namespace, then interface name
    rows.sort_by(|a, b| {
        a.backend_name
            .cmp(&b.backend_name)
            .then(a.namespace.cmp(&b.namespace))
            .then(a.interface_name.cmp(&b.interface_name))
    });

    rows
}

// Helper functions to create styled text cells
fn text_cell(content: String, size: f32, color: Color) -> Element<'static, TcGuiMessage> {
    text(content)
        .size(size)
        .style(move |_| text::Style { color: Some(color) })
        .into()
}

fn header_cell(content: &'static str, size: f32, color: Color) -> Element<'static, TcGuiMessage> {
    text(content)
        .size(size)
        .style(move |_| text::Style { color: Some(color) })
        .into()
}

/// Render the interface table view
pub fn render_interface_table(
    backend_manager: &BackendManager,
    theme: &Theme,
    zoom: f32,
) -> Element<'static, TcGuiMessage> {
    let rows = collect_interface_rows(backend_manager);
    let colors = ColorPalette::from_theme(theme);

    if rows.is_empty() {
        return container(
            text("No interfaces available")
                .size(scaled(14, zoom))
                .style(move |_| text::Style {
                    color: Some(colors.text_secondary),
                }),
        )
        .padding(scaled_spacing(20, zoom))
        .center_x(Length::Fill)
        .into();
    }

    let text_size = scaled(12, zoom);
    let small_text_size = scaled(11, zoom);
    let primary = colors.text_primary;
    let secondary = colors.text_secondary;
    let success = colors.success_green;
    let warning = colors.warning_orange;
    let rx_color = theme.colors.rx_color;
    let tx_color = theme.colors.tx_color;

    // Define table columns with static headers
    let interface_col = table::column(
        header_cell("Interface", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            text_cell(row.interface_name, text_size, primary)
        },
    )
    .width(Length::Fixed(120.0 * zoom));

    let namespace_col = table::column(
        header_cell("Namespace", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            text_cell(row.namespace, text_size, secondary)
        },
    )
    .width(Length::Fixed(120.0 * zoom));

    let status_col = table::column(
        header_cell("Status", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            let (status_text, status_color) = if row.is_up {
                ("UP", success)
            } else {
                ("DOWN", secondary)
            };
            text_cell(status_text.to_string(), small_text_size, status_color)
        },
    )
    .width(Length::Fixed(60.0 * zoom))
    .align_x(iced::alignment::Horizontal::Center);

    let tc_col = table::column(
        header_cell("TC", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            let (tc_text, tc_color) = if row.has_tc {
                ("Active", warning)
            } else {
                ("-", secondary)
            };
            text_cell(tc_text.to_string(), small_text_size, tc_color)
        },
    )
    .width(Length::Fixed(60.0 * zoom))
    .align_x(iced::alignment::Horizontal::Center);

    let rx_col = table::column(
        header_cell("RX", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            let rx_text = InterfaceTableRow::format_rate(row.rx_rate);
            text_cell(rx_text, small_text_size, rx_color)
        },
    )
    .width(Length::Fixed(90.0 * zoom))
    .align_x(iced::alignment::Horizontal::Right);

    let tx_col = table::column(
        header_cell("TX", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            let tx_text = InterfaceTableRow::format_rate(row.tx_rate);
            text_cell(tx_text, small_text_size, tx_color)
        },
    )
    .width(Length::Fixed(90.0 * zoom))
    .align_x(iced::alignment::Horizontal::Right);

    let backend_col = table::column(
        header_cell("Backend", text_size, primary),
        move |row: InterfaceTableRow| -> Element<'static, TcGuiMessage> {
            text_cell(row.backend_name, small_text_size, secondary)
        },
    )
    .width(Length::Fixed(100.0 * zoom));

    // Build the table
    let interface_table = table(
        [
            interface_col,
            namespace_col,
            status_col,
            tc_col,
            rx_col,
            tx_col,
            backend_col,
        ],
        rows,
    )
    .padding_x(scaled_spacing(8, zoom))
    .padding_y(scaled_spacing(4, zoom))
    .separator_x(1.0)
    .separator_y(1.0)
    .width(Length::Fill);

    // Wrap in scrollable - use default style since smart_scrollbar_style requires lifetime
    scrollable(interface_table)
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_rate_bytes() {
        assert_eq!(InterfaceTableRow::format_rate(Some(500.0)), "500 B/s");
    }

    #[test]
    fn test_format_rate_kilobytes() {
        assert_eq!(InterfaceTableRow::format_rate(Some(1500.0)), "1.5 KB/s");
    }

    #[test]
    fn test_format_rate_megabytes() {
        assert_eq!(
            InterfaceTableRow::format_rate(Some(1_500_000.0)),
            "1.5 MB/s"
        );
    }

    #[test]
    fn test_format_rate_gigabytes() {
        assert_eq!(
            InterfaceTableRow::format_rate(Some(1_500_000_000.0)),
            "1.5 GB/s"
        );
    }

    #[test]
    fn test_format_rate_none() {
        assert_eq!(InterfaceTableRow::format_rate(None), "-");
    }
}
