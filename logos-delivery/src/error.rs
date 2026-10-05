use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("failed to start node: {0}")]
    Startup(String),
    #[error("publish failed: {0}")]
    Publish(String),
    #[error("subscribe failed: {0}")]
    Subscribe(String),
    #[error("unsubscribe failed: {0}")]
    Unsubscribe(String),
    #[error("channel operation failed: {0}")]
    Channel(String),
    #[error("shutdown failed: {0}")]
    Shutdown(String),
    #[error("timed out waiting for: {0}")]
    Timeout(&'static str),
}

pub type Result<T> = std::result::Result<T, DeliveryError>;
