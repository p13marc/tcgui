//! Command templating system for common TC command patterns.
//!
//! This module provides a comprehensive templating system for generating
//! common traffic control command patterns, allowing users to define
//! reusable templates with parameters that can be customized per use case.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::builder::{
    NetemParams, QdiscParams, QdiscType, TbfParams, TcCommand, TcCommandBuilder, TcOperation,
    TcTarget,
};

/// Template category for organizing templates
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TemplateCategory {
    /// Network emulation templates
    NetworkEmulation,
    /// Rate limiting templates
    RateLimiting,
    /// Quality of service templates
    QualityOfService,
    /// Congestion control templates
    CongestionControl,
    /// Testing and debugging templates
    Testing,
    /// Production optimization templates
    Production,
    /// Custom user-defined category
    Custom(String),
}

impl std::fmt::Display for TemplateCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateCategory::NetworkEmulation => write!(f, "Network Emulation"),
            TemplateCategory::RateLimiting => write!(f, "Rate Limiting"),
            TemplateCategory::QualityOfService => write!(f, "Quality of Service"),
            TemplateCategory::CongestionControl => write!(f, "Congestion Control"),
            TemplateCategory::Testing => write!(f, "Testing"),
            TemplateCategory::Production => write!(f, "Production"),
            TemplateCategory::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Template parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// Parameter name
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameter data type
    pub param_type: ParameterType,
    /// Default value (if any)
    pub default_value: Option<ParameterValue>,
    /// Whether the parameter is required
    pub required: bool,
    /// Validation constraints
    pub constraints: Option<ParameterConstraints>,
}

/// Parameter data types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterType {
    String,
    Integer,
    Float,
    Boolean,
    /// Enumeration with possible values
    Enum(Vec<String>),
}

/// Parameter value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

/// Parameter constraints for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterConstraints {
    /// Minimum value (for numeric types)
    pub min: Option<f64>,
    /// Maximum value (for numeric types)
    pub max: Option<f64>,
    /// Regular expression pattern (for strings)
    pub pattern: Option<String>,
    /// List of allowed values
    pub allowed_values: Option<Vec<String>>,
}

/// TC command template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcTemplate {
    /// Unique template identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Template description
    pub description: String,
    /// Template category
    pub category: TemplateCategory,
    /// Template parameters
    pub parameters: Vec<TemplateParameter>,
    /// Template command configuration
    pub command_template: CommandTemplate,
    /// Template version
    pub version: String,
    /// Template author/creator
    pub author: Option<String>,
    /// Template tags for searching
    pub tags: Vec<String>,
}

/// Command template configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTemplate {
    /// TC operation
    pub operation: TcOperation,
    /// Qdisc type
    pub qdisc_type: QdiscType,
    /// Command target
    pub target: TcTarget,
    /// Parameter mappings to qdisc parameters
    pub parameter_mappings: HashMap<String, String>,
    /// Whether to use sudo
    pub use_sudo: bool,
    /// Additional raw arguments with parameter placeholders
    pub raw_args: Vec<String>,
}

/// Predefined template definitions
#[derive(Debug, Clone)]
pub struct PredefinedTemplate;

impl PredefinedTemplate {
    /// Get all predefined templates
    pub fn get_all() -> Vec<TcTemplate> {
        vec![
            Self::mobile_device_simulation(),
            Self::wan_simulation(),
            Self::basic_rate_limiting(),
            Self::burst_rate_limiting(),
            Self::simple_packet_loss(),
            Self::complex_network_emulation(),
            Self::production_rate_limit(),
            Self::testing_high_latency(),
            Self::testing_packet_corruption(),
            Self::qos_priority_scheduling(),
        ]
    }

    /// Mobile device simulation template
    fn mobile_device_simulation() -> TcTemplate {
        TcTemplate {
            id: "mobile_device_sim".to_string(),
            name: "Mobile Device Simulation".to_string(),
            description: "Simulate mobile device network conditions with variable latency and loss"
                .to_string(),
            category: TemplateCategory::NetworkEmulation,
            parameters: vec![
                TemplateParameter {
                    name: "signal_strength".to_string(),
                    description: "Signal strength level (excellent, good, fair, poor)".to_string(),
                    param_type: ParameterType::Enum(vec![
                        "excellent".to_string(),
                        "good".to_string(),
                        "fair".to_string(),
                        "poor".to_string(),
                    ]),
                    default_value: Some(ParameterValue::String("good".to_string())),
                    required: false,
                    constraints: None,
                },
                TemplateParameter {
                    name: "mobility".to_string(),
                    description: "Device mobility (stationary, walking, driving)".to_string(),
                    param_type: ParameterType::Enum(vec![
                        "stationary".to_string(),
                        "walking".to_string(),
                        "driving".to_string(),
                    ]),
                    default_value: Some(ParameterValue::String("stationary".to_string())),
                    required: false,
                    constraints: None,
                },
            ],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([
                    (
                        "signal_strength".to_string(),
                        "loss_delay_mapping".to_string(),
                    ),
                    ("mobility".to_string(), "jitter_mapping".to_string()),
                ]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "mobile".to_string(),
                "simulation".to_string(),
                "wireless".to_string(),
            ],
        }
    }

