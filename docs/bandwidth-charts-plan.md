# Bandwidth Charts Implementation Plan

## Overview

Add historical bandwidth charts showing RX/TX rates over time for each interface. Currently, only instantaneous rates are displayed. This plan adds time-series visualization with configurable time windows.

## Current State

- `BandwidthUpdate` messages arrive every ~1 second from backend
- `BandwidthDisplayComponent` shows only current `rx_bytes_per_sec` / `tx_bytes_per_sec`
- No historical data is stored
- `NetworkBandwidthStats` contains raw counters and calculated rates

```rust
// Current: tcgui-shared
pub struct NetworkBandwidthStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub timestamp: u64,
    // ... other fields
}
```

## Design

### Data Storage

```rust
// src/bandwidth_history.rs

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Single data point in the bandwidth history
#[derive(Debug, Clone, Copy)]
pub struct BandwidthSample {
    pub timestamp: Instant,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
}

/// Time-series data for one interface
#[derive(Debug, Clone)]
pub struct BandwidthHistory {
    samples: VecDeque<BandwidthSample>,
    max_duration: Duration,
    max_samples: usize,
}

impl BandwidthHistory {
    pub fn new(max_duration: Duration) -> Self {
        // At 1 sample/sec, 5 minutes = 300 samples
        let max_samples = max_duration.as_secs() as usize;
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_duration,
            max_samples,
        }
    }
    
    pub fn push(&mut self, rx: f64, tx: f64) {
        let now = Instant::now();
        
        // Add new sample
        self.samples.push_back(BandwidthSample {
            timestamp: now,
            rx_bytes_per_sec: rx,
            tx_bytes_per_sec: tx,
        });
        
        // Remove old samples
        let cutoff = now - self.max_duration;
        while let Some(front) = self.samples.front() {
            if front.timestamp < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }
        
        // Enforce max samples limit
        while self.samples.len() > self.max_samples {
            self.samples.pop_front();
        }
    }
    
    pub fn samples(&self) -> &VecDeque<BandwidthSample> {
        &self.samples
    }
    
    /// Get samples within a time window
    pub fn samples_in_window(&self, window: Duration) -> impl Iterator<Item = &BandwidthSample> {
        let cutoff = Instant::now() - window;
        self.samples.iter().filter(move |s| s.timestamp >= cutoff)
    }
    
    /// Calculate peak values in window
    pub fn peak_in_window(&self, window: Duration) -> (f64, f64) {
        self.samples_in_window(window).fold((0.0, 0.0), |(max_rx, max_tx), s| {
            (max_rx.max(s.rx_bytes_per_sec), max_tx.max(s.tx_bytes_per_sec))
        })
    }
    
    /// Calculate average values in window
    pub fn average_in_window(&self, window: Duration) -> (f64, f64) {
        let samples: Vec<_> = self.samples_in_window(window).collect();
        if samples.is_empty() {
            return (0.0, 0.0);
        }
        let count = samples.len() as f64;
        let (sum_rx, sum_tx) = samples.iter().fold((0.0, 0.0), |(rx, tx), s| {
            (rx + s.rx_bytes_per_sec, tx + s.tx_bytes_per_sec)
        });
        (sum_rx / count, sum_tx / count)
    }
}

/// Manages history for all interfaces
#[derive(Debug, Default)]
pub struct BandwidthHistoryManager {
    /// Key: "backend:namespace:interface"
    histories: HashMap<String, BandwidthHistory>,
    default_duration: Duration,
}

impl BandwidthHistoryManager {
    pub fn new(default_duration: Duration) -> Self {
        Self {
            histories: HashMap::new(),
            default_duration,
        }
    }
    
    pub fn record(&mut self, backend: &str, namespace: &str, interface: &str, rx: f64, tx: f64) {
        let key = format!("{}:{}:{}", backend, namespace, interface);
        self.histories
            .entry(key)
            .or_insert_with(|| BandwidthHistory::new(self.default_duration))
            .push(rx, tx);
    }
    
    pub fn get(&self, backend: &str, namespace: &str, interface: &str) -> Option<&BandwidthHistory> {
        let key = format!("{}:{}:{}", backend, namespace, interface);
        self.histories.get(&key)
    }
    
    pub fn cleanup_stale(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.histories.retain(|_, history| {
            history.samples.back()
                .is_some_and(|s| now.duration_since(s.timestamp) < max_age)
        });
    }
}
```

