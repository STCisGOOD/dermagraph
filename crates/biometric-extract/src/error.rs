
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExtractError {
    #[error("Failed to load image: {0}")]
    ImageLoadError(String),

    #[error("Unsupported image format: {0}")]
    UnsupportedFormat(String),

    #[error("Image too small: {width}x{height}, minimum is {min_size}x{min_size}")]
    ImageTooSmall {
        width: u32,
        height: u32,
        min_size: u32,
    },

    #[error("Image quality too low: score {score}, minimum is {minimum}")]
    LowQuality { score: f64, minimum: f64 },

    #[error("Insufficient minutiae: found {found}, minimum is {minimum}")]
    InsufficientMinutiae { found: usize, minimum: usize },

    #[error("Processing error: {0}")]
    ProcessingError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, ExtractError>;
