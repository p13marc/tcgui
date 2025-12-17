# Dark Mode Implementation Plan

## Overview

Add theme support to TC GUI with light/dark mode toggle. Currently, colors are hardcoded throughout the frontend components. This plan introduces a centralized theme system.

## Current State

Colors are scattered across multiple files:
- `interface/base.rs` - Interface card styling
- `interface/display/bandwidth.rs` - Bandwidth text colors
- `interface/display/status.rs` - Status indicator colors
- `interface/preset/manager.rs` - Preset dropdown styling
- `scenario_view.rs` - Scenario cards and progress bars
- `view.rs` - Main layout colors

Example of current hardcoded colors:
```rust
Color::from_rgb(0.4, 0.4, 0.4)  // text_secondary
Color::from_rgb(0.0, 0.6, 0.9)  // rx bandwidth (blue)
Color::from_rgb(0.9, 0.5, 0.0)  // tx bandwidth (orange)
```

## Design

### Theme Structure

```rust
// src/theme.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub mode: ThemeMode,
    pub colors: ThemeColors,
}

#[derive(Debug, Clone)]
pub struct ThemeColors {
    // Base colors
    pub background: Color,
    pub surface: Color,
    pub surface_hover: Color,
    
    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    
    // Semantic colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    
    // Bandwidth indicators
    pub rx_color: Color,  // Download/receive
    pub tx_color: Color,  // Upload/transmit
    
    // Interface states
    pub interface_up: Color,
    pub interface_down: Color,
    pub tc_active: Color,
    pub tc_inactive: Color,
    
    // Borders and dividers
    pub border: Color,
    pub divider: Color,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            colors: ThemeColors {
                background: Color::from_rgb(0.95, 0.95, 0.95),
                surface: Color::WHITE,
                surface_hover: Color::from_rgb(0.98, 0.98, 0.98),
                
                text_primary: Color::from_rgb(0.1, 0.1, 0.1),
                text_secondary: Color::from_rgb(0.4, 0.4, 0.4),
                text_muted: Color::from_rgb(0.6, 0.6, 0.6),
                
                success: Color::from_rgb(0.2, 0.7, 0.3),
                warning: Color::from_rgb(0.9, 0.6, 0.1),
                error: Color::from_rgb(0.9, 0.3, 0.3),
                info: Color::from_rgb(0.2, 0.5, 0.8),
                
                rx_color: Color::from_rgb(0.0, 0.6, 0.9),
                tx_color: Color::from_rgb(0.9, 0.5, 0.0),
                
                interface_up: Color::from_rgb(0.2, 0.7, 0.3),
                interface_down: Color::from_rgb(0.6, 0.6, 0.6),
                tc_active: Color::from_rgb(0.9, 0.6, 0.1),
                tc_inactive: Color::from_rgb(0.8, 0.8, 0.8),
                
                border: Color::from_rgb(0.85, 0.85, 0.85),
                divider: Color::from_rgb(0.9, 0.9, 0.9),
            },
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            colors: ThemeColors {
                background: Color::from_rgb(0.1, 0.1, 0.12),
                surface: Color::from_rgb(0.15, 0.15, 0.18),
                surface_hover: Color::from_rgb(0.2, 0.2, 0.24),
                
                text_primary: Color::from_rgb(0.95, 0.95, 0.95),
                text_secondary: Color::from_rgb(0.7, 0.7, 0.7),
                text_muted: Color::from_rgb(0.5, 0.5, 0.5),
                
                success: Color::from_rgb(0.3, 0.8, 0.4),
                warning: Color::from_rgb(1.0, 0.7, 0.2),
                error: Color::from_rgb(1.0, 0.4, 0.4),
                info: Color::from_rgb(0.4, 0.7, 1.0),
                
                rx_color: Color::from_rgb(0.3, 0.8, 1.0),
                tx_color: Color::from_rgb(1.0, 0.6, 0.2),
                
                interface_up: Color::from_rgb(0.3, 0.8, 0.4),
                interface_down: Color::from_rgb(0.5, 0.5, 0.5),
                tc_active: Color::from_rgb(1.0, 0.7, 0.2),
                tc_inactive: Color::from_rgb(0.3, 0.3, 0.3),
                
                border: Color::from_rgb(0.25, 0.25, 0.3),
                divider: Color::from_rgb(0.2, 0.2, 0.25),
            },
        }
    }
    
    pub fn toggle(&self) -> Self {
        match self.mode {
            ThemeMode::Light => Self::dark(),
            ThemeMode::Dark => Self::light(),
        }
    }
}
```

