
mod error;
mod traits;
mod mock;

#[cfg(feature = "adafruit")]
mod adafruit;

#[cfg(feature = "r503")]
mod r503;

pub use error::{SensorError, Result};
pub use traits::{FingerprintSensor, CapturedImage, SensorInfo, SensorConfig};
pub use mock::MockSensor;

#[cfg(feature = "adafruit")]
pub use adafruit::AdafruitSensor;

#[cfg(feature = "r503")]
pub use r503::R503Sensor;

use tracing::info;

pub enum Sensor {
    Mock(MockSensor),
    #[cfg(feature = "adafruit")]
    Adafruit(AdafruitSensor),
    #[cfg(feature = "r503")]
    R503(R503Sensor),
}

impl Sensor {
    pub async fn connect(config: &SensorConfig) -> Result<Self> {
        match config.sensor_type {
            SensorType::Mock => {
                info!("Using mock sensor");
                Ok(Sensor::Mock(MockSensor::new()))
            }
            #[cfg(feature = "adafruit")]
            SensorType::Adafruit => {
                let port = config.port.as_ref()
                    .ok_or(SensorError::ConfigError("Port required for Adafruit sensor".into()))?;
                info!("Connecting to Adafruit sensor on {}", port);
                let sensor = AdafruitSensor::connect(port, config.baud_rate).await?;
                Ok(Sensor::Adafruit(sensor))
            }
            #[cfg(feature = "r503")]
            SensorType::R503 => {
                let port = config.port.as_ref()
                    .ok_or(SensorError::ConfigError("Port required for R503 sensor".into()))?;
                info!("Connecting to R503 sensor on {}", port);
                let sensor = R503Sensor::connect(port, config.baud_rate).await?;
                Ok(Sensor::R503(sensor))
            }
            #[allow(unreachable_patterns)]
            _ => Err(SensorError::UnsupportedSensor(format!("{:?}", config.sensor_type))),
        }
    }

    pub async fn capture(&mut self) -> Result<CapturedImage> {
        match self {
            Sensor::Mock(s) => s.capture().await,
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.capture().await,
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.capture().await,
        }
    }

    pub async fn capture_no_wait(&mut self) -> Result<CapturedImage> {
        match self {
            Sensor::Mock(s) => s.capture_no_wait().await,
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.capture_no_wait().await,
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.capture_no_wait().await,
        }
    }

    pub fn info(&self) -> Result<SensorInfo> {
        match self {
            Sensor::Mock(s) => s.info(),
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.info(),
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.info(),
        }
    }

    pub async fn finger_present(&mut self) -> Result<bool> {
        match self {
            Sensor::Mock(s) => s.finger_present().await,
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.finger_present().await,
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.finger_present().await,
        }
    }

    pub async fn set_led(&mut self, on: bool) -> Result<()> {
        match self {
            Sensor::Mock(s) => s.set_led(on).await,
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.set_led(on).await,
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.set_led(on).await,
        }
    }

    pub async fn set_led_waiting(&mut self) -> Result<()> {
        match self {
            Sensor::Mock(_) => Ok(()),
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.set_led(true).await,
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.set_aura_led(0x01, 0x02, 0).await,
        }
    }

    pub async fn set_led_success(&mut self) -> Result<()> {
        match self {
            Sensor::Mock(_) => Ok(()),
            #[cfg(feature = "adafruit")]
            Sensor::Adafruit(s) => s.set_led(true).await,
            #[cfg(feature = "r503")]
            Sensor::R503(s) => s.set_aura_led(0x03, 0x03, 0).await,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorType {
    Mock,
    Adafruit,
    R503,
}

impl Default for SensorType {
    fn default() -> Self {
        Self::Mock
    }
}
