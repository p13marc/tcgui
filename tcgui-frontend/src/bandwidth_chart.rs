//! Bandwidth chart widget for visualizing network throughput over time.
//!
//! This module provides a Canvas-based line chart for displaying
//! historical RX/TX bandwidth data.

use std::time::{Duration, Instant};

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};

use crate::bandwidth_history::BandwidthHistory;

/// Time window options for the chart display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartTimeWindow {
    #[default]
    OneMinute,
    FiveMinutes,
}

impl ChartTimeWindow {
    /// Get the duration for this time window.
    pub fn duration(&self) -> Duration {
        match self {
            Self::OneMinute => Duration::from_secs(60),
            Self::FiveMinutes => Duration::from_secs(300),
        }
    }

    /// Get a display label for this time window.
    pub fn label(&self) -> &'static str {
        match self {
            Self::OneMinute => "1m",
            Self::FiveMinutes => "5m",
        }
    }
}

/// Chart colors for RX and TX lines.
#[derive(Debug, Clone, Copy)]
pub struct ChartColors {
    pub rx: Color,
    pub tx: Color,
    pub grid: Color,
    pub axis: Color,
    pub text: Color,
    pub background: Color,
}

impl Default for ChartColors {
    fn default() -> Self {
        Self {
            rx: Color::from_rgb(0.0, 0.6, 0.9),      // Blue for RX (download)
            tx: Color::from_rgb(0.9, 0.5, 0.0),      // Orange for TX (upload)
            grid: Color::from_rgba(0.5, 0.5, 0.5, 0.3),
            axis: Color::from_rgb(0.4, 0.4, 0.4),
            text: Color::from_rgb(0.5, 0.5, 0.5),
            background: Color::from_rgba(0.95, 0.95, 0.95, 0.5),
        }
    }
}

impl ChartColors {
    /// Create colors for dark theme.
    pub fn dark() -> Self {
        Self {
            rx: Color::from_rgb(0.3, 0.7, 1.0),      // Lighter blue for dark mode
            tx: Color::from_rgb(1.0, 0.6, 0.2),      // Lighter orange for dark mode
            grid: Color::from_rgba(0.6, 0.6, 0.6, 0.2),
            axis: Color::from_rgb(0.7, 0.7, 0.7),
            text: Color::from_rgb(0.7, 0.7, 0.7),
            background: Color::from_rgba(0.15, 0.15, 0.15, 0.5),
        }
    }
}

/// Bandwidth chart widget state.
#[derive(Debug)]
pub struct BandwidthChart {
    time_window: ChartTimeWindow,
    colors: ChartColors,
    cache: canvas::Cache,
}

impl Default for BandwidthChart {
    fn default() -> Self {
        Self::new()
    }
}

impl BandwidthChart {
    /// Create a new bandwidth chart.
    pub fn new() -> Self {
        Self {
            time_window: ChartTimeWindow::default(),
            colors: ChartColors::default(),
            cache: canvas::Cache::default(),
        }
    }

    /// Set the time window for the chart.
    pub fn set_time_window(&mut self, window: ChartTimeWindow) {
        if self.time_window != window {
            self.time_window = window;
            self.cache.clear();
        }
    }

    /// Set chart colors (for theme switching).
    pub fn set_colors(&mut self, colors: ChartColors) {
        self.colors = colors;
        self.cache.clear();
    }

    /// Clear the rendering cache (call when data changes).
    pub fn invalidate(&mut self) {
        self.cache.clear();
    }

    /// Create the chart view element.
    pub fn view<'a, Message: 'a>(
        &'a self,
        history: Option<&'a BandwidthHistory>,
        height: f32,
    ) -> Element<'a, Message, Theme, Renderer> {
        canvas(BandwidthChartProgram {
            history,
            time_window: self.time_window,
            colors: self.colors,
            cache: &self.cache,
        })
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .into()
    }
}

/// Helper function to create a canvas widget.
fn canvas<P, Message>(
    program: P,
) -> iced::widget::Canvas<P, Message, Theme, Renderer>
where
    P: canvas::Program<Message, Theme, Renderer>,
{
    iced::widget::Canvas::new(program)
}

