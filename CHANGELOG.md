# Changelog

All notable changes to this project will be documented in this file.

## [0.7.0] - 2026-01-18

### Changed

- **deps**: Update nlink from 0.6.0 to 0.8.0
  - Fixes namespace interface resolution bug where TC operations in network namespaces
    would fail because sysfs-based name resolution read from the host namespace
  - nlink 0.8.0 uses netlink-based resolution, making `namespace::connection_for()` 
    work correctly for all TC operations
  - API change: `get_qdiscs_for()` renamed to `get_qdiscs_by_name()`

## [0.6.0] - 2026-01-17

### Added

- **frontend**: Dual-control inputs with chips, slider, NumberInput for TC features
- **frontend**: Grid layout for feature cards using iced Grid widget
- **frontend**: Research-based presets for network condition simulation

### Changed

- **frontend**: Replaced dropdowns with expanded chips + slider for correlation values
- **frontend**: Improved text sizing and NumberInput proportions

### Fixed

- **frontend**: Feature cards now properly fill grid cell width
- **frontend**: Cards align to top with `align_y(Start)`
- **frontend**: Reduced card height with compact spacing
