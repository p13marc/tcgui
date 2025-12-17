# Preset System UI Implementation Plan

This document outlines the implementation plan for integrating the preset UI into the TC GUI frontend.

## Current State

The preset system has the foundational components in place:

| Component | Location | Status |
|-----------|----------|--------|
| `NetworkPreset` enum | `tcgui-shared/src/presets.rs` | Complete (8 presets) |
| `PresetConfiguration` struct | `tcgui-shared/src/presets.rs` | Complete |
| `PresetManagerComponent` | `tcgui-frontend/src/interface/preset/manager.rs` | Skeleton (unused) |
| `InterfaceState.current_preset` | `tcgui-frontend/src/interface/state.rs` | Field exists (dead_code) |

## Implementation Phases

### Phase 1: Message Infrastructure

**Effort:** 30 minutes

**Files to modify:**
- `tcgui-frontend/src/interface/messages.rs`
- `tcgui-frontend/src/messages.rs`

**Tasks:**

1. Add preset messages to `TcInterfaceMessage`:
```rust
// In tcgui-frontend/src/interface/messages.rs or messages.rs
pub enum TcInterfaceMessage {
    // ... existing messages ...
    
    // Preset messages
    PresetSelected(NetworkPreset),
    TogglePresetDropdown,
}
```

2. No changes needed to `TcGuiMessage` - we'll use existing `ApplyTc` for backend communication.

---

### Phase 2: Preset Application Logic

**Effort:** 1 hour

**Files to modify:**
- `tcgui-frontend/src/interface/preset/manager.rs`
- `tcgui-frontend/src/interface/state.rs`

**Tasks:**

1. Implement `apply_preset()` in `PresetManagerComponent`:
```rust
/// Apply a preset configuration to the interface state
pub fn apply_preset(
    &mut self, 
    preset: NetworkPreset, 
    state: &mut InterfaceState
) {
    self.current_preset = preset.clone();
    state.current_preset = preset.clone();
    
    if matches!(preset, NetworkPreset::Custom) {
        // Custom preset doesn't change settings
        return;
    }
    
    let config = preset.get_configuration();
    
    // Apply loss settings
    if config.loss > 0.0 {
        state.features.loss.enable();
        state.features.loss.config.percentage = config.loss;
        state.features.loss.config.correlation = config.correlation.unwrap_or(0.0);
    } else {
        state.features.loss.disable();
    }
    
    // Apply delay settings
    if config.delay_enabled {
        state.features.delay.enable();
        state.features.delay.config.delay_ms = config.delay_ms.unwrap_or(0.0);
        state.features.delay.config.jitter_ms = config.delay_jitter_ms.unwrap_or(0.0);
        state.features.delay.config.correlation = config.delay_correlation.unwrap_or(0.0);
    } else {
        state.features.delay.disable();
    }
    
    // Apply duplicate settings
    if config.duplicate_enabled {
        state.features.duplicate.enable();
        state.features.duplicate.config.percentage = config.duplicate_percent.unwrap_or(0.0);
        state.features.duplicate.config.correlation = config.duplicate_correlation.unwrap_or(0.0);
    } else {
        state.features.duplicate.disable();
    }
    
    // Apply reorder settings
    if config.reorder_enabled {
        state.features.reorder.enable();
        state.features.reorder.config.percentage = config.reorder_percent.unwrap_or(0.0);
        state.features.reorder.config.correlation = config.reorder_correlation.unwrap_or(0.0);
        state.features.reorder.config.gap = config.reorder_gap.unwrap_or(5);
    } else {
        state.features.reorder.disable();
    }
    
    // Apply corrupt settings
    if config.corrupt_enabled {
        state.features.corrupt.enable();
        state.features.corrupt.config.percentage = config.corrupt_percent.unwrap_or(0.0);
        state.features.corrupt.config.correlation = config.corrupt_correlation.unwrap_or(0.0);
    } else {
        state.features.corrupt.disable();
    }
    
    // Apply rate limit settings
    if config.rate_limit_enabled {
        state.features.rate_limit.enable();
        state.features.rate_limit.config.rate_kbps = config.rate_limit_kbps.unwrap_or(1000);
    } else {
        state.features.rate_limit.disable();
    }
    
    // Mark as applying to trigger backend update
    state.applying = true;
}
```

2. Remove `#[allow(dead_code)]` from `current_preset` in `InterfaceState`.

---

### Phase 3: Message Handling

**Effort:** 30 minutes

**Files to modify:**
- `tcgui-frontend/src/interface/base.rs`

**Tasks:**

1. Add message handlers in `TcInterface::update()`:
```rust
TcInterfaceMessage::PresetSelected(preset) => {
    tracing::debug!("Preset selected: {:?}", preset);
    self.preset_manager.apply_preset(preset, &mut self.state);
    // The applying flag is set, which will trigger TC apply
    Task::none()
}

TcInterfaceMessage::TogglePresetDropdown => {
    self.preset_manager.show_presets = !self.preset_manager.show_presets;
    Task::none()
}
```

