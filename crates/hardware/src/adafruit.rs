
use crate::error::{Result, SensorError};
use crate::traits::{async_trait, CapturedImage, FingerprintSensor, SensorInfo};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tracing::{debug, info, warn};

pub struct AdafruitSensor {
    port: SerialStream,
    address: u32,
    timeout_ms: u64,
}

const CMD_GEN_IMAGE: u8 = 0x01;
const CMD_IMG_2TZ: u8 = 0x02;
const CMD_UP_IMAGE: u8 = 0x0A;
const CMD_DOWN_IMAGE: u8 = 0x0B;
const CMD_VFY_PWD: u8 = 0x13;
const CMD_READ_SYS_PARA: u8 = 0x0F;
const CMD_AURA_LED: u8 = 0x35;

const ACK_OK: u8 = 0x00;
const ACK_PACKET_ERR: u8 = 0x01;
const ACK_NO_FINGER: u8 = 0x02;
const ACK_IMG_FAIL: u8 = 0x03;

const PKT_CMD: u8 = 0x01;
const PKT_DATA: u8 = 0x02;
const PKT_ACK: u8 = 0x07;
const PKT_END: u8 = 0x08;

impl AdafruitSensor {
    pub async fn connect(port_path: &str, baud_rate: u32) -> Result<Self> {
        info!("Connecting to Adafruit sensor on {} @ {}", port_path, baud_rate);

        let port = tokio_serial::new(port_path, baud_rate)
            .open_native_async()
            .map_err(|e| SensorError::SerialError(e.to_string()))?;

        let mut sensor = Self {
            port,
            address: 0xFFFFFFFF,
            timeout_ms: 5000,
        };

        sensor.verify_password(0x00000000).await?;

        info!("Adafruit sensor connected successfully");
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

        self.send_packet(PKT_CMD, &data).await?;
        let response = self.receive_packet().await?;

        if response.is_empty() || response[0] != ACK_OK {
            return Err(SensorError::InvalidResponse("Password verification failed".into()));
        }

        Ok(())
    }

    async fn send_packet(&mut self, packet_type: u8, data: &[u8]) -> Result<()> {
        let length = data.len() as u16 + 2;

        let mut packet = Vec::with_capacity(12 + data.len());

        packet.push(0xEF);
        packet.push(0x01);

        packet.push(((self.address >> 24) & 0xFF) as u8);
        packet.push(((self.address >> 16) & 0xFF) as u8);
        packet.push(((self.address >> 8) & 0xFF) as u8);
        packet.push((self.address & 0xFF) as u8);

        packet.push(packet_type);

        packet.push(((length >> 8) & 0xFF) as u8);
        packet.push((length & 0xFF) as u8);

        packet.extend_from_slice(data);

        let checksum: u16 = packet_type as u16
            + ((length >> 8) & 0xFF) as u16
            + (length & 0xFF) as u16
            + data.iter().map(|&b| b as u16).sum::<u16>();

        packet.push(((checksum >> 8) & 0xFF) as u8);
        packet.push((checksum & 0xFF) as u8);

        debug!("Sending packet: {:02X?}", packet);

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
            return Err(SensorError::InvalidResponse("Invalid packet header".into()));
        }

        let length = ((header[7] as u16) << 8) | (header[8] as u16);
        let data_len = length.saturating_sub(2) as usize;

        let mut data = vec![0u8; length as usize];
        self.port.read_exact(&mut data).await
            .map_err(|e| SensorError::SerialError(e.to_string()))?;

        debug!("Received packet: {:02X?}", data);

        Ok(data[..data_len].to_vec())
    }

    async fn gen_image(&mut self) -> Result<u8> {
        self.send_packet(PKT_CMD, &[CMD_GEN_IMAGE]).await?;
        let response = self.receive_packet().await?;

        if response.is_empty() {
            return Err(SensorError::InvalidResponse("Empty response".into()));
        }

        Ok(response[0])
    }

    async fn upload_image(&mut self) -> Result<Vec<u8>> {
        self.send_packet(PKT_CMD, &[CMD_UP_IMAGE]).await?;

        let ack = self.receive_packet().await?;
        if ack.is_empty() || ack[0] != ACK_OK {
            return Err(SensorError::CaptureFailed("Image upload rejected".into()));
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

            if packet_type == PKT_END {
                break;
            }
        }

        Ok(image_data)
    }
}

#[async_trait]
impl FingerprintSensor for AdafruitSensor {
    fn info(&self) -> Result<SensorInfo> {
        Ok(SensorInfo {
            model: "Adafruit Optical Fingerprint Sensor".to_string(),
            firmware_version: "1.0.0".to_string(),
            resolution_dpi: 500,
            image_width: 256,
            image_height: 288,
            storage_capacity: 127,
            stored_count: 0,
        })
    }

    async fn finger_present(&mut self) -> Result<bool> {
        match self.gen_image().await? {
            ACK_OK => Ok(true),
            ACK_NO_FINGER => Ok(false),
            code => {
                warn!("Unexpected response code: 0x{:02X}", code);
                Ok(false)
            }
        }
    }

    async fn capture(&mut self) -> Result<CapturedImage> {
        info!("Waiting for finger...");

        loop {
            match self.gen_image().await? {
                ACK_OK => break,
                ACK_NO_FINGER => {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                ACK_IMG_FAIL => {
                    return Err(SensorError::CaptureFailed("Image acquisition failed".into()));
                }
                code => {
                    return Err(SensorError::InvalidResponse(format!("Error code: 0x{:02X}", code)));
                }
            }
        }

        info!("Finger detected, capturing...");

        let raw_data = self.upload_image().await?;

        let data: Vec<u8> = raw_data
            .iter()
            .flat_map(|&b| {
                let hi = (b >> 4) * 17;
                let lo = (b & 0x0F) * 17;
                [hi, lo]
            })
            .collect();

        info!("Captured {} bytes ({} pixels)", data.len(), 256 * 288);

        info!("Remove finger...");
        while self.finger_present().await? {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Ok(CapturedImage {
            data,
            width: 256,
            height: 288,
            bpp: 8,
            quality: 80,
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
            width: 256,
            height: 288,
            bpp: 8,
            quality: 80,
        })
    }

    async fn set_led(&mut self, on: bool) -> Result<()> {
        let control = if on { 0x01 } else { 0x00 };
        let data = [CMD_AURA_LED, control, 0x00, 0x00, 0x02];

        self.send_packet(PKT_CMD, &data).await?;
        let response = self.receive_packet().await?;

        if response.is_empty() || response[0] != ACK_OK {
            warn!("LED control may not be supported on this model");
        }

        Ok(())
    }
}
