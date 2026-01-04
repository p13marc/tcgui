# Changelog

All notable changes to this project will be documented in this file.

## [0.6.0] - 2026-01-04

### Added
- TC diagnostic statistics in diagnostics output (drops, overlimits, qlen, backlog, bps/pps)
- Human-readable rate parsing for presets and scenarios (e.g., `rate: "10mbit"`)
- Full TC state capture for proper scenario rollback (stores actual netem config)
- TcDiagnosticStats type for TC qdisc effectiveness metrics

### Changed
- Upgraded nlink dependency from 0.5.0 to 0.6.0
- Gateway detection now uses nlink's get_routes() API instead of shell commands
- TC state restore now reapplies the original netem configuration instead of just clearing

### Removed
- Shell command dependency for gateway detection (`ip route show default`)

## [0.5.0] - 2026-01-04

### Added
- Carrier/link status indicator next to interface name (green when connected, dimmed when no carrier)
- Separate tracking of administrative state (is_up) vs operational state (is_oper_up)

### Changed
- Interface checkbox now clearly represents administrative state only
- NetworkInterface struct includes is_oper_up field for carrier detection

### Removed
- iced_anim dependency (was causing layout invalidation warnings on checkbox clicks)

### Fixed
- "More than 3 consecutive RedrawRequested events produced layout invalidation" warnings

## [0.4.0] - 2026-01-01

### Changed
- Migrated from rtnetlink to nlink for all netlink operations
- Use nlink 0.1.0 from crates.io instead of GitHub dependency
- Reduced bandwidth update log verbosity (info -> trace)

### Added
- TC qdisc statistics display using nlink API
- Multi-namespace event monitoring via NamespaceEventManager
- TC event monitoring via nlink EventStream
- Real-time bandwidth rate estimation using nlink

### Fixed
- Frontend-backend communication when only loopback is available
- Default localhost endpoints for local communication

## [0.3.0] - 2024-12-29

### Added
- Table view mode for interface list with compact 7-column layout (Interface, Namespace, Status, TC, RX, TX, Backend)
- View mode toggle button in header to switch between card and table views
- Animated background for interfaces with active TC qdisc using iced_anim
- Themed tooltip styling for better visibility in dark mode
- Smart scrollbar styling with hover/drag feedback
- Tooltip delays for TC parameter controls
- Column wrap for interface cards on wide screens

### Changed
- Improved dark mode support across all UI components

### Fixed
- Scenario control button icons visibility
- Feature control labels in dark mode
- Scenario view colors for dark mode
- Interface selection dialog colors for dark mode
- Checkbox and icon visibility in dark mode

## [0.2.0] - 2024-12-17

### Added
- Release workflow with Flatpak and Docker packaging
- Debian (.deb) and RPM package builds
- MIT license
- Rust 2024 edition support

### Changed
- Updated to Rust 1.92 in Dockerfile for edition 2024 support
- Added author and repository metadata to Cargo.toml
