# Emoji to SVG Icon Migration Plan

## Problem

Font-based emoji rendering is inconsistent across Linux systems. Different fonts, missing emoji fonts, or fallback rendering can cause emojis to display incorrectly or as placeholder boxes. This affects the UI appearance and usability.

## Solution

Replace all Unicode emoji characters with embedded SVG icons. SVGs render consistently regardless of system fonts and can be styled to match the application theme.

## Current Emoji Usage

The frontend uses **31 distinct emoji characters** across these files:

| File | Purpose |
|------|---------|
| `view.rs` | Main UI, tabs, status, theme toggle |
| `scenario_view.rs` | Scenario management, execution status, timeline |
| `interface/display/bandwidth.rs` | Bandwidth statistics |
| `interface/display/status.rs` | Interface status indicators |

### Emoji Categories

#### Status/State Indicators (6)
| Emoji | Usage | Proposed Icon |
|-------|-------|---------------|
| `🟢` | Ready state | Filled green circle |
| `⚡` | Applying/in progress | Lightning bolt |
| `✅` | Success | Checkmark in circle |
| `❌` | Error/failed | X in circle |
| `🔄` | Changing/reconnecting | Rotating arrows |
| `⚠️` | Warning/disconnected | Triangle with exclamation |

#### Navigation/UI Controls (7)
| Emoji | Usage | Proposed Icon |
|-------|-------|---------------|
| `🌐` | Interfaces tab | Globe |
| `📊` | Scenarios tab / charts | Bar chart |
| `🔍` | Zoom indicator | Magnifying glass |
| `👁` | Show/visibility on | Open eye |
| `🙈` | Hide/visibility off | Closed eye / eye with slash |
| `🌙` | Light mode toggle | Crescent moon |
| `☀️` | Dark mode toggle | Sun |

#### Container/Namespace Types (5)
| Emoji | Usage | Proposed Icon |
|-------|-------|---------------|
| `🏠` | Default namespace | House |
| `📁` | Traditional namespace | Folder |
| `🐳` | Docker container | Docker whale (or generic container) |
| `🦭` | Podman container | Podman seal (or generic container) |
| `📦` | Generic container | Box/package |

#### Playback Controls (9)
| Emoji | Usage | Proposed Icon |
|-------|-------|---------------|
| `▶️` / `▶` | Play/running | Play triangle |
| `⏸️` / `⏸` | Paused | Two vertical bars |
| `⏹️` / `⏹` | Stopped | Square |
| `✓` | Completed step | Checkmark |
| `✗` | Failed step | X mark |
| `○` | Pending step | Empty circle |
| `🔁` | Loop mode | Circular arrows |

#### Data/Activity (4)
| Emoji | Usage | Proposed Icon |
|-------|-------|---------------|
| `📈` | Rx rate / chart up | Line chart ascending |
| `📤` | Tx rate | Arrow pointing up from tray |
| `🚀` | Active interface | Rocket or activity pulse |
| `⏳` | Loading | Hourglass or spinner |

#### Labels/Sections (5)
| Emoji | Usage | Proposed Icon |
|-------|-------|---------------|
| `📡` | No interfaces message | Antenna/satellite dish |
| `📋` | Scenario list/details | Clipboard |
| `🖥️` | Backend header | Monitor/desktop |
| `🎮` | Active executions | Game controller or activity |
| `🎯` | Interface selection | Target/crosshair |
| `🔗` | Connected status | Chain link |

## Implementation Approach

### Option A: Iced's Built-in SVG Support

Iced has native SVG support via `iced::widget::svg`. We can:

1. Create an `icons/` directory with SVG files
2. Embed them at compile time using `include_bytes!`
3. Create an `Icon` enum with a method to render each icon

```rust
// tcgui-frontend/src/icons.rs
use iced::widget::svg::{Handle, Svg};
use iced::Length;

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    Globe,
    BarChart,
    CheckCircle,
    // ... etc
}

impl Icon {
    pub fn svg(self) -> Svg {
        let bytes: &'static [u8] = match self {
            Icon::Globe => include_bytes!("../icons/globe.svg"),
            Icon::BarChart => include_bytes!("../icons/bar-chart.svg"),
            Icon::CheckCircle => include_bytes!("../icons/check-circle.svg"),
            // ... etc
        };
        Svg::new(Handle::from_memory(bytes))
            .width(Length::Fixed(16.0))
            .height(Length::Fixed(16.0))
    }
}
```

### Option B: Icon Font (e.g., Material Icons, Feather Icons)

