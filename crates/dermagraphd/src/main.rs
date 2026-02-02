
mod api;
mod auth;
mod config;
mod crypto;
mod model;
mod server;
mod storage;
mod xlock_auth;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "dermagraphd")]
#[command(about = "Background service for privacy-preserving biometric authentication")]
#[command(version)]
struct Cli {
    #[arg(short, long)]
    verbose: bool,

    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Start {
        #[arg(long, default_value = "31415")]
        port: u16,

        #[arg(long)]
        foreground: bool,
    },

    Stop,

    Status,

    Register,

    Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| log_level.to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::load(cli.config)?;

    match cli.command {
        Commands::Start { port, foreground } => {
            info!("Starting dermagraphd on port {}", port);

            if !foreground {
                info!("Running in foreground (daemonize not yet implemented)");
            }

            let storage = storage::Storage::open(&config.data_dir).await?;

            if !storage.is_registered().await? {
                info!("No identity registered. Run 'dermagraphd register' first.");
            }

            server::run(port, config, storage).await?;
        }

        Commands::Stop => {
            info!("Stopping dermagraphd...");
            println!("Stop not yet implemented");
        }

        Commands::Status => {
            println!("Status:");
            println!("  Daemon: not running (status check not implemented)");
            println!("  Config: {:?}", config.data_dir);
        }

        Commands::Register => {
            info!("Registering new identity...");

            let storage = storage::Storage::open(&config.data_dir).await?;

            if storage.is_registered().await? {
                error!("Identity already registered. Delete data dir to re-register.");
                return Ok(());
            }

            let using_hardware = config.is_hardware_sensor();
            if using_hardware {
                println!("Using {:?} sensor on {:?}", config.sensor_type, config.sensor_port);
                println!("\nPlace your finger on the sensor...");
            } else {
                println!("Using mock sensor (development mode)");
                println!("Configure sensor_type and sensor_port in config for real biometrics.");
            }

            println!("\nEnter a passphrase for additional security (or press Enter to skip):");
            let mut passphrase_input = String::new();
            std::io::stdin().read_line(&mut passphrase_input)?;
            let passphrase_input = passphrase_input.trim();

            let policy = if passphrase_input.is_empty() {
                println!("No passphrase set. Data will be protected by biometric only.");
                println!("(Using BiometricOnly policy - explicit single-factor encryption)");
                crypto::PassphrasePolicy::BiometricOnly
            } else {
                println!("Passphrase set. You will need BOTH fingerprint AND passphrase to access your identity.");
                crypto::PassphrasePolicy::Required(passphrase_input.to_string())
            };

            let result = if using_hardware {
                let sensor_config = config.to_sensor_config();
                auth::register_with_sensor(&storage, &sensor_config, policy).await?
            } else {
                auth::register_mock(&storage, policy).await?
            };

            println!("\nIdentity registered! (encrypted with biometric-bound key)");
            println!("Commitment: 0x{}", hex::encode(result.commitment.to_be_bytes()));
            println!("\nSubmit this commitment to your identity registry.");
            println!("\nIMPORTANT: Your biometric key is stored in memory only.");
            println!("Keep the daemon running or re-scan your fingerprint after restart.");
        }

        Commands::Config => {
            println!("Configuration:");
            println!("  Data directory: {:?}", config.data_dir);
            println!("  Sensor type: {:?}", config.sensor_type);
        }
    }

    Ok(())
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}