### Chart Widget

Using Iced Canvas for rendering:

```rust
// src/bandwidth_chart.rs

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Size};

/// Time window options for the chart
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartTimeWindow {
    #[default]
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    OneHour,
}

impl ChartTimeWindow {
    pub fn duration(&self) -> Duration {
        match self {
            Self::OneMinute => Duration::from_secs(60),
            Self::FiveMinutes => Duration::from_secs(300),
            Self::FifteenMinutes => Duration::from_secs(900),
            Self::OneHour => Duration::from_secs(3600),
        }
    }
    
    pub fn label(&self) -> &'static str {
        match self {
            Self::OneMinute => "1m",
            Self::FiveMinutes => "5m",
            Self::FifteenMinutes => "15m",
            Self::OneHour => "1h",
        }
    }
}

/// Chart display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChartMode {
    #[default]
    Line,       // Line graph
    Area,       // Filled area under line
    Stacked,    // RX and TX stacked
}

pub struct BandwidthChart {
    history: BandwidthHistory,
    time_window: ChartTimeWindow,
    mode: ChartMode,
    show_rx: bool,
    show_tx: bool,
    cache: canvas::Cache,
}

impl BandwidthChart {
    pub fn new(history: BandwidthHistory) -> Self {
        Self {
            history,
            time_window: ChartTimeWindow::default(),
            mode: ChartMode::default(),
            show_rx: true,
            show_tx: true,
            cache: canvas::Cache::default(),
        }
    }
    
    pub fn update_history(&mut self, history: BandwidthHistory) {
        self.history = history;
        self.cache.clear();
    }
    
    pub fn set_time_window(&mut self, window: ChartTimeWindow) {
        self.time_window = window;
        self.cache.clear();
    }
    
    pub fn view(&self) -> Element<BandwidthChartMessage> {
        Canvas::new(self)
            .width(Length::Fill)
            .height(Length::Fixed(150.0))
            .into()
    }
}

impl canvas::Program<BandwidthChartMessage> for BandwidthChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            let padding = 40.0;
            let chart_width = size.width - padding * 2.0;
            let chart_height = size.height - padding * 2.0;
            
            // Draw background
            frame.fill_rectangle(
                Point::new(padding, padding),
                Size::new(chart_width, chart_height),
                Color::from_rgb(0.98, 0.98, 0.98),
            );
            
            // Get samples in window
            let samples: Vec<_> = self.history
                .samples_in_window(self.time_window.duration())
                .collect();
            
            if samples.len() < 2 {
                self.draw_no_data(frame, bounds);
                return;
            }
            
            // Calculate scale
            let max_value = samples.iter()
                .map(|s| s.rx_bytes_per_sec.max(s.tx_bytes_per_sec))
                .fold(0.0_f64, |a, b| a.max(b))
                .max(1024.0);  // Minimum 1 KB/s scale
            
            let now = Instant::now();
            let window_duration = self.time_window.duration();
            
            // Draw grid lines
            self.draw_grid(frame, padding, chart_width, chart_height, max_value);
            
            // Draw RX line
            if self.show_rx {
                self.draw_line(
                    frame, &samples, now, window_duration,
                    padding, chart_width, chart_height, max_value,
                    |s| s.rx_bytes_per_sec,
                    Color::from_rgb(0.0, 0.6, 0.9),
                );
            }
            
            // Draw TX line
            if self.show_tx {
                self.draw_line(
                    frame, &samples, now, window_duration,
                    padding, chart_width, chart_height, max_value,
                    |s| s.tx_bytes_per_sec,
                    Color::from_rgb(0.9, 0.5, 0.0),
                );
            }
            
            // Draw axes
            self.draw_axes(frame, padding, chart_width, chart_height);
            
            // Draw legend
            self.draw_legend(frame, size);
        });
        
        vec![geometry]
    }
}

impl BandwidthChart {
    fn draw_line<F>(
        &self,
        frame: &mut Frame,
        samples: &[&BandwidthSample],
        now: Instant,
        window_duration: Duration,
        padding: f32,
        chart_width: f32,
        chart_height: f32,
        max_value: f64,
        value_fn: F,
        color: Color,
    ) where
        F: Fn(&BandwidthSample) -> f64,
    {
        let mut path = Path::new(|builder| {
            let mut first = true;
            for sample in samples {
                let age = now.duration_since(sample.timestamp);
                let x = padding + chart_width * (1.0 - age.as_secs_f32() / window_duration.as_secs_f32());
                let y = padding + chart_height * (1.0 - value_fn(sample) as f32 / max_value as f32);
                
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
    
    fn draw_grid(
        &self,
        frame: &mut Frame,
        padding: f32,
        width: f32,
        height: f32,
        max_value: f64,
    ) {
        let grid_color = Color::from_rgb(0.9, 0.9, 0.9);
        let text_color = Color::from_rgb(0.5, 0.5, 0.5);
        
        // Horizontal grid lines (value axis)
        for i in 0..=4 {
            let y = padding + height * (i as f32 / 4.0);
            let path = Path::line(
                Point::new(padding, y),
                Point::new(padding + width, y),
            );
            frame.stroke(&path, Stroke::default().with_width(1.0).with_color(grid_color));
            
            // Value label
            let value = max_value * ((4 - i) as f64 / 4.0);
            frame.fill_text(canvas::Text {
                content: format_rate(value),
                position: Point::new(padding - 5.0, y),
                color: text_color,
                size: 10.0.into(),
                horizontal_alignment: iced::alignment::Horizontal::Right,
                vertical_alignment: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        }
    }
    
    fn draw_axes(&self, frame: &mut Frame, padding: f32, width: f32, height: f32) {
        let axis_color = Color::from_rgb(0.3, 0.3, 0.3);
        
        // Y axis
        let y_axis = Path::line(
            Point::new(padding, padding),
            Point::new(padding, padding + height),
        );
        frame.stroke(&y_axis, Stroke::default().with_width(1.0).with_color(axis_color));
        
        // X axis
        let x_axis = Path::line(
            Point::new(padding, padding + height),
            Point::new(padding + width, padding + height),
        );
        frame.stroke(&x_axis, Stroke::default().with_width(1.0).with_color(axis_color));
    }
    
    fn draw_legend(&self, frame: &mut Frame, size: Size) {
        let y = size.height - 15.0;
        
        if self.show_rx {
            frame.fill_rectangle(
                Point::new(50.0, y - 4.0),
                Size::new(12.0, 8.0),
                Color::from_rgb(0.0, 0.6, 0.9),
            );
            frame.fill_text(canvas::Text {
                content: "RX".to_string(),
                position: Point::new(65.0, y),
                color: Color::BLACK,
                size: 10.0.into(),
                ..Default::default()
            });
        }
        
        if self.show_tx {
            frame.fill_rectangle(
                Point::new(100.0, y - 4.0),
                Size::new(12.0, 8.0),
                Color::from_rgb(0.9, 0.5, 0.0),
            );
            frame.fill_text(canvas::Text {
                content: "TX".to_string(),
                position: Point::new(115.0, y),
                color: Color::BLACK,
                size: 10.0.into(),
                ..Default::default()
            });
        }
    }
    
    fn draw_no_data(&self, frame: &mut Frame, bounds: Rectangle) {
        frame.fill_text(canvas::Text {
            content: "No data".to_string(),
            position: Point::new(bounds.width / 2.0, bounds.height / 2.0),
            color: Color::from_rgb(0.5, 0.5, 0.5),
            size: 14.0.into(),
            horizontal_alignment: iced::alignment::Horizontal::Center,
            vertical_alignment: iced::alignment::Vertical::Center,
            ..Default::default()
        });
    }
}

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

#[derive(Debug, Clone)]
pub enum BandwidthChartMessage {
    SetTimeWindow(ChartTimeWindow),
    SetMode(ChartMode),
    ToggleRx,
    ToggleTx,
}
```