/// Create a bandwidth chart element without needing persistent state.
///
/// This is a convenience function for rendering a chart inline without
/// storing a BandwidthChart instance.
pub fn bandwidth_chart_view<'a, Message: 'a>(
    history: Option<&'a BandwidthHistory>,
    height: f32,
    dark_mode: bool,
) -> Element<'a, Message, Theme, Renderer> {
    let colors = if dark_mode {
        ChartColors::dark()
    } else {
        ChartColors::default()
    };

    canvas(StatelessBandwidthChart {
        history,
        time_window: ChartTimeWindow::default(),
        colors,
    })
    .width(Length::Fill)
    .height(Length::Fixed(height))
    .into()
}

/// A stateless bandwidth chart program that doesn't require a cache reference.
///
/// This version recreates geometry each frame but is simpler to use
/// in contexts where lifetime management is complex.
struct StatelessBandwidthChart<'a> {
    history: Option<&'a BandwidthHistory>,
    time_window: ChartTimeWindow,
    colors: ChartColors,
}

impl<Message> canvas::Program<Message, Theme, Renderer> for StatelessBandwidthChart<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // Draw fresh each frame for real-time updates (no caching)
        let mut frame = Frame::new(renderer, bounds.size());
        self.draw_chart(&mut frame, bounds.size());
        vec![frame.into_geometry()]
    }
}

