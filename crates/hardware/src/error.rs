
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SensorError {
    #[error("Sensor not connected: {0}")]
    NotConnected(String),

    #[error("Serial port error: {0}")]
    SerialError(String),

    #[error("Sensor not responding (timeout)")]
    Timeout,

    #[error("Invalid sensor response: {0}")]
    InvalidResponse(String),

    #[error("No finger detected on sensor")]
    NoFinger,

    #[error("Image capture failed: {0}")]
    CaptureFailed(String),

    #[error("Image quality too low: {0}%")]
    LowQuality(u8),

    #[error("Sensor type not supported: {0}")]
    UnsupportedSensor(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SensorError>;