## Integration Options

### Option A: Inline Mini-Charts (Per Interface)

Add a small sparkline next to each interface:

```
┌─────────────────────────────────────────────────────────┐
│ eth0 [UP] [TC: 50ms delay]   📈 1.2M 📤 340K  ▂▃▅▇▆▄▃▂ │
└─────────────────────────────────────────────────────────┘
```

- Pros: Always visible, compact
- Cons: Limited detail, no interactivity

### Option B: Expandable Chart (Per Interface)

Click interface to expand and show full chart:

```
┌─────────────────────────────────────────────────────────┐
│ eth0 [UP] [TC: 50ms delay]   📈 1.2M 📤 340K     [▼]   │
├─────────────────────────────────────────────────────────┤
│ [1m] [5m] [15m] [1h]                    Peak: 2.1M/s   │
│ ┌─────────────────────────────────────────────────────┐ │
│ │         ╱╲    ╱╲                                    │ │
│ │ RX ────╱  ╲──╱  ╲───────────────────────            │ │
│ │                                                     │ │
│ │ TX ─────────────────────────────────────            │ │
│ └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

- Pros: Detailed when needed, space-efficient
- Cons: Requires click to view

### Option C: Dedicated Charts Tab

New tab showing all interfaces with charts:

```
┌─────────────────────────────────────────────────────────┐
│ [Interfaces] [Scenarios] [Charts]                       │
├─────────────────────────────────────────────────────────┤
│ Time: [1m] [5m] [15m] [1h]        Interface: [All ▼]   │
├─────────────────────────────────────────────────────────┤
│ eth0                                                    │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ (chart)                                             │ │
│ └─────────────────────────────────────────────────────┘ │
│ docker0                                                 │
│ ┌─────────────────────────────────────────────────────┐ │
│ │ (chart)                                             │ │
│ └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

