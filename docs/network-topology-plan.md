# Network Topology View Implementation Plan

## Overview

Add a visual network topology diagram showing interfaces, their relationships, namespaces, and TC configuration status. This provides a bird's-eye view of the network setup being managed.

## Current State

- Interfaces are displayed as a flat list grouped by namespace
- No visualization of network relationships (bridges, veth pairs, VLANs)
- No visual indication of traffic flow or connectivity
- Backend already discovers interfaces via rtnetlink with metadata

## Design Goals

1. **Visual Clarity**: Show interfaces as nodes, connections as edges
2. **Status at a Glance**: Color-coded TC status, up/down state
3. **Interactive**: Click nodes to select interface, hover for details
4. **Namespace Awareness**: Group or color-code by namespace
5. **Real-time Updates**: Reflect interface changes dynamically

## Architecture

### Data Model

```rust
// src/topology/mod.rs

/// Represents a node in the topology graph
#[derive(Debug, Clone)]
pub struct TopologyNode {
    pub id: String,                    // Unique identifier
    pub interface: NetworkInterface,   // From tcgui_shared
    pub namespace: String,
    pub position: Point,               // Calculated or user-adjusted
    pub node_type: NodeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Physical,      // eth0, ens33, etc.
    Virtual,       // veth, tun, tap
    Bridge,        // br0, docker0
    Loopback,      // lo
    Wireless,      // wlan0
    Unknown,
}

impl NodeType {
    pub fn from_interface(iface: &NetworkInterface) -> Self {
        let name = &iface.name;
        if name == "lo" {
            NodeType::Loopback
        } else if name.starts_with("br") || name.starts_with("docker") || name.starts_with("virbr") {
            NodeType::Bridge
        } else if name.starts_with("veth") || name.starts_with("tap") || name.starts_with("tun") {
            NodeType::Virtual
        } else if name.starts_with("wl") || name.starts_with("wlan") {
            NodeType::Wireless
        } else if name.starts_with("eth") || name.starts_with("ens") || name.starts_with("enp") {
            NodeType::Physical
        } else {
            NodeType::Unknown
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            NodeType::Physical => "🖥️",
            NodeType::Virtual => "🔗",
            NodeType::Bridge => "🌉",
            NodeType::Loopback => "🔄",
            NodeType::Wireless => "📶",
            NodeType::Unknown => "❓",
        }
    }
}

/// Represents a connection between nodes
#[derive(Debug, Clone)]
pub struct TopologyEdge {
    pub from: String,  // Node ID
    pub to: String,    // Node ID
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone, Copy)]
pub enum EdgeType {
    VethPair,      // veth endpoints
    BridgePort,    // Interface attached to bridge
    Parent,        // VLAN parent, bond member
}

/// Complete topology graph
#[derive(Debug, Clone, Default)]
pub struct TopologyGraph {
    pub nodes: HashMap<String, TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}
```

### Edge Detection

Detect relationships from interface metadata and naming conventions:

```rust
impl TopologyGraph {
    pub fn build_from_interfaces(
        interfaces: &HashMap<String, HashMap<String, NetworkInterface>>
    ) -> Self {
        let mut graph = Self::default();
        
        // First pass: create nodes
        for (namespace, ifaces) in interfaces {
            for (name, iface) in ifaces {
                let node = TopologyNode {
                    id: format!("{}:{}", namespace, name),
                    interface: iface.clone(),
                    namespace: namespace.clone(),
                    position: Point::ORIGIN,  // Will be calculated
                    node_type: NodeType::from_interface(iface),
                };
                graph.nodes.insert(node.id.clone(), node);
            }
        }
        
        // Second pass: detect edges
        graph.detect_veth_pairs();
        graph.detect_bridge_ports();
        
        // Calculate layout
        graph.calculate_layout();
        
        graph
    }
    
    fn detect_veth_pairs(&mut self) {
        // veth pairs often have naming patterns like:
        // - veth123 <-> eth0@if456
        // - vethXXX <-> vethYYY (peer index in metadata)
        // This requires parsing interface link info
    }
    
    fn detect_bridge_ports(&mut self) {
        // Interfaces with master = bridge name
        // Requires bridge port info from rtnetlink
    }
    
    fn calculate_layout(&mut self) {
        // Simple force-directed or hierarchical layout
        // Bridges at top, physical on sides, virtual in middle
    }
}
```

### Rendering with Iced Canvas

