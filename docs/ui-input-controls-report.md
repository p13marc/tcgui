# UI/UX Report: Slider vs Text Input for TC Values

## Executive Summary

This report analyzes the best approach for TC GUI's numeric input controls, addressing the preference divide between users who favor sliders (visual, quick adjustments) and those who prefer text inputs (precise, keyboard-driven). The recommended solution is a **dual-control pattern** that combines both input methods in a synchronized widget.

---

## Current State Analysis

TC GUI currently uses **sliders exclusively** for all 12 numeric parameters:

| Parameter | Range | Step | Current Input |
|-----------|-------|------|---------------|
| Loss % | 0.0-100.0 | 0.1 | Slider only |
| Loss Correlation | 0.0-100.0 | 0.1 | Slider only |
| Delay (ms) | 0.0-5000.0 | 1.0 | Slider only |
| Delay Jitter (ms) | 0.0-1000.0 | 1.0 | Slider only |
| Delay Correlation | 0.0-100.0 | 0.1 | Slider only |
| Duplicate % | 0.0-100.0 | 0.1 | Slider only |
| Duplicate Correlation | 0.0-100.0 | 0.1 | Slider only |
| Reorder % | 0.0-100.0 | 0.1 | Slider only |
| Reorder Gap | 1-10 | 1 | Slider only |
| Reorder Correlation | 0.0-100.0 | 0.1 | Slider only |
| Corrupt % | 0.0-100.0 | 0.1 | Slider only |
| Rate Limit (kbps) | 1-1000000 | 1 | Slider only |

---

## User Preference Analysis

### Slider Advocates

**Pros:**
- Visual feedback on value position within range
- Quick "ballpark" adjustments with mouse/touch
- Intuitive for exploration ("about 30%")
- Good for relative adjustments ("a bit more/less")

**Cons:**
- Difficult to hit exact values (motor skill dependent)
- Frustrating for accessibility users with motor difficulties
- Slow for users who know exact target values
- Tedious for large ranges (e.g., rate limit 1-1,000,000 kbps)

### Text Input Advocates

**Pros:**
- Precise value entry (type exactly "47.5%")
- Fast for power users who know targets
- Keyboard-friendly workflow
- Better accessibility for motor-impaired users
- Essential for large ranges

**Cons:**
- No visual context of value position in range
- Requires validation handling for out-of-range/invalid input
- Slower for exploration-style adjustments
- No immediate visual feedback during typing

---

## Industry Best Practices

Research from Nielsen Norman Group, Smashing Magazine, and Baymard Institute consistently recommends the **dual-control pattern**:

> "Combine visual & numeric inputs: Offer both sliders and precise input fields—this caters to both casual users and those needing exact values."
> — [Smashing Magazine: Designing The Perfect Slider](https://www.smashingmagazine.com/2017/07/designing-perfect-slider/)

> "Selecting a precise value using a slider is a difficult task requiring good motor skills. If picking an exact value is important, choose an alternate UI element."
> — [NN/G: Slider Design Rules of Thumb](https://www.nngroup.com/articles/gui-slider-controls/)

> "Make sliders keyboard-navigable and include visual indicators. Allow manual value entry when slider isn't practical."
> — [Uxcel: Slider Best Practices](https://app.uxcel.com/courses/ui-components-n-patterns/sliders-best-practices-918)

### Real-World Examples

- **Airbnb**: Price filters use sliders with editable numeric fields
- **Figma**: Combines drag handles with direct input fields
- **Video editors**: Timeline + timecode input synchronized
- **Audio software**: Knobs/faders with numeric readouts you can click to edit

---

## Recommended Solution: Synchronized Dual-Control Widget

### Design Concept

```
┌──────────────────────────────────────────────────────────────┐
│ Delay:  ├────────●──────────────────┤  [  250  ] ms         │
│         0                         5000                       │
└──────────────────────────────────────────────────────────────┘
          ↑                              ↑
     Visual slider                 Editable text field
     (click/drag)                  (click to type)
```

### Behavior Specification

1. **Bidirectional Sync**: Slider and text field always show same value
2. **Real-time Slider Updates**: Dragging slider updates text field live
3. **Commit-on-Enter/Blur**: Text changes apply on Enter key or focus loss
4. **Validation Feedback**: Invalid text shows error state, reverts on blur
5. **Range Clamping**: Out-of-range values clamp to min/max with visual feedback

### Implementation Options for Iced

#### Option A: Compose Existing Widgets (Recommended)

Use Iced's `row!` layout to combine `slider` and `text_input`:

```rust
fn value_control<'a>(
    label: &str,
    value: f32,
    range: RangeInclusive<f32>,
    on_slider_change: impl Fn(f32) -> Message,
    on_text_change: impl Fn(String) -> Message,
    text_state: &str,
    unit: &str,
) -> Element<'a, Message> {
    row![
        text(label).width(60),
        slider(range, value, on_slider_change)
            .width(Length::FillPortion(3))
            .step(0.1),
        text_input("", text_state)
            .on_input(on_text_change)
            .width(Length::Fixed(60.0)),
        text(unit).width(30),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
```

#### Option B: Use iced_aw NumberInput

The [iced_aw](https://github.com/iced-rs/iced_aw) library provides `NumberInput`:

```rust
use iced_aw::NumberInput;

NumberInput::new(
    value,
    0.0..=100.0,
    Message::ValueChanged
)
.step(0.1)
```

This could be placed adjacent to a slider for a combined experience.

#### Option C: Custom Widget Component

Create a reusable `SliderInput` widget encapsulating the dual-control pattern:

```rust
pub struct SliderInput<T> {
    value: T,
    range: RangeInclusive<T>,
    step: T,
    text_state: String,
    editing: bool,
}

impl SliderInput<f32> {
    pub fn view(&self) -> Element<SliderInputMessage> {
        // Internal row with slider + text_input
        // Handles synchronization internally
    }
    
    pub fn update(&mut self, msg: SliderInputMessage) {
        match msg {
            SliderInputMessage::SliderMoved(v) => {
                self.value = v;
                self.text_state = format!("{:.1}", v);
            }
            SliderInputMessage::TextChanged(s) => {
                self.text_state = s;
            }
            SliderInputMessage::TextSubmit => {
                if let Ok(v) = self.text_state.parse() {
                    self.value = v.clamp(*self.range.start(), *self.range.end());
                }
                self.text_state = format!("{:.1}", self.value);
            }
        }
    }
}
```

---

## Special Considerations by Parameter Type

### Percentages (Loss, Corruption, Duplicate, Reorder, Correlations)

- **Range**: 0.0 - 100.0
- **Precision**: 1 decimal place (0.1 step)
- **Recommendation**: Standard dual-control works well
- **Text width**: 5 characters ("100.0")

### Time Values (Delay, Jitter)

- **Range**: 0 - 5000 ms (delay), 0 - 1000 ms (jitter)
- **Precision**: Whole milliseconds typical, sub-ms optional
- **Recommendation**: Consider unit dropdown (ms/s) for flexibility
- **Text width**: 4-5 characters

### Rate Limit

- **Range**: 1 - 1,000,000 kbps (very large range!)
- **Precision**: Whole numbers
- **Recommendation**: 
  - Text input becomes **primary** for this field
  - Slider uses logarithmic scale OR
  - Add unit selector (kbps/Mbps/Gbps)
  - Consider preset quick-buttons: "1M", "10M", "100M", "1G"

```
Rate: [──●───────────] [ 15000 ] ▼ kbps  [1M] [10M] [100M]
           log scale     text input  unit   quick presets
```

### Reorder Gap

- **Range**: 1 - 10 (very small discrete range)
- **Precision**: Integers only
- **Recommendation**: 
  - Slider works fine for this small range
  - Could use stepper buttons instead: [−] 5 [+]
  - Or radio buttons / segmented control

---

## Accessibility Improvements

The dual-control pattern significantly improves accessibility:

| User Need | Slider Only | Dual Control |
|-----------|-------------|--------------|
| Motor impairment | Difficult | Text input alternative |
| Keyboard navigation | Limited | Full keyboard support |
| Screen readers | Value only | Label + value + range |
| Low vision | Hard to see thumb | Can read/type numbers |
| Power users | Slower | Fast direct entry |

### Required Accessibility Features

1. **Keyboard navigation**: Tab between controls, arrow keys for slider
2. **ARIA labels**: Announce "Delay: 250 milliseconds, range 0 to 5000"
3. **Focus indicators**: Clear visual focus states
4. **Error announcements**: Screen reader announces validation errors

---

## Implementation Roadmap

### Phase 1: Foundation (Low Risk)

1. Add text input next to each slider (read-only display initially)
2. Ensure consistent layout across all parameters
3. Add unit labels where missing

### Phase 2: Bidirectional Sync

1. Make text inputs editable
2. Implement slider → text sync (already have value display)
3. Implement text → slider sync with validation
4. Handle parsing errors gracefully

### Phase 3: Enhanced Controls

1. Special treatment for Rate Limit (log scale or presets)
2. Stepper buttons for Gap parameter
3. Unit selector for large values

### Phase 4: Polish

1. Keyboard shortcuts (up/down arrows in text field)
2. Click-to-select-all in text fields
3. Validation error styling
4. Tooltip updates for dual-control pattern

---

## Recommended UI Layout

### Before (Current)

```
Loss:    ├────────●───────────────────┤  25.0%
```

### After (Proposed)

```
Loss:    ├────────●───────────────────┤  [ 25.0 ] %
         ↑ drag to adjust              ↑ click to type exact value
```

### Compact Alternative

For space-constrained views, stack vertically:

```
Loss ────────────────────────
├────────●───────────────────┤
[ 25.0 ] %
```

---

## Summary of Recommendations

| Priority | Recommendation | Effort | Impact |
|----------|----------------|--------|--------|
| **High** | Add editable text input next to all sliders | Medium | High |
| **High** | Bidirectional sync between slider and text | Medium | High |
| **Medium** | Logarithmic scale or presets for Rate Limit | Low | Medium |
| **Medium** | Stepper buttons for Gap (small integer range) | Low | Low |
| **Low** | Unit selector dropdown for rate/time values | Medium | Medium |
| **Low** | Create reusable SliderInput widget | High | Medium |

---

## Sources

- [NN/G: Slider Design Rules of Thumb](https://www.nngroup.com/articles/gui-slider-controls/)
- [Smashing Magazine: Designing The Perfect Slider](https://www.smashingmagazine.com/2017/07/designing-perfect-slider/)
- [Baymard: Slider Interface Requirements](https://baymard.com/blog/slider-interfaces)
- [Uxcel: Slider Best Practices](https://app.uxcel.com/courses/ui-components-n-patterns/sliders-best-practices-918)
- [SetProduct: Slider UI Design Guide](https://www.setproduct.com/blog/slider-ui-design)
- [iced-rs/iced GitHub](https://github.com/iced-rs/iced)
- [iced_aw: Additional Widgets](https://github.com/iced-rs/iced_aw)
- [Iced Widget Documentation](https://docs.rs/iced/latest/iced/widget/index.html)

---

## Conclusion

The dual-control pattern (slider + synchronized text input) is the industry-standard solution for accommodating both preference groups. It provides:

- **Visual users**: Quick exploration with sliders
- **Precision users**: Exact value entry with text
- **Accessibility**: Alternative input methods for all abilities
- **Power users**: Keyboard-driven workflow option

The implementation complexity is moderate, and the pattern can be rolled out incrementally starting with the most-used parameters.
