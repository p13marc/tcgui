//! Enhanced TC command generation and templating system.
//!
//! This module provides comprehensive support for generating Linux traffic control
//! commands with validation, extensibility, and template-based patterns.

pub mod builder;
pub mod templates;

pub use builder::{
    HtbParams, NetemParams, PrioParams, QdiscParams, QdiscType, RedParams, SfqParams, TbfParams,
    TcCommand, TcCommandBuilder, TcOperation, TcTarget,
};
pub use templates::{
    CustomTemplate, PredefinedTemplate, TcTemplate, TemplateCategory, TemplateManager,
    TemplateParameter,
};