Use Iced's `Canvas` widget for custom drawing:

```rust
// src/topology/view.rs

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::{mouse, Color, Element, Length, Point, Rectangle, Size, Theme};

pub struct TopologyView {
    graph: TopologyGraph,
    selected_node: Option<String>,
    hovered_node: Option<String>,
    cache: canvas::Cache,
}

impl TopologyView {
    pub fn new() -> Self {
        Self {
            graph: TopologyGraph::default(),
            selected_node: None,
            hovered_node: None,
            cache: canvas::Cache::default(),
        }
    }
    
    pub fn update_graph(&mut self, graph: TopologyGraph) {
        self.graph = graph;
        self.cache.clear();  // Redraw needed
    }
    
    pub fn view(&self) -> Element<TopologyMessage> {
        Canvas::new(self)
            .width(Length::Fill)
            .height(Length::Fixed(400.0))
            .into()
    }
}

impl canvas::Program<TopologyMessage> for TopologyView {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            // Draw edges first (behind nodes)
            for edge in &self.graph.edges {
                self.draw_edge(frame, edge);
            }
            
            // Draw nodes
            for node in self.graph.nodes.values() {
                self.draw_node(frame, node);
            }
        });
        
        vec![geometry]
    }

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<TopologyMessage>) {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if let Some(node_id) = self.hit_test(position) {
                        return (
                            canvas::event::Status::Captured,
                            Some(TopologyMessage::NodeSelected(node_id)),
                        );
                    }
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(position) = cursor.position_in(bounds) {
                    let hovered = self.hit_test(position);
                    return (
                        canvas::event::Status::Captured,
                        Some(TopologyMessage::NodeHovered(hovered)),
                    );
                }
            }
            _ => {}
        }
        (canvas::event::Status::Ignored, None)
    }
}

impl TopologyView {
    fn draw_node(&self, frame: &mut Frame, node: &TopologyNode) {
        let radius = 25.0;
        let center = node.position;
        
        // Node circle
        let color = self.node_color(node);
        let circle = Path::circle(center, radius);
        frame.fill(&circle, color);
        
        // Border (thicker if selected)
        let stroke_width = if self.selected_node.as_ref() == Some(&node.id) {
            3.0
        } else {
            1.0
        };
        frame.stroke(
            &circle,
            Stroke::default().with_width(stroke_width).with_color(Color::BLACK),
        );
        
        // Label
        frame.fill_text(canvas::Text {
            content: node.interface.name.clone(),
            position: Point::new(center.x, center.y + radius + 12.0),
            color: Color::BLACK,
            size: 11.0.into(),
            horizontal_alignment: iced::alignment::Horizontal::Center,
            ..Default::default()
        });
    }
    
    fn draw_edge(&self, frame: &mut Frame, edge: &TopologyEdge) {
        let from = self.graph.nodes.get(&edge.from);
        let to = self.graph.nodes.get(&edge.to);
        
        if let (Some(from), Some(to)) = (from, to) {
            let path = Path::line(from.position, to.position);
            let color = match edge.edge_type {
                EdgeType::VethPair => Color::from_rgb(0.3, 0.6, 0.9),
                EdgeType::BridgePort => Color::from_rgb(0.5, 0.5, 0.5),
                EdgeType::Parent => Color::from_rgb(0.7, 0.7, 0.3),
            };
            frame.stroke(&path, Stroke::default().with_width(2.0).with_color(color));
        }
    }
    
    fn node_color(&self, node: &TopologyNode) -> Color {
        // Base color by type
        let base = match node.node_type {
            NodeType::Physical => Color::from_rgb(0.2, 0.6, 0.9),
            NodeType::Virtual => Color::from_rgb(0.6, 0.4, 0.8),
            NodeType::Bridge => Color::from_rgb(0.3, 0.7, 0.5),
            NodeType::Loopback => Color::from_rgb(0.5, 0.5, 0.5),
            NodeType::Wireless => Color::from_rgb(0.9, 0.6, 0.2),
            NodeType::Unknown => Color::from_rgb(0.7, 0.7, 0.7),
        };
        
        // Dim if interface is down
        if !node.interface.is_up {
            return Color::from_rgba(base.r * 0.5, base.g * 0.5, base.b * 0.5, 0.6);
        }
        
        // TODO: Add TC active indicator (border or glow)
        
        base
    }
    
    fn hit_test(&self, position: Point) -> Option<String> {
        let radius = 25.0;
        for (id, node) in &self.graph.nodes {
            let dx = position.x - node.position.x;
            let dy = position.y - node.position.y;
            if dx * dx + dy * dy <= radius * radius {
                return Some(id.clone());
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub enum TopologyMessage {
    NodeSelected(String),
    NodeHovered(Option<String>),
    Refresh,
}
```

