# Iced 0.14 Improvements Plan

This document outlines potential improvements to tcgui-frontend based on new features available in Iced 0.14.

## Priority Legend

- **P1**: High impact, relatively easy to implement
- **P2**: Medium impact or moderate complexity
- **P3**: Nice to have, lower priority or higher complexity

---

## P1: High Priority Improvements

### 1. Tooltip Delays for TC Parameters

**Current State**: No tooltips explaining TC parameters.

**Improvement**: Add tooltips with configurable delays to explain what each TC parameter does (Loss, Delay, Jitter, Correlation, etc.).

**Files to Modify**:
- `tcgui-frontend/src/interface.rs` (TC controls)
- `tcgui-frontend/src/components/` (if component-based)

**Implementation**:
```rust
tooltip(
    checkbox("Loss", enabled),
    "Randomly drop packets at the specified percentage",
    tooltip::Position::Top
).delay(Duration::from_millis(500))
```

**Effort**: Low

---

### 2. Smart Scrollbars

**Current State**: Standard scrollbars in interface and scenario lists.

**Improvement**: Use smart scrollbars that auto-hide when content fits, providing a cleaner UI.

**Files to Modify**:
- `tcgui-frontend/src/view.rs`
- `tcgui-frontend/src/scenario_view.rs`

**Implementation**: Apply scrollbar styling options available in 0.14 for auto-hiding behavior.

**Effort**: Low

---

## P2: Medium Priority Improvements

### 3. ~~Grid Widget for TC Controls~~ (Not Applicable)

**Evaluation**: After investigating the Iced 0.14 Grid widget, it was determined to be unsuitable for TC controls:

- Grid uses equal-sized cells (designed for responsive image galleries)
- TC controls require different column widths (labels ~50px, sliders ~120px, values ~50px)
- Current row-based layout with explicit widths provides better control
- The existing implementation with tooltips works well

**Status**: Skipped - current row-based approach is more appropriate

---

---

### 4. Column Wrap for Interface Cards

**Current State**: Interface cards are laid out in a single column within each namespace.

**Improvement**: Use `column().wrap()` to allow interface cards to flow horizontally when window width permits, making better use of widescreen displays.

**Files to Modify**:
- `tcgui-frontend/src/view.rs` (namespace rendering)

**Implementation**:
```rust
column(interface_cards)
    .wrap()
    .spacing(scaled_spacing(8, zoom))
```

**Considerations**:
- May need to set max-width on interface cards
- Test with various window sizes
- Consider making this behavior optional via settings

**Effort**: Medium

---

### 5. Auto-Scrolling for Scenario Execution (Deferred)

**Current State**: Scenario execution progress may require manual scrolling to see current step.

**Improvement**: Use scrollable auto-scrolling to keep the currently executing step visible.

**Investigation Results**:
The implementation requires:
1. Adding a scrollable with a widget ID around the timeline
2. Tracking step changes in ScenarioManager
3. Returning `iced::widget::scrollable::snap_to()` Tasks when steps change
4. Coordinating between view rendering and state updates

This is more complex than initially estimated due to Iced's message-driven architecture.

**Status**: Deferred - requires significant refactoring of scenario execution flow

**Effort**: High (was: Medium)

---

### 6. Float/Pin for Overlay Dialogs

**Current State**: Interface selection dialog uses `stack!()` for overlay positioning.

**Improvement**: Evaluate `float` and `pin` widgets for potentially cleaner overlay positioning.

**Files to Modify**:
- `tcgui-frontend/src/view.rs` (dialog rendering)

**Investigation Needed**:
- Compare `float`/`pin` API with current `stack!()` approach
- Determine if it provides better positioning control
- May not be worth changing if current approach works well

**Effort**: Medium (includes investigation)

---

## P3: Lower Priority / Higher Complexity

### 7. Animation with iced_anim (Dependency Added, Implementation Deferred)

**Current State**: TC state changes (active/inactive) are instant with static colors.

**Improvement**: Add smooth animations using the [iced_anim](https://github.com/bradysimon/iced_anim) crate (v0.3.x for Iced 0.14).

**Progress**:
- [x] Added `iced_anim = "0.3"` dependency to Cargo.toml
- [ ] Implement animated interface card backgrounds
- [ ] Implement animated scenario step transitions

**Why iced_anim**:
- Provides `Animated<T>` wrapper for state values
- Supports spring-based animations (ideal for interactive UI) and transition-based with easing curves
- Built-in support for `f32`, `iced::Color`, and `iced::Theme`
- Can derive `Animate` trait for custom structs

**Implementation Complexity**:
Adding animations requires restructuring the component architecture:
1. `InterfaceState` must include `Animated<Color>` for background transitions
2. Animation tick messages need to propagate through the message hierarchy
3. Views must wrap content in `Animation` widgets
4. The `TcInterface::update()` must handle animation events

**Animation Candidates** (prioritized):
1. TC activation/deactivation background color fade
2. Status indicator color transitions
3. Scenario step progress highlighting

**Status**: Dependency added; implementation deferred for future PR

**Effort**: High (requires architectural changes to state management)

---

### 8. Table Widget for Interface List

**Current State**: Interfaces rendered as individual cards in a scrollable column.

**Improvement**: Use the new `table` widget for a more structured data presentation with sortable columns.

**Evaluation Needed**:
- Current card-based design is visually rich (bandwidth charts, TC controls inline)
- Table widget may be better suited for a "list view" mode
- Could offer both: Card View (current) and Table View (new)

**Potential Table Columns**:
- Interface name
- Namespace
- Status (Up/Down)
- Current bandwidth (RX/TX)
- TC Status (Active/Inactive)
- Quick actions

**Files to Modify**:
- `tcgui-frontend/src/view.rs`
- New: `tcgui-frontend/src/table_view.rs`

**Effort**: High (new view mode)

---

### 9. Sensor Widget for Hover Detection

**Current State**: Interface cards don't have special hover behavior beyond button states.

**Improvement**: Use `sensor` widget to detect hover on interface cards and show additional information or quick actions.

**Potential Uses**:
- Show detailed bandwidth stats on hover
- Reveal quick action buttons
- Highlight related interfaces

**Effort**: Medium-High

---

## Implementation Status

| Item | Status | Notes |
|------|--------|-------|
| Tooltip delays (P1) | **Done** | 500ms delay on all TC controls |
| Smart scrollbars (P1) | **Done** | State-based opacity (idle/hover/drag) |
| Grid for TC controls (P2) | **Skipped** | Not suitable - Grid uses equal-sized cells |
| Column wrap (P2) | **Done** | Interface cards wrap on wide screens |
| Auto-scrolling scenarios (P2) | **Deferred** | High complexity - requires widget IDs and Task coordination |
| Animations with iced_anim (P3) | **Deferred** | Dependency added; implementation requires architectural changes |

---

## Not Recommended

### Incremental Markdown Parser
Not applicable - the app doesn't use markdown rendering.

### Input Method Support
Already handled by Iced - no action needed unless international text input issues are reported.

### Concurrent Image Decoding
Not applicable - the app uses SVG icons, not raster images.

---

## Testing Considerations

- All visual changes should be tested at different zoom levels (0.5x to 2.0x)
- Theme changes must work correctly in both Light and Dark modes
- Animation performance should be verified on lower-end hardware
- Grid/wrap layouts need testing at various window sizes