2. Remove `#[allow(dead_code)]` from `preset_manager` field.

---

### Phase 4: Preset UI Component

**Effort:** 1-2 hours

**Files to modify:**
- `tcgui-frontend/src/interface/base.rs` (view method)
- `tcgui-frontend/src/interface/preset/manager.rs` (add view method)

**Tasks:**

1. Add preset dropdown view in `PresetManagerComponent`:
```rust
/// Render the preset selector UI
pub fn view(&self) -> Element<TcInterfaceMessage> {
    let current_label = self.current_preset.display_name();
    
    let dropdown_button = button(
        row![
            text(&current_label).size(12),
            text(if self.show_presets { " ^" } else { " v" }).size(10),
        ]
    )
    .padding(4)
    .on_press(TcInterfaceMessage::TogglePresetDropdown);
    
    if self.show_presets {
        let preset_buttons: Vec<Element<_>> = self.available_presets
            .iter()
            .map(|preset| {
                let is_selected = *preset == self.current_preset;
                button(
                    text(preset.display_name())
                        .size(11)
                        .style(if is_selected { 
                            Color::from_rgb(0.2, 0.6, 1.0) 
                        } else { 
                            Color::WHITE 
                        })
                )
                .padding(4)
                .width(Length::Fill)
                .on_press(TcInterfaceMessage::PresetSelected(preset.clone()))
                .into()
            })
            .collect();
        
        column![
            dropdown_button,
            container(
                column(preset_buttons).spacing(2)
            )
            .padding(4)
            .style(container::rounded_box)
        ]
        .into()
    } else {
        dropdown_button.into()
    }
}
```

2. Integrate preset selector in `TcInterface::render_main_row()`:
```rust
// Add after interface name, before feature toggles
let preset_selector = self.preset_manager.view();

row![
    interface_checkbox,
    text(&self.state.name).width(Length::Fixed(80.0)),
    preset_selector,  // NEW
    // ... feature checkboxes ...
]
```

---

### Phase 5: Visual Polish & UX

**Effort:** 1 hour

**Tasks:**

1. **Tooltip/Description**: Show preset description on hover
2. **Visual Indicator**: Highlight when current settings match a preset
3. **Custom Detection**: Auto-switch to "Custom" when user manually changes a parameter after applying a preset
4. **Keyboard Navigation**: Support arrow keys in dropdown

---

### Phase 6: Testing

**Effort:** 1 hour

**Files to modify:**
- `tcgui-frontend/src/interface/preset/manager.rs` (add tests)

**Tasks:**

1. Add unit tests for preset application:
```rust
#[test]
fn test_apply_satellite_preset() {
    let mut component = PresetManagerComponent::new();
    let mut state = InterfaceState::new("eth0");
    
    component.apply_preset(NetworkPreset::SatelliteLink, &mut state);
    
    assert!(state.features.loss.enabled);
    assert_eq!(state.features.loss.config.percentage, 1.0);
    assert!(state.features.delay.enabled);
    assert_eq!(state.features.delay.config.delay_ms, 500.0);
    assert!(state.features.rate_limit.enabled);
    assert_eq!(state.features.rate_limit.config.rate_kbps, 2000);
    assert!(!state.features.corrupt.enabled);
}

#[test]
fn test_apply_custom_preset_preserves_settings() {
    let mut component = PresetManagerComponent::new();
    let mut state = InterfaceState::new("eth0");
    
    // Set some manual values
    state.features.loss.enable();
    state.features.loss.config.percentage = 5.0;
    
    // Apply Custom preset - should not change settings
    component.apply_preset(NetworkPreset::Custom, &mut state);
    
    assert!(state.features.loss.enabled);
    assert_eq!(state.features.loss.config.percentage, 5.0);
}
```

2. Integration test: Apply preset and verify TC command would include correct parameters.

---

## Implementation Order

| Step | Phase | Priority | Dependencies |
|------|-------|----------|--------------|
| 1 | Phase 1: Messages | Required | None |
| 2 | Phase 2: Apply Logic | Required | Phase 1 |
| 3 | Phase 3: Handlers | Required | Phase 2 |
| 4 | Phase 4: UI | Required | Phase 3 |
| 5 | Phase 6: Tests | Required | Phase 2 |
| 6 | Phase 5: Polish | Optional | Phase 4 |

## Estimated Total Effort

- **Minimum (functional):** 3-4 hours
- **Complete (with polish):** 5-6 hours

## Verification

```bash
# After each phase
just dev-fast    # Format + clippy + fast tests

# Final verification  
just dev         # Full development cycle
just test        # All tests pass
```

## Future Enhancements

1. **Custom Preset Saving**: Allow users to save current settings as a named preset
2. **Preset Import/Export**: JSON file for sharing presets
3. **Per-Interface Presets**: Remember last-used preset per interface
4. **Preset Categories**: Group presets by use case (Mobile, Satellite, Testing)