## Layout Algorithms

### Option 1: Hierarchical Layout (Recommended)

```
     [Namespace: default]          [Namespace: container-ns]
           |                              |
        [br0]                          [eth0]
       /     \                            
   [eth0]  [veth1]--------------------[veth0]
```

- Bridges at top level
- Physical interfaces on edges
- Virtual interfaces connected to their peers
- Namespaces as visual grouping (boxes or colors)

### Option 2: Force-Directed Layout

- Nodes repel each other
- Edges act as springs
- Good for organic-looking graphs
- More complex to implement

### Option 3: Grid Layout

- Simple row/column arrangement
- Group by namespace in columns
- Less visual but very predictable

## Integration

### New Tab in UI

Add topology as a new tab alongside the interface list:

```rust
// ui_state.rs
pub enum AppTab {
    Interfaces,
    Scenarios,
    Topology,  // New
}

// view.rs
fn render_tabs(current: AppTab) -> Element<TcGuiMessage> {
    row![
        tab_button("Interfaces", AppTab::Interfaces, current),
        tab_button("Scenarios", AppTab::Scenarios, current),
        tab_button("Topology", AppTab::Topology, current),
    ]
}
```

### Messages

```rust
// messages.rs
pub enum TcGuiMessage {
    // ... existing
    TopologyMessage(TopologyMessage),
}

pub enum TopologyMessage {
    NodeSelected(String),      // namespace:interface
    NodeHovered(Option<String>),
    Refresh,
    ZoomIn,
    ZoomOut,
    ResetView,
}
```

## Implementation Phases

### Phase 1: Basic Structure (3-4 hours)

1. Create `src/topology/mod.rs` with data structures
2. Create `TopologyGraph::build_from_interfaces()`
3. Add simple grid layout algorithm
4. Basic canvas rendering (circles + lines)

### Phase 2: Visual Polish (2-3 hours)

1. Node icons/colors by type
2. Edge styling by type
3. Selected/hovered states
4. Interface labels

### Phase 3: Interaction (2-3 hours)

1. Click to select interface
2. Hover tooltips with details
3. Sync selection with interface list tab
4. Double-click to navigate to interface

### Phase 4: Advanced Features (Optional, 3-4 hours)

1. Zoom and pan controls
2. Force-directed layout option
3. Namespace grouping boxes
4. Real-time bandwidth flow animation
5. TC status indicators on nodes

## Files to Create/Modify

| File | Purpose |
|------|---------|
| `src/topology/mod.rs` | Module exports |
| `src/topology/graph.rs` | TopologyGraph, Node, Edge types |
| `src/topology/layout.rs` | Layout algorithms |
| `src/topology/view.rs` | Canvas rendering |
| `src/ui_state.rs` | Add Topology tab |
| `src/messages.rs` | Add TopologyMessage |
| `src/app.rs` | Handle topology messages |
| `src/view.rs` | Render topology tab |

## Backend Requirements

To properly detect edges (veth pairs, bridge ports), the backend may need to expose additional interface metadata:

```rust
// tcgui-shared NetworkInterface additions
pub struct NetworkInterface {
    // ... existing fields
    pub link_index: Option<u32>,      // Interface index
    pub peer_index: Option<u32>,      // For veth: peer's index
    pub master_index: Option<u32>,    // Bridge master index
    pub link_kind: Option<String>,    // "veth", "bridge", "tun", etc.
}
```

This requires rtnetlink query enhancements in the backend.

## Estimated Effort

- **Minimum viable** (static graph, no edges): 4-5 hours
- **With edge detection**: 6-8 hours
- **Full interactive**: 10-12 hours
- **With animations/advanced**: 15+ hours

## Dependencies

- Iced `canvas` feature (already available in iced 0.14)
- No external graph layout libraries needed for basic implementation

## Future Enhancements

- Drag nodes to custom positions
- Save/load layout preferences
- Export topology as SVG/PNG
- Show bandwidth flow as animated particles
- Highlight path between two interfaces
- Group by custom tags
