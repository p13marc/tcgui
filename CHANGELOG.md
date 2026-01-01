# Changelog

All notable changes to this project will be documented in this file.

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
