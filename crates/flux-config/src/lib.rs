//! Owned schema and supervised Lua configuration loading. This crate never links Lua.
mod bounds;
mod coordinator;
mod schema;
mod supervisor;
pub use coordinator::*;
pub use schema::*;
pub use supervisor::*;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0}")]
    Invalid(String),
    #[error("configuration I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration helper exceeded deadline and was killed and reaped")]
    Timeout,
    #[error("configuration helper evaluation cancelled")]
    Cancelled,
    #[error("configuration helper failed ({status}): {diagnostic}")]
    Helper { status: String, diagnostic: String },
}
pub type Result<T> = std::result::Result<T, ConfigError>;
pub(crate) fn invalid(message: impl Into<String>) -> ConfigError {
    ConfigError::Invalid(message.into())
}
