# Setup Guide

## Hardware Requirements

### Shopping List (~$140)

| Item | Link | Cost |
|------|------|------|
| Raspberry Pi 4 (2GB+) | [Amazon](https://www.amazon.com/dp/B0899VXM8F?ref=ppx_yo2ov_dt_b_fed_asin_title) | $100 |
| R503 Fingerprint Sensor | [Amazon](https://www.amazon.com/dp/B09MLBNY78?ref=ppx_yo2ov_dt_b_fed_asin_title) | $25 |
| USB-C Power Supply (5V/3A) | Any | $8 |
| MicroSD Card (32GB+) | Any | $6 |
| Jumper wires (female-female) | Any | $3 |

### Wiring Diagram

```
R503 Sensor          Raspberry Pi 4
───────────          ──────────────
VCC (Red)     ────▶  Pin 1 (3.3V)
GND (Black)   ────▶  Pin 6 (GND)
TX (Yellow)   ────▶  Pin 10 (RXD/GPIO15)
RX (Green)    ────▶  Pin 8 (TXD/GPIO14)
```

---

## Software Installation

### 1. Raspberry Pi Setup

```bash
# Flash Raspberry Pi OS Lite (64-bit) to SD card
# Enable SSH, configure WiFi

# SSH into Pi
ssh pi@raspberrypi.local

# Enable UART
sudo raspi-config
# → Interface Options → Serial Port
# → Login shell: No
# → Serial hardware: Yes
# Reboot

# Install dependencies
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Noir (nargo)
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup

# Install Sunspot
cargo install sunspot-cli
```

### 2. Deploy Daemon

**Option A: Cross-compile on host machine**

```bash
# On Windows/Mac/Linux with Docker
cd dermagraph

docker run --rm -v "$(pwd):/project" -w /project rust:latest bash -c "
  dpkg --add-architecture arm64 && \
  apt-get update -qq && \
  apt-get install -y -qq gcc-aarch64-linux-gnu libc6-dev-arm64-cross && \
  rustup target add aarch64-unknown-linux-gnu && \
  export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc && \
  cargo build --release --target aarch64-unknown-linux-gnu -p dermagraphd
"

# Copy to Pi
scp target/aarch64-unknown-linux-gnu/release/dermagraphd pi@<PI_IP>:/usr/local/bin/
```

**Option B: Build on Pi (slower)**

```bash
# On Pi
git clone https://github.com/STCisGOOD/dermagraph.git
cd dermagraph
cargo build --release -p dermagraphd
sudo cp target/release/dermagraphd /usr/local/bin/
```

### 3. Deploy CNN Weights

The pre-trained CNN weights are available from the GitHub release (not included in repo due to size).

```bash
# Download weights from GitHub release
curl -L -o best_burn.safetensors \
  https://github.com/STCisGOOD/dermagraph/releases/download/v1.0.0/best_burn.safetensors

# Copy to Pi
scp best_burn.safetensors pi@<PI_IP>:/home/pi/dermagraph/checkpoints/
```

Or download directly on the Pi:
```bash
# On Pi
mkdir -p ~/dermagraph/checkpoints
curl -L -o ~/dermagraph/checkpoints/best_burn.safetensors \
  https://github.com/STCisGOOD/dermagraph/releases/download/v1.0.0/best_burn.safetensors
```

### 4. Deploy Noir Circuit

```bash
# On Pi
mkdir -p ~/dermagraph/circuits/person_identity

# Copy circuit files
scp -r circuits/person_identity/* pi@<PI_IP>:~/dermagraph/circuits/person_identity/

# Compile circuit (on Pi)
cd ~/dermagraph/circuits/person_identity
nargo compile

# Setup Sunspot (generate proving key)
sunspot setup target/person_identity.json
```

### 5. Start Daemon

```bash
# Create systemd service
sudo tee /etc/systemd/system/dermagraphd.service << EOF
[Unit]
Description=Dermagraph Daemon
After=network.target

[Service]
Type=simple
User=pi
WorkingDirectory=/home/pi/dermagraph
ExecStart=/usr/local/bin/dermagraphd start
Restart=always
Environment="DERMAGRAPH_DATA=/home/pi/.dermagraphd"
Environment="DERMAGRAPH_CNN_WEIGHTS=/home/pi/dermagraph/checkpoints/best_burn.safetensors"
Environment="DERMAGRAPH_CIRCUIT_DIR=/home/pi/dermagraph/circuits/person_identity"

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable dermagraphd
sudo systemctl start dermagraphd

# Check logs
journalctl -u dermagraphd -f
```

---

## Web App Setup

### 1. Configure Environment

```bash
cd web-app

# Create .env.local
cat > .env.local << EOF
VITE_DAEMON_URL=http://<PI_IP>:31415
VITE_USE_REAL_SOLANA=true
VITE_PRIVY_APP_ID=<your-privy-app-id>
EOF
```

### 2. Install Dependencies

```bash
npm install
```

### 3. Run Development Server

```bash
npm run dev
# Open http://localhost:5173
```

---

## Solana Setup

### 1. Configure CLI

```bash
solana config set --url devnet
solana-keygen new  # or use existing
solana airdrop 2   # get devnet SOL
```

### 2. Deploy Programs (if needed)

```bash
cd solana/programs/dao-voting

# Build
anchor build

# Deploy
anchor deploy
```

### 3. Initialize DAO

```bash
npx tsx scripts/init-dao.ts
```

### 4. Sync Merkle Root

After enrollment on Pi:

```bash
# Get merkle root from daemon logs
# journalctl -u dermagraphd | grep "merkle_root"

# Update on-chain
npx tsx scripts/update-merkle-root.ts <merkle_root_hex>
```

---

## Testing the Flow

### 1. Enroll Identity

1. Open web app
2. Connect wallet (Privy)
3. Click "Register"
4. Scan 3 fingers on R503 sensor
5. Wait for enrollment complete

### 2. Cast Vote

1. Click on a proposal
2. Scan any finger
3. Wait for proof generation (~35s)
4. Sign transaction
5. See "Vote Recorded" success

### 3. Verify On-Chain

```bash
# Check transaction on explorer
solana confirm <tx-signature> -v
```

---

## Troubleshooting

### Daemon won't start
```bash
# Check sensor connection
ls /dev/ttyS0  # Should exist

# Check permissions
sudo usermod -a -G dialout pi
# Logout and login again
```

### Proof verification fails
```bash
# Check merkle roots match
# Daemon:
journalctl -u dermagraphd | grep "merkle_root"

# On-chain:
solana account <DAO_PDA> --output json | jq '.data'

# If different, sync:
npx tsx scripts/update-merkle-root.ts <daemon_root>
```

### CNN inference slow
- First inference loads model (~26s)
- Subsequent inferences are faster (~15s)
- Consider pre-warming on daemon start

### "No space left on device"
```bash
# Clean old proofs
rm -rf ~/dermagraph/circuits/target/*.gz
rm -rf ~/dermagraph/circuits/target/*.proof
```