Use an icon font where each icon is a single character. This requires:
- Bundling the font file
- Loading it as a custom font in Iced
- Using specific Unicode code points for each icon

**Pros**: Single font file, easy color theming
**Cons**: Still a font (though more reliable), less flexibility in sizing

### Recommendation: Option A (SVG)

SVGs are the most reliable solution because:
1. No font dependencies at all
2. Vector graphics scale perfectly at any size
3. Can be themed by color filtering or using CSS-like styling
4. Each icon is self-contained
5. Easy to add/modify icons without font tools

## Icon Sources

Several open-source icon sets with compatible licenses:

| Icon Set | License | Style | URL |
|----------|---------|-------|-----|
| Heroicons | MIT | Clean, modern | https://heroicons.com |
| Lucide | ISC | Feather-like | https://lucide.dev |
| Tabler Icons | MIT | Consistent stroke | https://tabler-icons.io |
| Bootstrap Icons | MIT | Bootstrap style | https://icons.getbootstrap.com |

**Recommendation**: Lucide or Heroicons for their clean design and MIT license.

## Implementation Steps

### Phase 1: Setup (1 PR)
1. Create `tcgui-frontend/icons/` directory
2. Create `tcgui-frontend/src/icons.rs` module
3. Download initial set of SVG icons (from Lucide/Heroicons)
4. Add `Icon` enum with first few icons
5. Update `Cargo.toml` if needed for SVG feature

### Phase 2: Status Icons
Replace status-related emojis:
- `🟢` `⚡` `✅` `❌` `🔄` `⚠️`
- Files: `view.rs`, `status.rs`

### Phase 3: Navigation/UI Icons
Replace navigation emojis:
- `🌐` `📊` `🔍` `👁` `🙈` `🌙` `☀️`
- Files: `view.rs`

### Phase 4: Container Icons
Replace namespace/container emojis:
- `🏠` `📁` `🐳` `🦭` `📦`
- Files: `view.rs`

### Phase 5: Playback Controls
Replace playback emojis:
- `▶️` `⏸️` `⏹️` `✓` `✗` `○` `🔁`
- Files: `scenario_view.rs`

### Phase 6: Data/Section Icons
Replace remaining emojis:
- `📈` `📤` `🚀` `⏳` `📡` `📋` `🖥️` `🎮` `🎯` `🔗`
- Files: `scenario_view.rs`, `bandwidth.rs`

## SVG Requirements

Each SVG should:
- Be 24x24 viewBox (scalable to any size)
- Use `currentColor` for stroke/fill (allows theming)
- Be optimized (no unnecessary metadata)
- Have consistent stroke width (typically 2px at 24x24)

Example SVG structure:
```xml
<svg xmlns="http://www.w3.org/2000/svg" 
     width="24" height="24" 
     viewBox="0 0 24 24" 
     fill="none" 
     stroke="currentColor" 
     stroke-width="2" 
     stroke-linecap="round" 
     stroke-linejoin="round">
  <!-- paths here -->
</svg>
```

## Theme Support

For dark/light mode theming with SVGs:

```rust
impl Icon {
    pub fn svg_colored(self, color: Color) -> Svg {
        // Apply color filter or use themed SVG variant
        self.svg().style(move |_theme, _status| svg::Style {
            color: Some(color),
        })
    }
}
```

## File Structure After Migration

```
tcgui-frontend/
├── icons/
│   ├── globe.svg
│   ├── bar-chart.svg
│   ├── check-circle.svg
│   ├── x-circle.svg
│   ├── refresh.svg
│   ├── alert-triangle.svg
│   ├── eye.svg
│   ├── eye-off.svg
│   ├── moon.svg
│   ├── sun.svg
│   ├── home.svg
│   ├── folder.svg
│   ├── box.svg
│   ├── container.svg
│   ├── play.svg
│   ├── pause.svg
│   ├── stop.svg
│   ├── check.svg
│   ├── x.svg
│   ├── circle.svg
│   ├── repeat.svg
│   ├── trending-up.svg
│   ├── upload.svg
│   ├── activity.svg
│   ├── loader.svg
│   ├── radio.svg
│   ├── clipboard.svg
│   ├── monitor.svg
│   ├── gamepad.svg
│   ├── target.svg
│   └── link.svg
└── src/
    ├── icons.rs        # New module
    └── ...
```

## Next Steps

1. Confirm this approach works for you
2. Choose an icon set (I recommend Lucide)
3. I can create the icons module and download the SVGs
4. Migrate one file at a time, testing after each phase
