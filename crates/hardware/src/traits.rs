
use crate::error::Result;
pub use async_trait::async_trait;
use biometric_extract::MinutiaeSet;
use turing_core::GraphLaplacian;

#[derive(Debug, Clone)]
pub struct SensorConfig {
    pub sensor_type: crate::SensorType,
    pub port: Option<String>,
    pub baud_rate: u32,
    pub timeout_ms: u64,
    pub retries: u32,
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self {
            sensor_type: crate::SensorType::Mock,
            port: None,
            baud_rate: 57600,
            timeout_ms: 5000,
            retries: 3,
        }
    }
}

impl SensorConfig {
    pub fn mock() -> Self {
        Self::default()
    }

    pub fn adafruit(port: impl Into<String>) -> Self {
        Self {
            sensor_type: crate::SensorType::Adafruit,
            port: Some(port.into()),
            baud_rate: 57600,
            ..Default::default()
        }
    }

    pub fn r503(port: impl Into<String>) -> Self {
        Self {
            sensor_type: crate::SensorType::R503,
            port: Some(port.into()),
            baud_rate: 57600,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct SensorInfo {
    pub model: String,
    pub firmware_version: String,
    pub resolution_dpi: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub storage_capacity: u32,
    pub stored_count: u32,
}

pub struct CapturedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub quality: u8,
}

impl CapturedImage {
    pub fn extract_minutiae(&self) -> Result<MinutiaeSet> {
        biometric_extract::extract_minutiae(&self.data)
            .map_err(|e| crate::SensorError::CaptureFailed(e.to_string()))
    }

    pub async fn extract_laplacian(&self) -> Result<GraphLaplacian> {
        biometric_extract::extract_laplacian(&self.data)
            .await
            .map_err(|e| crate::SensorError::CaptureFailed(e.to_string()))
    }

    pub fn save_png(&self, path: &str) -> Result<()> {
        use image::{GrayImage, ImageBuffer};

        let img: GrayImage = ImageBuffer::from_raw(self.width, self.height, self.data.clone())
            .ok_or_else(|| crate::SensorError::CaptureFailed("Invalid image data".into()))?;

        img.save(path).map_err(|e| crate::SensorError::CaptureFailed(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
pub trait FingerprintSensor: Send {
    fn info(&self) -> Result<SensorInfo>;

    async fn finger_present(&mut self) -> Result<bool>;

    async fn capture(&mut self) -> Result<CapturedImage>;

    async fn capture_no_wait(&mut self) -> Result<CapturedImage>;

    async fn set_led(&mut self, _on: bool) -> Result<()> {
        Ok(())
    }
}
