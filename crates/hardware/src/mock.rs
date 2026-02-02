
use crate::error::Result;
use crate::traits::{async_trait, CapturedImage, FingerprintSensor, SensorInfo};
use tracing::info;

pub struct MockSensor {
    finger_present: bool,
    capture_count: u32,
}

impl MockSensor {
    pub fn new() -> Self {
        Self {
            finger_present: true,
            capture_count: 0,
        }
    }

    fn generate_fingerprint(&self) -> Vec<u8> {
        let width = 256u32;
        let height = 288u32;
        let mut data = vec![255u8; (width * height) as usize];

        let cx = width as f64 / 2.0;
        let cy = height as f64 / 2.0;

        let seed = self.capture_count as f64 * 0.1;

        for y in 0..height {
            for x in 0..width {
                let dx = x as f64 - cx;
                let dy = y as f64 - cy;
                let r = (dx * dx + dy * dy).sqrt();
                let theta = dy.atan2(dx);

                let freq = 0.15 + seed * 0.01;
                let phase = theta * 2.0 + r * 0.05;
                let ridge = (phase * freq * std::f64::consts::TAU).sin();

                let noise = (x as f64 * 17.0 + y as f64 * 31.0 + seed * 100.0).sin() * 0.3;

                let value = ridge + noise;
                let pixel = if value > 0.0 { 255u8 } else { 0u8 };

                data[(y * width + x) as usize] = pixel;
            }
        }

        data
    }
}

impl Default for MockSensor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FingerprintSensor for MockSensor {
    fn info(&self) -> Result<SensorInfo> {
        Ok(SensorInfo {
            model: "Mock Sensor v1.0".to_string(),
            firmware_version: "0.0.1-dev".to_string(),
            resolution_dpi: 500,
            image_width: 256,
            image_height: 288,
            storage_capacity: 0,
            stored_count: 0,
        })
    }

    async fn finger_present(&mut self) -> Result<bool> {
        Ok(self.finger_present)
    }

    async fn capture(&mut self) -> Result<CapturedImage> {
        info!("Mock capture #{}", self.capture_count);

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let data = self.generate_fingerprint();
        self.capture_count += 1;

        Ok(CapturedImage {
            data,
            width: 256,
            height: 288,
            bpp: 8,
            quality: 85,
        })
    }

    async fn capture_no_wait(&mut self) -> Result<CapturedImage> {
        self.capture().await
    }

    async fn set_led(&mut self, on: bool) -> Result<()> {
        info!("Mock LED: {}", if on { "ON" } else { "OFF" });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_sensor() {
        let mut sensor = MockSensor::new();

        let info = sensor.info().await.unwrap();
        assert_eq!(info.model, "Mock Sensor v1.0");

        let present = sensor.finger_present().await.unwrap();
        assert!(present);

        let image = sensor.capture().await.unwrap();
        assert_eq!(image.width, 256);
        assert_eq!(image.height, 288);
        assert_eq!(image.data.len(), 256 * 288);
    }
}