### Integration Points

#### 1. Application State

Add theme to `TcGui` and `UiStateManager`:

```rust
// ui_state.rs
pub struct UiStateManager {
    // ... existing fields
    pub theme: Theme,
}

// messages.rs
pub enum TcGuiMessage {
    // ... existing variants
    ToggleTheme,
}
```

#### 2. Theme Toggle Button

Add to the header/toolbar area:

```rust
// view.rs
fn render_header(theme: &Theme) -> Element<TcGuiMessage> {
    let icon = match theme.mode {
        ThemeMode::Light => "🌙",  // Moon for switching to dark
        ThemeMode::Dark => "☀️",   // Sun for switching to light
    };
    
    button(text(icon))
        .on_press(TcGuiMessage::ToggleTheme)
        .into()
}
```

#### 3. Pass Theme to Components

Components receive theme via view methods:

```rust
// Before
pub fn view(&self) -> Element<'_, TcInterfaceMessage>

// After
pub fn view(&self, theme: &Theme) -> Element<'_, TcInterfaceMessage>
```

## Implementation Steps

### Phase 1: Theme Infrastructure (1-2 hours)

1. Create `src/theme.rs` with `Theme`, `ThemeMode`, `ThemeColors`
2. Add `theme: Theme` to `UiStateManager`
3. Add `ToggleTheme` message variant
4. Handle theme toggle in `app.rs` update function

### Phase 2: Theme Propagation (2-3 hours)

1. Update `render_main_view()` to accept `&Theme`
2. Update `TcInterface::view()` to accept `&Theme`
3. Update display components (`BandwidthDisplayComponent`, `StatusDisplayComponent`)
4. Update `ScenarioView` to accept `&Theme`
5. Update preset manager components

### Phase 3: Color Migration (2-3 hours)

1. Replace hardcoded colors in `interface/base.rs`
2. Replace hardcoded colors in `interface/display/*.rs`
3. Replace hardcoded colors in `scenario_view.rs`
4. Replace hardcoded colors in `view.rs`
5. Replace hardcoded colors in preset components

### Phase 4: Persistence (Optional, 1 hour)

1. Save theme preference to config file (`~/.config/tcgui/settings.json`)
2. Load theme on startup
3. Respect system theme preference (if detectable)

## Files to Modify

| File | Changes |
|------|---------|
| `src/theme.rs` | New file - theme definitions |
| `src/lib.rs` or `src/main.rs` | Export theme module |
| `src/ui_state.rs` | Add theme field |
| `src/messages.rs` | Add ToggleTheme message |
| `src/app.rs` | Handle ToggleTheme, pass theme to view |
| `src/view.rs` | Accept theme, use theme colors |
| `src/interface/base.rs` | Accept theme, migrate colors |
| `src/interface/display/bandwidth.rs` | Accept theme, migrate colors |
| `src/interface/display/status.rs` | Accept theme, migrate colors |
| `src/interface/preset/manager.rs` | Accept theme, migrate colors |
| `src/scenario_view.rs` | Accept theme, migrate colors |

## Testing

1. Visual verification in both modes
2. Ensure all text is readable in both themes
3. Verify contrast ratios meet accessibility guidelines
4. Test theme toggle responsiveness

## Estimated Effort

- **Minimum viable**: 4-6 hours (theme system + major components)
- **Complete migration**: 6-8 hours (all components + polish)
- **With persistence**: 7-9 hours (save/load preferences)

## Future Enhancements

- Custom theme colors via config file
- High contrast mode for accessibility
- System theme detection (follow OS preference)
- Accent color customization
