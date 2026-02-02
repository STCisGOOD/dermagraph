
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use directories::ProjectDirs;
use hardware::SensorConfig as HardwareSensorConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorType {
    Adafruit,
    R503,
    Mock,
}

impl Default for SensorType {
    fn default() -> Self {
        Self::Mock
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,

    pub sensor_type: SensorType,

    pub sensor_port: Option<String>,

    pub http_port: u16,

    #[serde(default = "default_http_bind")]
    pub http_bind: String,

    pub unix_socket: bool,

    pub socket_path: Option<PathBuf>,

    pub turing_iterations: usize,

    #[serde(default)]
    pub circuit_dir: Option<PathBuf>,

    #[serde(default)]
    pub sunspot_path: Option<PathBuf>,

    #[serde(default)]
    pub cnn_weights_path: Option<PathBuf>,

    #[serde(default)]
    pub solana: SolanaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaConfig {
    pub rpc_url: String,

    pub zk_verifier_program_id: String,

    pub dao_voting_program_id: String,

    pub dermagraph_verifier_program_id: String,
}

fn default_http_bind() -> String {
    "127.0.0.1".to_string()
}

impl Default for SolanaConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.devnet.solana.com".to_string(),
            zk_verifier_program_id: "BUwQwQYN3XHK7zLxGSkP9ajtfqtif4CrnH74vceVPHSh".to_string(),
            dao_voting_program_id: "DAOvoting111111111111111111111111111111111".to_string(),
            dermagraph_verifier_program_id: "DrmGrph1111111111111111111111111111111111111".to_string(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = ProjectDirs::from("", "", "dermagraphd")
            .map(|p| p.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".dermagraphd"));

        Self {
            data_dir,
            sensor_type: SensorType::default(),
            sensor_port: None,
            http_port: 31415,
            http_bind: default_http_bind(),
            unix_socket: cfg!(unix),
            socket_path: Some(PathBuf::from("/tmp/dermagraphd.sock")),
            turing_iterations: 64,
            circuit_dir: None,
            sunspot_path: None,
            cnn_weights_path: None,
            solana: SolanaConfig::default(),
        }
    }
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> anyhow::Result<Self> {
        if let Some(path) = path {
            let content = std::fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let default_path = Self::default_config_path();
            if default_path.exists() {
                let content = std::fs::read_to_string(&default_path)?;
                let config: Config = toml::from_str(&content)?;
                Ok(config)
            } else {
                Ok(Config::default())
            }
        }
    }

    pub fn default_config_path() -> PathBuf {
        ProjectDirs::from("", "", "dermagraphd")
            .map(|p| p.config_dir().join("config.toml"))
            .unwrap_or_else(|| PathBuf::from(".dermagraphd/config.toml"))
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn to_sensor_config(&self) -> HardwareSensorConfig {
        match self.sensor_type {
            SensorType::Mock => HardwareSensorConfig::mock(),
            SensorType::R503 => {
                let port = self.sensor_port.clone()
                    .unwrap_or_else(|| {
                        if cfg!(target_os = "linux") {
                            "/dev/ttyS0".to_string()
                        } else if cfg!(target_os = "windows") {
                            "COM3".to_string()
                        } else {
                            "/dev/tty.usbserial".to_string()
                        }
                    });
                HardwareSensorConfig::r503(port)
            }
            SensorType::Adafruit => {
                let port = self.sensor_port.clone()
                    .unwrap_or_else(|| "/dev/ttyUSB0".to_string());
                HardwareSensorConfig::adafruit(port)
            }
        }
    }

    pub fn is_hardware_sensor(&self) -> bool {
        !matches!(self.sensor_type, SensorType::Mock)
    }
}