    /// WAN simulation template
    fn wan_simulation() -> TcTemplate {
        TcTemplate {
            id: "wan_simulation".to_string(),
            name: "WAN Link Simulation".to_string(),
            description:
                "Simulate WAN link characteristics with configurable bandwidth and latency"
                    .to_string(),
            category: TemplateCategory::NetworkEmulation,
            parameters: vec![
                TemplateParameter {
                    name: "bandwidth_mbps".to_string(),
                    description: "Bandwidth limit in Mbps".to_string(),
                    param_type: ParameterType::Integer,
                    default_value: Some(ParameterValue::Integer(10)),
                    required: true,
                    constraints: Some(ParameterConstraints {
                        min: Some(1.0),
                        max: Some(1000.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "latency_ms".to_string(),
                    description: "Base latency in milliseconds".to_string(),
                    param_type: ParameterType::Integer,
                    default_value: Some(ParameterValue::Integer(50)),
                    required: true,
                    constraints: Some(ParameterConstraints {
                        min: Some(1.0),
                        max: Some(1000.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "jitter_ms".to_string(),
                    description: "Latency jitter in milliseconds".to_string(),
                    param_type: ParameterType::Integer,
                    default_value: Some(ParameterValue::Integer(10)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "loss_percent".to_string(),
                    description: "Packet loss percentage".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(0.1)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(10.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
            ],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([
                    ("bandwidth_mbps".to_string(), "rate_limit_kbps".to_string()),
                    ("latency_ms".to_string(), "delay_ms".to_string()),
                    ("jitter_ms".to_string(), "delay_jitter_ms".to_string()),
                    ("loss_percent".to_string(), "loss_percent".to_string()),
                ]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "wan".to_string(),
                "simulation".to_string(),
                "latency".to_string(),
            ],
        }
    }

    /// Basic rate limiting template
    fn basic_rate_limiting() -> TcTemplate {
        TcTemplate {
            id: "basic_rate_limit".to_string(),
            name: "Basic Rate Limiting".to_string(),
            description: "Simple bandwidth rate limiting using TBF".to_string(),
            category: TemplateCategory::RateLimiting,
            parameters: vec![
                TemplateParameter {
                    name: "rate".to_string(),
                    description: "Rate limit (e.g., '1mbit', '500kbit')".to_string(),
                    param_type: ParameterType::String,
                    default_value: Some(ParameterValue::String("1mbit".to_string())),
                    required: true,
                    constraints: Some(ParameterConstraints {
                        min: None,
                        max: None,
                        pattern: Some(r"^\d+[kmg]?bit$".to_string()),
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "burst".to_string(),
                    description: "Burst size (e.g., '32kbit', '1600b')".to_string(),
                    param_type: ParameterType::String,
                    default_value: Some(ParameterValue::String("32kbit".to_string())),
                    required: false,
                    constraints: None,
                },
            ],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Tbf,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([
                    ("rate".to_string(), "rate".to_string()),
                    ("burst".to_string(), "burst".to_string()),
                ]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec!["rate".to_string(), "limit".to_string(), "tbf".to_string()],
        }
    }

    /// Burst rate limiting template
    fn burst_rate_limiting() -> TcTemplate {
        TcTemplate {
            id: "burst_rate_limit".to_string(),
            name: "Burst Rate Limiting".to_string(),
            description: "Advanced rate limiting with burst and peak rate control".to_string(),
            category: TemplateCategory::RateLimiting,
            parameters: vec![
                TemplateParameter {
                    name: "rate".to_string(),
                    description: "Sustained rate limit".to_string(),
                    param_type: ParameterType::String,
                    default_value: Some(ParameterValue::String("1mbit".to_string())),
                    required: true,
                    constraints: None,
                },
                TemplateParameter {
                    name: "burst".to_string(),
                    description: "Burst buffer size".to_string(),
                    param_type: ParameterType::String,
                    default_value: Some(ParameterValue::String("32kbit".to_string())),
                    required: true,
                    constraints: None,
                },
                TemplateParameter {
                    name: "peakrate".to_string(),
                    description: "Peak rate limit".to_string(),
                    param_type: ParameterType::String,
                    default_value: Some(ParameterValue::String("2mbit".to_string())),
                    required: false,
                    constraints: None,
                },
                TemplateParameter {
                    name: "limit".to_string(),
                    description: "Queue limit in bytes".to_string(),
                    param_type: ParameterType::String,
                    default_value: Some(ParameterValue::String("3000b".to_string())),
                    required: false,
                    constraints: None,
                },
            ],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Tbf,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([
                    ("rate".to_string(), "rate".to_string()),
                    ("burst".to_string(), "burst".to_string()),
                    ("peakrate".to_string(), "peakrate".to_string()),
                    ("limit".to_string(), "limit".to_string()),
                ]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "rate".to_string(),
                "burst".to_string(),
                "advanced".to_string(),
            ],
        }
    }

    /// Simple packet loss template
    fn simple_packet_loss() -> TcTemplate {
        TcTemplate {
            id: "simple_packet_loss".to_string(),
            name: "Simple Packet Loss".to_string(),
            description: "Basic packet loss simulation for testing".to_string(),
            category: TemplateCategory::Testing,
            parameters: vec![
                TemplateParameter {
                    name: "loss_percent".to_string(),
                    description: "Packet loss percentage (0-100)".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(1.0)),
                    required: true,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "correlation".to_string(),
                    description: "Loss correlation percentage (0-100)".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(25.0)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
            ],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([
                    ("loss_percent".to_string(), "loss_percent".to_string()),
                    ("correlation".to_string(), "loss_correlation".to_string()),
                ]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "loss".to_string(),
                "testing".to_string(),
                "simple".to_string(),
            ],
        }
    }

    /// Complex network emulation template
    fn complex_network_emulation() -> TcTemplate {
        TcTemplate {
            id: "complex_netem".to_string(),
            name: "Complex Network Emulation".to_string(),
            description: "Full-featured network emulation with all netem parameters".to_string(),
            category: TemplateCategory::NetworkEmulation,
            parameters: vec![
                TemplateParameter {
                    name: "delay_ms".to_string(),
                    description: "Base delay in milliseconds".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(100.0)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(5000.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "jitter_ms".to_string(),
                    description: "Delay jitter in milliseconds".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(10.0)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(1000.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "loss_percent".to_string(),
                    description: "Packet loss percentage".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(1.0)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "duplicate_percent".to_string(),
                    description: "Packet duplication percentage".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(0.1)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "reorder_percent".to_string(),
                    description: "Packet reordering percentage".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(0.5)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
                TemplateParameter {
                    name: "corrupt_percent".to_string(),
                    description: "Packet corruption percentage".to_string(),
                    param_type: ParameterType::Float,
                    default_value: Some(ParameterValue::Float(0.02)),
                    required: false,
                    constraints: Some(ParameterConstraints {
                        min: Some(0.0),
                        max: Some(100.0),
                        pattern: None,
                        allowed_values: None,
                    }),
                },
            ],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([
                    ("delay_ms".to_string(), "delay_ms".to_string()),
                    ("jitter_ms".to_string(), "delay_jitter_ms".to_string()),
                    ("loss_percent".to_string(), "loss_percent".to_string()),
                    (
                        "duplicate_percent".to_string(),
                        "duplicate_percent".to_string(),
                    ),
                    ("reorder_percent".to_string(), "reorder_percent".to_string()),
                    ("corrupt_percent".to_string(), "corrupt_percent".to_string()),
                ]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "complex".to_string(),
                "netem".to_string(),
                "full".to_string(),
            ],
        }
    }

    /// Production rate limiting template
    fn production_rate_limit() -> TcTemplate {
        TcTemplate {
            id: "production_rate_limit".to_string(),
            name: "Production Rate Limiting".to_string(),
            description: "Production-ready rate limiting with conservative settings".to_string(),
            category: TemplateCategory::Production,
            parameters: vec![TemplateParameter {
                name: "rate".to_string(),
                description: "Maximum rate (e.g., '100mbit')".to_string(),
                param_type: ParameterType::String,
                default_value: Some(ParameterValue::String("100mbit".to_string())),
                required: true,
                constraints: None,
            }],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Tbf,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([("rate".to_string(), "rate".to_string())]),
                use_sudo: true,
                raw_args: vec![
                    "burst".to_string(),
                    "128kbit".to_string(),
                    "limit".to_string(),
                    "10000b".to_string(),
                ],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "production".to_string(),
                "stable".to_string(),
                "rate".to_string(),
            ],
        }
    }

    /// High latency testing template
    fn testing_high_latency() -> TcTemplate {
        TcTemplate {
            id: "testing_high_latency".to_string(),
            name: "High Latency Testing".to_string(),
            description: "Simulate high-latency connections for testing".to_string(),
            category: TemplateCategory::Testing,
            parameters: vec![TemplateParameter {
                name: "latency_type".to_string(),
                description: "Type of high latency connection".to_string(),
                param_type: ParameterType::Enum(vec![
                    "satellite".to_string(),
                    "intercontinental".to_string(),
                    "extreme".to_string(),
                ]),
                default_value: Some(ParameterValue::String("satellite".to_string())),
                required: true,
                constraints: None,
            }],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([(
                    "latency_type".to_string(),
                    "latency_preset".to_string(),
                )]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "testing".to_string(),
                "latency".to_string(),
                "high".to_string(),
            ],
        }
    }

    /// Packet corruption testing template
    fn testing_packet_corruption() -> TcTemplate {
        TcTemplate {
            id: "testing_packet_corruption".to_string(),
            name: "Packet Corruption Testing".to_string(),
            description: "Test application resilience with packet corruption".to_string(),
            category: TemplateCategory::Testing,
            parameters: vec![TemplateParameter {
                name: "corruption_level".to_string(),
                description: "Corruption severity level".to_string(),
                param_type: ParameterType::Enum(vec![
                    "light".to_string(),
                    "moderate".to_string(),
                    "heavy".to_string(),
                ]),
                default_value: Some(ParameterValue::String("light".to_string())),
                required: true,
                constraints: None,
            }],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([(
                    "corruption_level".to_string(),
                    "corruption_preset".to_string(),
                )]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "testing".to_string(),
                "corruption".to_string(),
                "resilience".to_string(),
            ],
        }
    }

    /// QoS priority scheduling template
    fn qos_priority_scheduling() -> TcTemplate {
        TcTemplate {
            id: "qos_priority".to_string(),
            name: "QoS Priority Scheduling".to_string(),
            description: "Priority-based quality of service scheduling".to_string(),
            category: TemplateCategory::QualityOfService,
            parameters: vec![TemplateParameter {
                name: "bands".to_string(),
                description: "Number of priority bands (2-16)".to_string(),
                param_type: ParameterType::Integer,
                default_value: Some(ParameterValue::Integer(3)),
                required: true,
                constraints: Some(ParameterConstraints {
                    min: Some(2.0),
                    max: Some(16.0),
                    pattern: None,
                    allowed_values: None,
                }),
            }],
            command_template: CommandTemplate {
                operation: TcOperation::Replace,
                qdisc_type: QdiscType::Prio,
                target: TcTarget::Root,
                parameter_mappings: HashMap::from([("bands".to_string(), "bands".to_string())]),
                use_sudo: true,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("TC GUI Team".to_string()),
            tags: vec![
                "qos".to_string(),
                "priority".to_string(),
                "scheduling".to_string(),
            ],
        }
    }
}

/// Custom template definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomTemplate {
    /// Template definition
    pub template: TcTemplate,
    /// Creation timestamp
    pub created_at: std::time::SystemTime,
    /// Last modified timestamp
    pub modified_at: std::time::SystemTime,
}

/// Template manager for handling template operations
pub struct TemplateManager {
    /// Predefined templates
    predefined_templates: HashMap<String, TcTemplate>,
    /// User-defined custom templates
    custom_templates: HashMap<String, CustomTemplate>,
}

impl TemplateManager {
    /// Create a new template manager
    pub fn new() -> Self {
        let predefined = PredefinedTemplate::get_all();
        let mut predefined_templates = HashMap::new();

        for template in predefined {
            predefined_templates.insert(template.id.clone(), template);
        }

        Self {
            predefined_templates,
            custom_templates: HashMap::new(),
        }
    }

    /// Get all available templates
    pub fn get_all_templates(&self) -> Vec<&TcTemplate> {
        let mut templates: Vec<&TcTemplate> = Vec::new();

        // Add predefined templates
        templates.extend(self.predefined_templates.values());

        // Add custom templates
        templates.extend(self.custom_templates.values().map(|ct| &ct.template));

        templates
    }

    /// Get templates by category
    pub fn get_templates_by_category(&self, category: &TemplateCategory) -> Vec<&TcTemplate> {
        self.get_all_templates()
            .into_iter()
            .filter(|template| &template.category == category)
            .collect()
    }

    /// Search templates by tags
    pub fn search_templates(&self, query: &str) -> Vec<&TcTemplate> {
        let query_lower = query.to_lowercase();
        self.get_all_templates()
            .into_iter()
            .filter(|template| {
                template.name.to_lowercase().contains(&query_lower)
                    || template.description.to_lowercase().contains(&query_lower)
                    || template
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Get template by ID
    pub fn get_template(&self, id: &str) -> Option<&TcTemplate> {
        self.predefined_templates
            .get(id)
            .or_else(|| self.custom_templates.get(id).map(|ct| &ct.template))
    }

    /// Add custom template
    pub fn add_custom_template(&mut self, template: TcTemplate) -> Result<()> {
        if self.predefined_templates.contains_key(&template.id) {
            return Err(anyhow!(
                "Template ID '{}' conflicts with predefined template",
                template.id
            ));
        }

        let now = std::time::SystemTime::now();
        let template_id = template.id.clone();
        let custom_template = CustomTemplate {
            template,
            created_at: now,
            modified_at: now,
        };

        self.custom_templates
            .insert(template_id.clone(), custom_template);
        info!("Added custom template: {}", template_id);
        Ok(())
    }

    /// Update custom template
    pub fn update_custom_template(&mut self, id: &str, template: TcTemplate) -> Result<()> {
        if let Some(custom_template) = self.custom_templates.get_mut(id) {
            custom_template.template = template;
            custom_template.modified_at = std::time::SystemTime::now();
            info!("Updated custom template: {}", id);
            Ok(())
        } else {
            Err(anyhow!("Custom template '{}' not found", id))
        }
    }

    /// Remove custom template
    pub fn remove_custom_template(&mut self, id: &str) -> Result<()> {
        if self.custom_templates.remove(id).is_some() {
            info!("Removed custom template: {}", id);
            Ok(())
        } else {
            Err(anyhow!("Custom template '{}' not found", id))
        }
    }

    /// Generate command from template
    pub fn generate_command_from_template(
        &self,
        template_id: &str,
        device: &str,
        namespace: Option<&str>,
        parameter_values: HashMap<String, ParameterValue>,
    ) -> Result<TcCommand> {
        let template = self
            .get_template(template_id)
            .ok_or_else(|| anyhow!("Template '{}' not found", template_id))?;

        // Validate required parameters
        for param in &template.parameters {
            if param.required && !parameter_values.contains_key(&param.name) {
                return Err(anyhow!("Required parameter '{}' not provided", param.name));
            }
        }

        // Validate parameter values
        for (name, value) in &parameter_values {
            if let Some(param) = template.parameters.iter().find(|p| &p.name == name) {
                self.validate_parameter_value(param, value)?;
            } else {
                warn!(
                    "Unknown parameter '{}' provided for template '{}'",
                    name, template_id
                );
            }
        }

        // Build the TC command
        let mut builder = TcCommandBuilder::new()
            .operation(template.command_template.operation.clone())
            .device(device)
            .qdisc(template.command_template.qdisc_type.clone())
            .target(template.command_template.target.clone());

        if template.command_template.use_sudo {
            builder = builder.with_sudo();
        }

        if let Some(ns) = namespace {
            builder = builder.namespace(ns);
        }

        // Add raw arguments
        if !template.command_template.raw_args.is_empty() {
            builder = builder.raw_args(template.command_template.raw_args.clone());
        }

        // Convert parameters to qdisc parameters
        let qdisc_params = self.convert_parameters_to_qdisc_params(
            &template.command_template.qdisc_type,
            &template.command_template.parameter_mappings,
            &parameter_values,
            template,
        )?;

        if let Some(params) = qdisc_params {
            builder = builder.params(params);
        }

        let command = builder.build()?;
        debug!(
            "Generated command from template '{}': {:?}",
            template_id, command
        );

        Ok(command)
    }

    /// Validate parameter value against parameter definition
    fn validate_parameter_value(
        &self,
        param: &TemplateParameter,
        value: &ParameterValue,
    ) -> Result<()> {
        match (&param.param_type, value) {
            (ParameterType::String, ParameterValue::String(s)) => {
                if let Some(ref constraints) = param.constraints {
                    if let Some(ref pattern) = constraints.pattern {
                        let regex = regex::Regex::new(pattern)
                            .map_err(|e| anyhow!("Invalid regex pattern '{}': {}", pattern, e))?;
                        if !regex.is_match(s) {
                            return Err(anyhow!(
                                "Parameter '{}' value '{}' does not match pattern '{}'",
                                param.name,
                                s,
                                pattern
                            ));
                        }
                    }
                    if let Some(ref allowed) = constraints.allowed_values
                        && !allowed.contains(s)
                    {
                        return Err(anyhow!(
                            "Parameter '{}' value '{}' is not in allowed values: {:?}",
                            param.name,
                            s,
                            allowed
                        ));
                    }
                }
            }
            (ParameterType::Integer, ParameterValue::Integer(i)) => {
                if let Some(ref constraints) = param.constraints {
                    let value_f64 = *i as f64;
                    if let Some(min) = constraints.min
                        && value_f64 < min
                    {
                        return Err(anyhow!(
                            "Parameter '{}' value {} is less than minimum {}",
                            param.name,
                            i,
                            min
                        ));
                    }
                    if let Some(max) = constraints.max
                        && value_f64 > max
                    {
                        return Err(anyhow!(
                            "Parameter '{}' value {} is greater than maximum {}",
                            param.name,
                            i,
                            max
                        ));
                    }
                }
            }
            (ParameterType::Float, ParameterValue::Float(f)) => {
                if let Some(ref constraints) = param.constraints {
                    if let Some(min) = constraints.min
                        && *f < min
                    {
                        return Err(anyhow!(
                            "Parameter '{}' value {} is less than minimum {}",
                            param.name,
                            f,
                            min
                        ));
                    }
                    if let Some(max) = constraints.max
                        && *f > max
                    {
                        return Err(anyhow!(
                            "Parameter '{}' value {} is greater than maximum {}",
                            param.name,
                            f,
                            max
                        ));
                    }
                }
            }
            (ParameterType::Boolean, ParameterValue::Boolean(_)) => {
                // Boolean values are always valid
            }
            (ParameterType::Enum(allowed_values), ParameterValue::String(s)) => {
                if !allowed_values.contains(s) {
                    return Err(anyhow!(
                        "Parameter '{}' value '{}' is not in allowed enum values: {:?}",
                        param.name,
                        s,
                        allowed_values
                    ));
                }
            }
            _ => {
                return Err(anyhow!(
                    "Parameter '{}' type mismatch: expected {:?}, got {:?}",
                    param.name,
                    param.param_type,
                    value
                ));
            }
        }
        Ok(())
    }

    /// Convert template parameters to qdisc parameters
    fn convert_parameters_to_qdisc_params(
        &self,
        qdisc_type: &QdiscType,
        parameter_mappings: &HashMap<String, String>,
        parameter_values: &HashMap<String, ParameterValue>,
        template: &TcTemplate,
    ) -> Result<Option<QdiscParams>> {
        match qdisc_type {
            QdiscType::Netem => {
                let mut netem_params = NetemParams::default();

                for (param_name, param_value) in parameter_values {
                    if let Some(mapping) = parameter_mappings.get(param_name) {
                        self.apply_netem_parameter(
                            &mut netem_params,
                            mapping,
                            param_value,
                            template,
                        )?;
                    }
                }

                Ok(Some(QdiscParams::Netem(netem_params)))
            }
            QdiscType::Tbf => {
                let mut tbf_params = TbfParams {
                    rate: "1mbit".to_string(),
                    burst: None,
                    limit: None,
                    peakrate: None,
                    mtu: None,
                };

                for (param_name, param_value) in parameter_values {
                    if let Some(mapping) = parameter_mappings.get(param_name) {
                        self.apply_tbf_parameter(&mut tbf_params, mapping, param_value)?;
                    }
                }

                Ok(Some(QdiscParams::Tbf(tbf_params)))
            }
            _ => {
                // For other qdisc types, return None for now
                Ok(None)
            }
        }
    }

    /// Apply parameter to netem configuration
    fn apply_netem_parameter(
        &self,
        netem_params: &mut NetemParams,
        mapping: &str,
        value: &ParameterValue,
        _template: &TcTemplate,
    ) -> Result<()> {
        match mapping {
            "delay_ms" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.delay_ms = Some(*f as f32);
                } else if let ParameterValue::Integer(i) = value {
                    netem_params.delay_ms = Some(*i as f32);
                }
            }
            "delay_jitter_ms" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.delay_jitter_ms = Some(*f as f32);
                } else if let ParameterValue::Integer(i) = value {
                    netem_params.delay_jitter_ms = Some(*i as f32);
                }
            }
            "loss_percent" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.loss_percent = Some(*f as f32);
                } else if let ParameterValue::Integer(i) = value {
                    netem_params.loss_percent = Some(*i as f32);
                }
            }
            "loss_correlation" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.loss_correlation = Some(*f as f32);
                }
            }
            "duplicate_percent" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.duplicate_percent = Some(*f as f32);
                }
            }
            "reorder_percent" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.reorder_percent = Some(*f as f32);
                }
            }
            "corrupt_percent" => {
                if let ParameterValue::Float(f) = value {
                    netem_params.corrupt_percent = Some(*f as f32);
                }
            }
            "rate_limit_kbps" => {
                if let ParameterValue::Integer(i) = value {
                    netem_params.rate_limit_kbps = Some(*i as u32);
                }
            }
            // Handle special preset mappings
            "loss_delay_mapping" => {
                if let ParameterValue::String(signal_strength) = value {
                    self.apply_signal_strength_preset(netem_params, signal_strength)?;
                }
            }
            "latency_preset" => {
                if let ParameterValue::String(latency_type) = value {
                    self.apply_latency_preset(netem_params, latency_type)?;
                }
            }
            "corruption_preset" => {
                if let ParameterValue::String(corruption_level) = value {
                    self.apply_corruption_preset(netem_params, corruption_level)?;
                }
            }
            _ => {
                warn!("Unknown netem parameter mapping: {}", mapping);
            }
        }
        Ok(())
    }

    /// Apply parameter to TBF configuration
    fn apply_tbf_parameter(
        &self,
        tbf_params: &mut TbfParams,
        mapping: &str,
        value: &ParameterValue,
    ) -> Result<()> {
        match mapping {
            "rate" => {
                if let ParameterValue::String(s) = value {
                    tbf_params.rate = s.clone();
                }
            }
            "burst" => {
                if let ParameterValue::String(s) = value {
                    tbf_params.burst = Some(s.clone());
                }
            }
            "limit" => {
                if let ParameterValue::String(s) = value {
                    tbf_params.limit = Some(s.clone());
                }
            }
            "peakrate" => {
                if let ParameterValue::String(s) = value {
                    tbf_params.peakrate = Some(s.clone());
                }
            }
            "mtu" => {
                if let ParameterValue::String(s) = value {
                    tbf_params.mtu = Some(s.clone());
                }
            }
            _ => {
                warn!("Unknown TBF parameter mapping: {}", mapping);
            }
        }
        Ok(())
    }

    /// Apply signal strength preset to netem parameters
    fn apply_signal_strength_preset(
        &self,
        netem_params: &mut NetemParams,
        signal_strength: &str,
    ) -> Result<()> {
        match signal_strength {
            "excellent" => {
                netem_params.delay_ms = Some(5.0);
                netem_params.delay_jitter_ms = Some(1.0);
                netem_params.loss_percent = Some(0.01);
            }
            "good" => {
                netem_params.delay_ms = Some(20.0);
                netem_params.delay_jitter_ms = Some(5.0);
                netem_params.loss_percent = Some(0.1);
            }
            "fair" => {
                netem_params.delay_ms = Some(100.0);
                netem_params.delay_jitter_ms = Some(50.0);
                netem_params.loss_percent = Some(1.0);
                netem_params.loss_correlation = Some(25.0);
            }
            "poor" => {
                netem_params.delay_ms = Some(300.0);
                netem_params.delay_jitter_ms = Some(150.0);
                netem_params.loss_percent = Some(5.0);
                netem_params.loss_correlation = Some(50.0);
            }
            _ => {
                return Err(anyhow!(
                    "Unknown signal strength preset: {}",
                    signal_strength
                ));
            }
        }
        Ok(())
    }

    /// Apply latency preset to netem parameters
    fn apply_latency_preset(
        &self,
        netem_params: &mut NetemParams,
        latency_type: &str,
    ) -> Result<()> {
        match latency_type {
            "satellite" => {
                netem_params.delay_ms = Some(600.0);
                netem_params.delay_jitter_ms = Some(50.0);
            }
            "intercontinental" => {
                netem_params.delay_ms = Some(200.0);
                netem_params.delay_jitter_ms = Some(20.0);
            }
            "extreme" => {
                netem_params.delay_ms = Some(2000.0);
                netem_params.delay_jitter_ms = Some(500.0);
            }
            _ => {
                return Err(anyhow!("Unknown latency preset: {}", latency_type));
            }
        }
        Ok(())
    }

    /// Apply corruption preset to netem parameters
    fn apply_corruption_preset(
        &self,
        netem_params: &mut NetemParams,
        corruption_level: &str,
    ) -> Result<()> {
        match corruption_level {
            "light" => {
                netem_params.corrupt_percent = Some(0.01);
            }
            "moderate" => {
                netem_params.corrupt_percent = Some(0.1);
                netem_params.corrupt_correlation = Some(10.0);
            }
            "heavy" => {
                netem_params.corrupt_percent = Some(1.0);
                netem_params.corrupt_correlation = Some(50.0);
            }
            _ => {
                return Err(anyhow!("Unknown corruption preset: {}", corruption_level));
            }
        }
        Ok(())
    }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_manager_creation() {
        let manager = TemplateManager::new();
        let templates = manager.get_all_templates();
        assert!(!templates.is_empty());

        // Should have all predefined templates
        assert!(templates.len() >= 10);
    }

    #[test]
    fn test_search_templates() {
        let manager = TemplateManager::new();

        let results = manager.search_templates("mobile");
        assert!(!results.is_empty());
        assert!(results.iter().any(|t| t.name.contains("Mobile")));

        let results = manager.search_templates("rate");
        assert!(!results.is_empty());
        assert!(results.iter().any(|t| t.tags.contains(&"rate".to_string())));
    }

    #[test]
    fn test_templates_by_category() {
        let manager = TemplateManager::new();

        let netem_templates =
            manager.get_templates_by_category(&TemplateCategory::NetworkEmulation);
        assert!(!netem_templates.is_empty());

        let rate_templates = manager.get_templates_by_category(&TemplateCategory::RateLimiting);
        assert!(!rate_templates.is_empty());
    }

    #[test]
    fn test_generate_command_from_template() {
        let manager = TemplateManager::new();

        let mut params = HashMap::new();
        params.insert("loss_percent".to_string(), ParameterValue::Float(5.0));
        params.insert("correlation".to_string(), ParameterValue::Float(25.0));

        let result =
            manager.generate_command_from_template("simple_packet_loss", "eth0", None, params);

        assert!(result.is_ok());
        let command = result.unwrap();
        let args = command.to_args();
        assert!(args.contains(&"netem".to_string()));
        assert!(args.contains(&"loss".to_string()));
        assert!(args.contains(&"5%".to_string()));
    }

    #[test]
    fn test_parameter_validation() {
        let manager = TemplateManager::new();

        // Valid parameters should work
        let mut params = HashMap::new();
        params.insert("bandwidth_mbps".to_string(), ParameterValue::Integer(10));
        params.insert("latency_ms".to_string(), ParameterValue::Integer(50));

        let result = manager.generate_command_from_template("wan_simulation", "eth0", None, params);
        assert!(result.is_ok());

        // Invalid parameters should fail
        let mut invalid_params = HashMap::new();
        invalid_params.insert("bandwidth_mbps".to_string(), ParameterValue::Integer(2000)); // > max

        let result =
            manager.generate_command_from_template("wan_simulation", "eth0", None, invalid_params);
        assert!(result.is_err());
    }

    #[test]
    fn test_custom_template_management() {
        let mut manager = TemplateManager::new();

        let custom_template = TcTemplate {
            id: "custom_test".to_string(),
            name: "Custom Test Template".to_string(),
            description: "A test template".to_string(),
            category: TemplateCategory::Custom("Test".to_string()),
            parameters: vec![],
            command_template: CommandTemplate {
                operation: TcOperation::Add,
                qdisc_type: QdiscType::Netem,
                target: TcTarget::Root,
                parameter_mappings: HashMap::new(),
                use_sudo: false,
                raw_args: vec![],
            },
            version: "1.0".to_string(),
            author: Some("Test User".to_string()),
            tags: vec!["test".to_string()],
        };

        // Add custom template
        assert!(manager.add_custom_template(custom_template.clone()).is_ok());

        // Should be able to find it
        assert!(manager.get_template("custom_test").is_some());

        // Remove custom template
        assert!(manager.remove_custom_template("custom_test").is_ok());
        assert!(manager.get_template("custom_test").is_none());
    }
}