impl StatelessBandwidthChart<'_> {
    /// Draw the complete chart.
    fn draw_chart(&self, frame: &mut Frame, size: Size) {
        let padding = ChartPadding {
            left: 45.0,
            right: 10.0,
            top: 10.0,
            bottom: 20.0,
        };

        let chart_width = size.width - padding.left - padding.right;
        let chart_height = size.height - padding.top - padding.bottom;

        if chart_width <= 0.0 || chart_height <= 0.0 {
            return;
        }

        // Draw background
        frame.fill_rectangle(
            Point::new(padding.left, padding.top),
            Size::new(chart_width, chart_height),
            self.colors.background,
        );

        // Get samples and calculate scale
        let samples: Vec<_> = self
            .history
            .map(|h| h.samples_in_window(self.time_window.duration()).collect())
            .unwrap_or_default();

        if samples.len() < 2 {
            self.draw_no_data(frame, size);
            return;
        }

        // Calculate max value for Y-axis scaling
        let max_value = samples
            .iter()
            .map(|s| s.rx_bytes_per_sec.max(s.tx_bytes_per_sec))
            .fold(0.0_f64, |a, b| a.max(b))
            .max(1024.0); // Minimum 1 KB/s scale

        let now = Instant::now();
        let window_duration = self.time_window.duration();

        // Draw grid lines
        self.draw_grid(frame, &padding, chart_width, chart_height, max_value);

        // Draw RX line (download - blue)
        self.draw_data_line(
            frame,
            &samples,
            now,
            window_duration,
            &padding,
            chart_width,
            chart_height,
            max_value,
            |s| s.rx_bytes_per_sec,
            self.colors.rx,
        );

        // Draw TX line (upload - orange)
        self.draw_data_line(
            frame,
            &samples,
            now,
            window_duration,
            &padding,
            chart_width,
            chart_height,
            max_value,
            |s| s.tx_bytes_per_sec,
            self.colors.tx,
        );

        // Draw axes
        self.draw_axes(frame, &padding, chart_width, chart_height);

        // Draw legend
        self.draw_legend(frame, size);
    }

    /// Draw a data line on the chart.
    #[allow(clippy::too_many_arguments)]
    fn draw_data_line<F>(
        &self,
        frame: &mut Frame,
        samples: &[&crate::bandwidth_history::BandwidthSample],
        now: Instant,
        window_duration: Duration,
        padding: &ChartPadding,
        chart_width: f32,
        chart_height: f32,
        max_value: f64,
        value_fn: F,
        color: Color,
    ) where
        F: Fn(&crate::bandwidth_history::BandwidthSample) -> f64,
    {
        let path = Path::new(|builder| {
            let mut first = true;
            for sample in samples {
                let age = now.duration_since(sample.timestamp);
                let x = padding.left
                    + chart_width * (1.0 - age.as_secs_f32() / window_duration.as_secs_f32());
                let y = padding.top
                    + chart_height * (1.0 - value_fn(sample) as f32 / max_value as f32);

                // Clamp to chart bounds
                let x = x.clamp(padding.left, padding.left + chart_width);
                let y = y.clamp(padding.top, padding.top + chart_height);

                if first {
                    builder.move_to(Point::new(x, y));
                    first = false;
                } else {
                    builder.line_to(Point::new(x, y));
                }
            }
        });

        frame.stroke(&path, Stroke::default().with_width(2.0).with_color(color));
    }

    /// Draw grid lines and axis labels.
    fn draw_grid(
        &self,
        frame: &mut Frame,
        padding: &ChartPadding,
        width: f32,
        height: f32,
        max_value: f64,
    ) {
        // Horizontal grid lines (4 divisions)
        for i in 0..=4 {
            let y = padding.top + height * (i as f32 / 4.0);
            let path = Path::line(
                Point::new(padding.left, y),
                Point::new(padding.left + width, y),
            );
            frame.stroke(
                &path,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(self.colors.grid),
            );

            // Y-axis value label
            let value = max_value * ((4 - i) as f64 / 4.0);
            frame.fill_text(Text {
                content: format_rate(value),
                position: Point::new(padding.left - 5.0, y),
                color: self.colors.text,
                size: 10.0.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        }

        // Time axis labels
        let time_labels: Vec<(f32, String)> = match self.time_window {
            ChartTimeWindow::OneMinute => vec![
                (1.0, "now".to_string()),
                (0.5, "30s".to_string()),
                (0.0, "1m".to_string()),
            ],
            ChartTimeWindow::FiveMinutes => vec![
                (1.0, "now".to_string()),
                (0.6, "2m".to_string()),
                (0.2, "4m".to_string()),
                (0.0, "5m".to_string()),
            ],
        };

        for (pos, label) in time_labels {
            let x = padding.left + width * pos;
            frame.fill_text(Text {
                content: label,
                position: Point::new(x, padding.top + height + 12.0),
                color: self.colors.text,
                size: 9.0.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Top,
                ..Default::default()
            });
        }
    }

    /// Draw X and Y axes.
    fn draw_axes(&self, frame: &mut Frame, padding: &ChartPadding, width: f32, height: f32) {
        // Y-axis
        let y_axis = Path::line(
            Point::new(padding.left, padding.top),
            Point::new(padding.left, padding.top + height),
        );
        frame.stroke(
            &y_axis,
            Stroke::default()
                .with_width(1.0)
                .with_color(self.colors.axis),
        );

        // X-axis
        let x_axis = Path::line(
            Point::new(padding.left, padding.top + height),
            Point::new(padding.left + width, padding.top + height),
        );
        frame.stroke(
            &x_axis,
            Stroke::default()
                .with_width(1.0)
                .with_color(self.colors.axis),
        );
    }

    /// Draw the legend.
    fn draw_legend(&self, frame: &mut Frame, size: Size) {
        let y = size.height - 8.0;
        let box_size = Size::new(10.0, 6.0);

        // RX legend
        frame.fill_rectangle(Point::new(50.0, y - 3.0), box_size, self.colors.rx);
        frame.fill_text(Text {
            content: "RX".to_string(),
            position: Point::new(63.0, y),
            color: self.colors.text,
            size: 9.0.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });

        // TX legend
        frame.fill_rectangle(Point::new(85.0, y - 3.0), box_size, self.colors.tx);
        frame.fill_text(Text {
            content: "TX".to_string(),
            position: Point::new(98.0, y),
            color: self.colors.text,
            size: 9.0.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });
    }

    /// Draw "No data" message when there's insufficient data.
    fn draw_no_data(&self, frame: &mut Frame, size: Size) {
        frame.fill_text(Text {
            content: "Collecting data...".to_string(),
            position: Point::new(size.width / 2.0, size.height / 2.0),
            color: self.colors.text,
            size: 12.0.into(),
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });
    }
}

/// Canvas program for rendering the bandwidth chart.
struct BandwidthChartProgram<'a> {
    history: Option<&'a BandwidthHistory>,
    time_window: ChartTimeWindow,
    colors: ChartColors,
    cache: &'a canvas::Cache,
}

impl<Message> canvas::Program<Message, Theme, Renderer> for BandwidthChartProgram<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            self.draw_chart(frame, bounds.size());
        });

        vec![geometry]
    }
}