- Pros: Full focus on metrics, compare interfaces
- Cons: Separate from interface controls

### Recommended: Option B (Expandable)

Best balance of visibility and detail. Can start simple and add features.

## Implementation Phases

### Phase 1: Data Collection (2-3 hours)

1. Create `src/bandwidth_history.rs` with `BandwidthHistory` and `BandwidthHistoryManager`
2. Add `BandwidthHistoryManager` to `TcGui` state
3. Record samples in `handle_bandwidth_update()`
4. Add periodic cleanup of stale data

### Phase 2: Basic Chart Widget (3-4 hours)

1. Create `src/bandwidth_chart.rs` with canvas rendering
2. Implement line chart drawing
3. Add grid and axis rendering
4. Add RX/TX legend

### Phase 3: Interface Integration (2-3 hours)

1. Add expand/collapse toggle to `TcInterface`
2. Pass history to interface component
3. Render chart when expanded
4. Add time window selector buttons

### Phase 4: Polish (2-3 hours)

1. Smooth line rendering (bezier curves)
2. Hover tooltips with exact values
3. Peak/average indicators
4. Auto-scaling improvements
5. Theme integration (dark mode colors)

## Files to Create/Modify

| File | Purpose |
|------|---------|
| `src/bandwidth_history.rs` | History storage and management |
| `src/bandwidth_chart.rs` | Chart widget and rendering |
| `src/app.rs` | Add history manager, record samples |
| `src/interface/base.rs` | Add chart expand toggle |
| `src/interface/state.rs` | Add expanded state |
| `src/messages.rs` | Add chart messages |

## Memory Considerations

At 1 sample/second:
- 1 minute = 60 samples
- 5 minutes = 300 samples
- 1 hour = 3600 samples

Per sample: ~24 bytes (timestamp + 2x f64)
Per interface (1 hour): ~86 KB
100 interfaces for 1 hour: ~8.6 MB

Recommendation: Default to 5-minute history, allow user to increase.

## Performance Considerations

1. **Cache rendering**: Use `canvas::Cache` to avoid redrawing every frame
2. **Throttle updates**: Invalidate cache at most 1/second
3. **Lazy loading**: Only compute visible charts
4. **Downsampling**: For longer time windows, average multiple samples

## Estimated Effort

- **Minimum viable** (single line chart): 5-6 hours
- **With controls and polish**: 8-10 hours
- **Full featured** (multiple modes, tooltips): 12-15 hours

## Future Enhancements

- Export chart data as CSV
- Compare multiple interfaces overlay
- Bandwidth alerts (threshold warnings)
- Historical data persistence (save to file)
- Aggregate charts (total backend bandwidth)
- Packets/second view alongside bytes
