use thiserror::Error;

/// Common error types for tcgui application
#[derive(Error, Debug)]
pub enum TcguiError {
    #[error("Zenoh communication error: {message}")]
    ZenohError { message: String },

    #[error("Network operation error: {message}")]
    NetworkError { message: String },

    #[error("Traffic control command failed: {message}")]
    TcCommandError { message: String },

    #[error("Interface not found: {interface}")]
    InterfaceNotFound { interface: String },

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),
}

/// Backend-specific errors
#[derive(Error, Debug)]
pub enum BackendError {
    #[error("Failed to initialize backend: {message}")]
    InitializationError { message: String },

    #[error("Netlink error: {0}")]
    NlinkError(#[from] nlink::netlink::Error),

    #[error("Failed to read network statistics: {0}")]
    NetworkStatsError(#[from] std::io::Error),

    #[error("Network error: {message}")]
    NetworkError { message: String },

    #[error("Common error: {0}")]
    Common(#[from] TcguiError),
}

/// Frontend-specific errors
#[derive(Error, Debug)]
pub enum FrontendError {
    #[error("GUI initialization error: {message}")]
    GuiInitError { message: String },

    #[error("Zenoh subscription error: {message}")]
    SubscriptionError { message: String },

    #[error("Common error: {0}")]
    Common(#[from] TcguiError),
}

/// Zenoh configuration specific errors
#[derive(Error, Debug)]
pub enum ZenohConfigError {
    #[error("Invalid zenoh mode: expected 'peer' or 'client', got '{mode}'")]
    InvalidMode { mode: String },

    #[error("Invalid endpoint format: '{endpoint}' - {reason}")]
    InvalidEndpoint { endpoint: String, reason: String },

    #[error("Unsupported protocol: '{protocol}' in endpoint '{endpoint}'")]
    InvalidProtocol { protocol: String, endpoint: String },

    #[error("Invalid address format: '{address}' for protocol '{protocol}' - {reason}")]
    InvalidAddress {
        address: String,
        protocol: String,
        reason: String,
    },

    #[error("Mode-endpoint mismatch: {mode:?} mode {reason}")]
    ModeEndpointMismatch {
        mode: crate::ZenohMode,
        reason: String,
    },

    #[error("Configuration validation failed: {message}")]
    ValidationError { message: String },

    #[error("Failed to create zenoh config: {reason}")]
    ZenohConfigCreationError { reason: String },

    #[error("Property configuration error: failed to set '{key}' = '{value}': {reason}")]
    PropertyError {
        key: String,
        value: String,
        reason: String,
    },

    #[error("Endpoint parsing error: '{input}' could not be parsed as a valid endpoint")]
    EndpointParsingError { input: String },
}

impl ZenohConfigError {
    /// Create an InvalidAddress error with automatic reason from parsing failure
    pub fn invalid_address_from_parse_error(
        address: &str,
        protocol: &str,
        parse_err: &dyn std::error::Error,
    ) -> Self {
        ZenohConfigError::InvalidAddress {
            address: address.to_string(),
            protocol: protocol.to_string(),
            reason: parse_err.to_string(),
        }
    }

    /// Create a ModeEndpointMismatch error for client mode with listen endpoints
    pub fn client_cannot_listen() -> Self {
        ZenohConfigError::ModeEndpointMismatch {
            mode: crate::ZenohMode::Client,
            reason: "cannot have listen endpoints. Clients can only connect to other nodes"
                .to_string(),
        }
    }

    /// Create an InvalidEndpoint error with a descriptive reason
    pub fn unsupported_endpoint_format(endpoint: &str) -> Self {
        ZenohConfigError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
            reason:
                "expected format like 'tcp/127.0.0.1:7447', 'connect/tcp/...', or 'listen/tcp/...'"
                    .to_string(),
        }
    }
}

/// Result type aliases for convenience
pub type TcguiResult<T> = anyhow::Result<T>;
pub type BackendResult<T> = Result<T, BackendError>;
pub type FrontendResult<T> = Result<T, FrontendError>;
pub type ZenohConfigResult<T> = Result<T, ZenohConfigError>;