impl BandwidthChartProgram<'_> {
    /// Draw the complete chart.
    fn draw_chart(&self, frame: &mut Frame, size: Size) {
        let padding = ChartPadding {
            left: 45.0,
            right: 10.0,
            top: 10.0,
            bottom: 20.0,
        };

        let chart_width = size.width - padding.left - padding.right;
        let chart_height = size.height - padding.top - padding.bottom;

        if chart_width <= 0.0 || chart_height <= 0.0 {
            return;
        }

        // Draw background
        frame.fill_rectangle(
            Point::new(padding.left, padding.top),
            Size::new(chart_width, chart_height),
            self.colors.background,
        );

        // Get samples and calculate scale
        let samples: Vec<_> = self
            .history
            .map(|h| h.samples_in_window(self.time_window.duration()).collect())
            .unwrap_or_default();

        if samples.len() < 2 {
            self.draw_no_data(frame, size);
            return;
        }

        // Calculate max value for Y-axis scaling
        let max_value = samples
            .iter()
            .map(|s| s.rx_bytes_per_sec.max(s.tx_bytes_per_sec))
            .fold(0.0_f64, |a, b| a.max(b))
            .max(1024.0); // Minimum 1 KB/s scale

        let now = Instant::now();
        let window_duration = self.time_window.duration();

        // Draw grid lines
        self.draw_grid(frame, &padding, chart_width, chart_height, max_value);

        // Draw RX line (download - blue)
        self.draw_data_line(
            frame,
            &samples,
            now,
            window_duration,
            &padding,
            chart_width,
            chart_height,
            max_value,
            |s| s.rx_bytes_per_sec,
            self.colors.rx,
        );

        // Draw TX line (upload - orange)
        self.draw_data_line(
            frame,
            &samples,
            now,
            window_duration,
            &padding,
            chart_width,
            chart_height,
            max_value,
            |s| s.tx_bytes_per_sec,
            self.colors.tx,
        );

        // Draw axes
        self.draw_axes(frame, &padding, chart_width, chart_height);

        // Draw legend
        self.draw_legend(frame, size);
    }

    /// Draw a data line on the chart.
    #[allow(clippy::too_many_arguments)]
    fn draw_data_line<F>(
        &self,
        frame: &mut Frame,
        samples: &[&crate::bandwidth_history::BandwidthSample],
        now: Instant,
        window_duration: Duration,
        padding: &ChartPadding,
        chart_width: f32,
        chart_height: f32,
        max_value: f64,
        value_fn: F,
        color: Color,
    ) where
        F: Fn(&crate::bandwidth_history::BandwidthSample) -> f64,
    {
        let path = Path::new(|builder| {
            let mut first = true;
            for sample in samples {
                let age = now.duration_since(sample.timestamp);
                let x = padding.left
                    + chart_width * (1.0 - age.as_secs_f32() / window_duration.as_secs_f32());
                let y = padding.top
                    + chart_height * (1.0 - value_fn(sample) as f32 / max_value as f32);

                // Clamp to chart bounds
                let x = x.clamp(padding.left, padding.left + chart_width);
                let y = y.clamp(padding.top, padding.top + chart_height);

                if first {
                    builder.move_to(Point::new(x, y));
                    first = false;
                } else {
                    builder.line_to(Point::new(x, y));
                }
            }
        });

        frame.stroke(&path, Stroke::default().with_width(2.0).with_color(color));
    }

    /// Draw grid lines and axis labels.
    fn draw_grid(
        &self,
        frame: &mut Frame,
        padding: &ChartPadding,
        width: f32,
        height: f32,
        max_value: f64,
    ) {
        // Horizontal grid lines (4 divisions)
        for i in 0..=4 {
            let y = padding.top + height * (i as f32 / 4.0);
            let path = Path::line(
                Point::new(padding.left, y),
                Point::new(padding.left + width, y),
            );
            frame.stroke(
                &path,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(self.colors.grid),
            );

            // Y-axis value label
            let value = max_value * ((4 - i) as f64 / 4.0);
            frame.fill_text(Text {
                content: format_rate(value),
                position: Point::new(padding.left - 5.0, y),
                color: self.colors.text,
                size: 10.0.into(),
                align_x: iced::alignment::Horizontal::Right.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        }

        // Time axis labels
        let time_labels: Vec<(f32, String)> = match self.time_window {
            ChartTimeWindow::OneMinute => vec![
                (1.0, "now".to_string()),
                (0.5, "30s".to_string()),
                (0.0, "1m".to_string()),
            ],
            ChartTimeWindow::FiveMinutes => vec![
                (1.0, "now".to_string()),
                (0.6, "2m".to_string()),
                (0.2, "4m".to_string()),
                (0.0, "5m".to_string()),
            ],
        };

        for (pos, label) in time_labels {
            let x = padding.left + width * pos;
            frame.fill_text(Text {
                content: label,
                position: Point::new(x, padding.top + height + 12.0),
                color: self.colors.text,
                size: 9.0.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Top,
                ..Default::default()
            });
        }
    }

    /// Draw X and Y axes.
    fn draw_axes(&self, frame: &mut Frame, padding: &ChartPadding, width: f32, height: f32) {
        // Y-axis
        let y_axis = Path::line(
            Point::new(padding.left, padding.top),
            Point::new(padding.left, padding.top + height),
        );
        frame.stroke(
            &y_axis,
            Stroke::default()
                .with_width(1.0)
                .with_color(self.colors.axis),
        );

        // X-axis
        let x_axis = Path::line(
            Point::new(padding.left, padding.top + height),
            Point::new(padding.left + width, padding.top + height),
        );
        frame.stroke(
            &x_axis,
            Stroke::default()
                .with_width(1.0)
                .with_color(self.colors.axis),
        );
    }

    /// Draw the legend.
    fn draw_legend(&self, frame: &mut Frame, size: Size) {
        let y = size.height - 8.0;
        let box_size = Size::new(10.0, 6.0);

        // RX legend
        frame.fill_rectangle(Point::new(50.0, y - 3.0), box_size, self.colors.rx);
        frame.fill_text(Text {
            content: "RX".to_string(),
            position: Point::new(63.0, y),
            color: self.colors.text,
            size: 9.0.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });

        // TX legend
        frame.fill_rectangle(Point::new(85.0, y - 3.0), box_size, self.colors.tx);
        frame.fill_text(Text {
            content: "TX".to_string(),
            position: Point::new(98.0, y),
            color: self.colors.text,
            size: 9.0.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });
    }

    /// Draw "No data" message when there's insufficient data.
    fn draw_no_data(&self, frame: &mut Frame, size: Size) {
        frame.fill_text(Text {
            content: "Collecting data...".to_string(),
            position: Point::new(size.width / 2.0, size.height / 2.0),
            color: self.colors.text,
            size: 12.0.into(),
            align_x: iced::alignment::Horizontal::Center.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Default::default()
        });
    }
}

/// Chart padding configuration.
struct ChartPadding {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

/// Format bytes per second with appropriate units.
fn format_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_073_741_824.0 {
        format!("{:.1}G", bytes_per_sec / 1_073_741_824.0)
    } else if bytes_per_sec >= 1_048_576.0 {
        format!("{:.1}M", bytes_per_sec / 1_048_576.0)
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.0}K", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0}B", bytes_per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_rate() {
        assert_eq!(format_rate(0.0), "0B");
        assert_eq!(format_rate(500.0), "500B");
        assert_eq!(format_rate(1500.0), "1K");
        assert_eq!(format_rate(1_500_000.0), "1.4M");
        assert_eq!(format_rate(1_500_000_000.0), "1.4G");
    }

    #[test]
    fn test_time_window_duration() {
        assert_eq!(
            ChartTimeWindow::OneMinute.duration(),
            Duration::from_secs(60)
        );
        assert_eq!(
            ChartTimeWindow::FiveMinutes.duration(),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn test_time_window_label() {
        assert_eq!(ChartTimeWindow::OneMinute.label(), "1m");
        assert_eq!(ChartTimeWindow::FiveMinutes.label(), "5m");
    }

    #[test]
    fn test_chart_colors_default() {
        let colors = ChartColors::default();
        // Just verify it doesn't panic and has reasonable values
        assert!(colors.rx.r >= 0.0 && colors.rx.r <= 1.0);
        assert!(colors.tx.r >= 0.0 && colors.tx.r <= 1.0);
    }

    #[test]
    fn test_chart_colors_dark() {
        let colors = ChartColors::dark();
        // Just verify it doesn't panic and has reasonable values
        assert!(colors.rx.r >= 0.0 && colors.rx.r <= 1.0);
        assert!(colors.tx.r >= 0.0 && colors.tx.r <= 1.0);
    }

    #[test]
    fn test_bandwidth_chart_new() {
        let chart = BandwidthChart::new();
        assert_eq!(chart.time_window, ChartTimeWindow::OneMinute);
    }

    #[test]
    fn test_bandwidth_chart_set_time_window() {
        let mut chart = BandwidthChart::new();
        chart.set_time_window(ChartTimeWindow::FiveMinutes);
        assert_eq!(chart.time_window, ChartTimeWindow::FiveMinutes);
    }
}
