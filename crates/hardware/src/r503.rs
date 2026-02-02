
use crate::error::{Result, SensorError};
use crate::traits::{async_trait, CapturedImage, FingerprintSensor, SensorInfo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tracing::{debug, info, warn};

pub struct R503Sensor {
    port: SerialStream,
    address: u32,
    timeout_ms: u64,
}

const CMD_GEN_IMAGE: u8 = 0x01;
const CMD_UP_IMAGE: u8 = 0x0A;
const CMD_VFY_PWD: u8 = 0x13;
const CMD_READ_SYS_PARA: u8 = 0x0F;
const CMD_AURA_CONTROL: u8 = 0x35;
const CMD_CHECK_SENSOR: u8 = 0x36;

const ACK_OK: u8 = 0x00;
const ACK_NO_FINGER: u8 = 0x02;

const LED_BREATHING: u8 = 0x01;
const LED_FLASHING: u8 = 0x02;
const LED_ON: u8 = 0x03;
const LED_OFF: u8 = 0x04;
const LED_GRADUAL_ON: u8 = 0x05;
const LED_GRADUAL_OFF: u8 = 0x06;

const COLOR_RED: u8 = 0x01;
const COLOR_BLUE: u8 = 0x02;
const COLOR_PURPLE: u8 = 0x03;

impl R503Sensor {
    pub async fn connect(port_path: &str, baud_rate: u32) -> Result<Self> {
        info!("Connecting to R503 sensor on {} @ {}", port_path, baud_rate);

        let port = tokio_serial::new(port_path, baud_rate)
            .open_native_async()
            .map_err(|e| SensorError::SerialError(e.to_string()))?;

        let mut sensor = Self {
            port,
            address: 0xFFFFFFFF,
            timeout_ms: 5000,
        };

        sensor.verify_password(0x00000000).await?;

        if let Err(e) = sensor.check_sensor().await {
            info!("Sensor check skipped (may not be supported): {}", e);
        }

        sensor.set_aura_led(LED_ON, COLOR_BLUE, 0).await?;

        info!("R503 sensor connected successfully");
        Ok(sensor)
    }

    async fn verify_password(&mut self, password: u32) -> Result<()> {
        let data = [
            CMD_VFY_PWD,
            ((password >> 24) & 0xFF) as u8,
            ((password >> 16) & 0xFF) as u8,
            ((password >> 8) & 0xFF) as u8,
            (password & 0xFF) as u8,
        ];

        self.send_packet(&data).await?;
        let response = self.receive_packet().await?;

        if response.is_empty() || response[0] != ACK_OK {
            return Err(SensorError::InvalidResponse("Password verification failed".into()));
        }

        Ok(())
    }

    async fn check_sensor(&mut self) -> Result<()> {
        self.send_packet(&[CMD_CHECK_SENSOR]).await?;
        let response = self.receive_packet().await?;

        if response.is_empty() || response[0] != ACK_OK {
            return Err(SensorError::NotConnected("Sensor check failed".into()));
        }

        Ok(())
    }

    pub async fn set_aura_led(&mut self, control: u8, color: u8, count: u8) -> Result<()> {
        let data = [CMD_AURA_CONTROL, control, 0x00, color, count];

        self.send_packet(&data).await?;
        let _ = self.receive_packet().await;

        Ok(())
    }

    async fn send_packet(&mut self, data: &[u8]) -> Result<()> {
        let length = data.len() as u16 + 2;

        let mut packet = Vec::with_capacity(12 + data.len());

        packet.push(0xEF);
        packet.push(0x01);

        packet.push(((self.address >> 24) & 0xFF) as u8);
        packet.push(((self.address >> 16) & 0xFF) as u8);
        packet.push(((self.address >> 8) & 0xFF) as u8);
        packet.push((self.address & 0xFF) as u8);

        packet.push(0x01);

        packet.push(((length >> 8) & 0xFF) as u8);
        packet.push((length & 0xFF) as u8);

        packet.extend_from_slice(data);

        let checksum: u16 = 0x01u16
            + ((length >> 8) & 0xFF) as u16
            + (length & 0xFF) as u16
            + data.iter().map(|&b| b as u16).sum::<u16>();

        packet.push(((checksum >> 8) & 0xFF) as u8);
        packet.push((checksum & 0xFF) as u8);

        debug!("R503 send: {:02X?}", packet);

        self.port.write_all(&packet).await?;
        self.port.flush().await?;

        Ok(())
    }

    async fn receive_packet(&mut self) -> Result<Vec<u8>> {
        let mut header = [0u8; 9];

        tokio::time::timeout(
            std::time::Duration::from_millis(self.timeout_ms),
            self.port.read_exact(&mut header),
        )
        .await
        .map_err(|_| SensorError::Timeout)?
        .map_err(|e| SensorError::SerialError(e.to_string()))?;

        if header[0] != 0xEF || header[1] != 0x01 {
            return Err(SensorError::InvalidResponse("Invalid header".into()));
        }

        let length = ((header[7] as u16) << 8) | (header[8] as u16);
        let data_len = length.saturating_sub(2) as usize;

        let mut data = vec![0u8; length as usize];
        self.port.read_exact(&mut data).await
            .map_err(|e| SensorError::SerialError(e.to_string()))?;

        debug!("R503 recv: {:02X?}", data);

        Ok(data[..data_len].to_vec())
    }

    async fn gen_image(&mut self) -> Result<u8> {
        self.send_packet(&[CMD_GEN_IMAGE]).await?;
        let response = self.receive_packet().await?;

        if response.is_empty() {
            return Err(SensorError::InvalidResponse("Empty response".into()));
        }

        Ok(response[0])
    }

    async fn upload_image(&mut self) -> Result<Vec<u8>> {
        self.send_packet(&[CMD_UP_IMAGE]).await?;

        let ack = self.receive_packet().await?;
        if ack.is_empty() || ack[0] != ACK_OK {
            return Err(SensorError::CaptureFailed("Upload rejected".into()));
        }

        let mut image_data = Vec::new();

        loop {
            let mut header = [0u8; 9];
            self.port.read_exact(&mut header).await
                .map_err(|e| SensorError::SerialError(e.to_string()))?;

            let packet_type = header[6];
            let length = ((header[7] as u16) << 8) | (header[8] as u16);
            let data_len = length.saturating_sub(2) as usize;

            let mut data = vec![0u8; length as usize];
            self.port.read_exact(&mut data).await
                .map_err(|e| SensorError::SerialError(e.to_string()))?;

            image_data.extend_from_slice(&data[..data_len]);

            if packet_type == 0x08 {
                break;
            }
        }

        Ok(image_data)
    }
}

#[async_trait]
impl FingerprintSensor for R503Sensor {
    fn info(&self) -> Result<SensorInfo> {
        Ok(SensorInfo {
            model: "R503 Capacitive Fingerprint Sensor".to_string(),
            firmware_version: "1.0.0".to_string(),
            resolution_dpi: 508,
            image_width: 192,
            image_height: 192,
            storage_capacity: 200,
            stored_count: 0,
        })
    }

    async fn finger_present(&mut self) -> Result<bool> {
        match self.gen_image().await? {
            ACK_OK => Ok(true),
            ACK_NO_FINGER => Ok(false),
            _ => Ok(false),
        }
    }

    async fn capture(&mut self) -> Result<CapturedImage> {
        self.set_aura_led(LED_BREATHING, COLOR_BLUE, 0).await?;

        info!("Waiting for finger on R503...");

        loop {
            match self.gen_image().await? {
                ACK_OK => break,
                ACK_NO_FINGER => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                code => {
                    self.set_aura_led(LED_FLASHING, COLOR_RED, 3).await?;
                    return Err(SensorError::CaptureFailed(format!("Error: 0x{:02X}", code)));
                }
            }
        }

        self.set_aura_led(LED_ON, COLOR_PURPLE, 0).await?;

        info!("Finger detected, uploading image...");

        let raw_data = self.upload_image().await?;

        let data: Vec<u8> = raw_data
            .iter()
            .flat_map(|&b| [(b >> 4) * 17, (b & 0x0F) * 17])
            .collect();

        info!("Remove finger...");
        while self.finger_present().await? {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        self.set_aura_led(LED_ON, COLOR_BLUE, 0).await?;

        Ok(CapturedImage {
            data,
            width: 192,
            height: 192,
            bpp: 8,
            quality: 85,
        })
    }

    async fn capture_no_wait(&mut self) -> Result<CapturedImage> {
        let result = self.gen_image().await?;

        if result == ACK_NO_FINGER {
            return Err(SensorError::NoFinger);
        }

        if result != ACK_OK {
            return Err(SensorError::CaptureFailed(format!("Error: 0x{:02X}", result)));
        }

        let raw_data = self.upload_image().await?;

        let data: Vec<u8> = raw_data
            .iter()
            .flat_map(|&b| [(b >> 4) * 17, (b & 0x0F) * 17])
            .collect();

        Ok(CapturedImage {
            data,
            width: 192,
            height: 192,
            bpp: 8,
            quality: 85,
        })
    }

    async fn set_led(&mut self, on: bool) -> Result<()> {
        if on {
            self.set_aura_led(LED_ON, COLOR_BLUE, 0).await
        } else {
            self.set_aura_led(LED_OFF, 0, 0).await
        }
    }
}
