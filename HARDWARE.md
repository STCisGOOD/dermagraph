# Hardware Setup Guide

> Setting up the R503 fingerprint sensor with Raspberry Pi for Dermagraph.

---

## Bill of Materials

| Component | Model | Price | Link |
|-----------|-------|-------|------|
| Fingerprint Sensor | R503 Capacitive | ~$15 | [Amazon](https://www.amazon.com/dp/B07V4KXBLJ) |
| Single-Board Computer | Raspberry Pi 4 | ~$15 | [Adafruit](https://www.amazon.com/dp/B0899VXM8F?ref=ppx_yo2ov_dt_b_fed_asin_title) |
| MicroSD Card | 16GB+ Class 10 | ~$8 | Any brand |
| Jumper Wires | Female-to-Female | ~$3 | Any brand |
| **Total** | | **~$41** | |

**Alternative Pi Options:**
- Raspberry Pi 4 (4GB): Faster, ~$55 — Recommended for development
- Raspberry Pi 3B+: Works but slower proof generation

---

## R503 Sensor Specifications

```
┌────────────────────────────────────────────────┐
│                R503 CAPACITIVE                 │
│            FINGERPRINT MODULE                  │
├────────────────────────────────────────────────┤
│  Resolution:     508 DPI                       │
│  Image Size:     192 × 192 pixels              │
│  Sensing Area:   15.5 × 15.5 mm                │
│  Interface:      UART (TTL 3.3V)               │
│  Baud Rate:      9600 - 115200                 │
│  Voltage:        3.3V (3.0V - 3.6V)            │
│  Current:        < 50mA                        │
│  FAR:            < 0.001%                      │
│  FRR:            < 1%                          │
└────────────────────────────────────────────────┘
```

**Why Capacitive (not Optical)?**
- Better liveness detection (detects real skin, not photos)
- Works with dry/wet fingers
- More compact form factor
- Lower power consumption

---

## Wiring Diagram

```
    R503 SENSOR                    RASPBERRY PI
    ───────────                    ────────────

    ┌─────────┐                   ┌───────────────────┐
    │  ┌───┐  │                   │  ○ ○ ○ ○ ○ ○ ...  │ ← GPIO Header
    │  │   │  │   VCC (Red)       │  1 2              │
    │  │   │──────────────────────│──○                │ 3.3V (Pin 1)
    │  │   │  │                   │                   │
    │  │   │  │   GND (Black)     │    ○              │
    │  │   │──────────────────────│────○              │ GND (Pin 6)
    │  │   │  │                   │                   │
    │  │   │  │   TX (Yellow)     │        ○          │
    │  │   │──────────────────────│────────○          │ GPIO15/RX (Pin 10)
    │  │   │  │                   │                   │
    │  │   │  │   RX (Green)      │          ○        │
    │  │   │──────────────────────│──────────○        │ GPIO14/TX (Pin 8)
    │  └───┘  │                   │                   │
    │ TOUCH   │   Touch (Blue)    │            ○      │
    │ (opt)   │───────────────────│────────────○      │ GPIO17 (Pin 11)
    └─────────┘                   └───────────────────┘


    Wire Color Reference:
    ─────────────────────
    Red    = VCC (Power 3.3V)
    Black  = GND (Ground)
    Yellow = TX  (Sensor transmit → Pi receive)
    Green  = RX  (Sensor receive ← Pi transmit)
    Blue   = Touch sense (optional interrupt)
```

### Pin Mapping Table

| R503 Wire | Color | Pi Pin | GPIO | Function |
|-----------|-------|--------|------|----------|
| VCC | Red | 1 | - | 3.3V Power |
| GND | Black | 6 | - | Ground |
| TX | Yellow | 10 | GPIO15 | UART RX |
| RX | Green | 8 | GPIO14 | UART TX |
| Touch | Blue | 11 | GPIO17 | Touch IRQ (optional) |

---

## Raspberry Pi Setup

### 1. Flash Raspberry Pi OS

```bash
# Download Raspberry Pi Imager
# https://www.raspberrypi.com/software/

# Flash "Raspberry Pi OS Lite (64-bit)" to SD card
# Enable SSH in imager settings
# Set hostname: dermagraphd
# Set username/password
```

### 2. Enable UART

```bash
# SSH into Pi
ssh pi@dermagraphd.local

# Edit boot config
sudo nano /boot/config.txt

# Add these lines:
enable_uart=1
dtoverlay=disable-bt

# Disable serial console (frees up UART for sensor)
sudo raspi-config
# → Interface Options → Serial Port
# → Login shell over serial? No
# → Serial port hardware enabled? Yes

# Reboot
sudo reboot
```

### 3. Install Dependencies

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install build essentials
sudo apt install -y build-essential pkg-config libssl-dev

# Install Noir (nargo)
curl -L https://raw.githubusercontent.com/noir-lang/noirup/main/install | bash
noirup

# Clone and build Dermagraph
git clone https://github.com/STCisGOOD/dermagraph.git
cd dermagraph
cargo build --release -p dermagraphd
```

### 4. Install Sunspot (Groth16 Prover)

```bash
# Sunspot is the Noir → Solana bridge
# Build from source for ARM64:
git clone https://github.com/Sunspot-Foundation/sunspot.git
cd sunspot
cargo build --release
sudo cp target/release/sunspot /usr/local/bin/
```

### 5. Verify Sensor Connection

```bash
# Check serial port exists
ls -la /dev/ttyAMA0

# Test with minicom (optional)
sudo apt install minicom
minicom -D /dev/ttyAMA0 -b 57600
```

---

## Running the Daemon

### Basic Start

```bash
cd ~/dermagraph
./target/release/dermagraphd \
    --sensor r503 \
    --serial-port /dev/ttyAMA0 \
    --http-port 31415
```

### Systemd Service (Production)

```bash
# Create service file
sudo nano /etc/systemd/system/dermagraphd.service
```

```ini
[Unit]
Description=Dermagraph Biometric Daemon
After=network.target

[Service]
Type=simple
User=pi
WorkingDirectory=/home/pi/dermagraph
ExecStart=/home/pi/dermagraph/target/release/dermagraphd \
    --sensor r503 \
    --serial-port /dev/ttyAMA0 \
    --http-port 31415
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
# Enable and start
sudo systemctl daemon-reload
sudo systemctl enable dermagraphd
sudo systemctl start dermagraphd

# Check status
sudo systemctl status dermagraphd

# View logs
journalctl -u dermagraphd -f
```

---

## Network Configuration

### Option A: Same Network (Development)

```
Laptop (Browser)
      │
      │ HTTP
      ▼
Bridge Server (localhost:3000)
      │
      │ HTTP (192.168.1.X:31415)
      ▼
Raspberry Pi (dermagraphd)
```

```bash
# On laptop, set bridge server to point to Pi
# bridge-server/.env
DAEMON_URL=http://192.168.1.20:31415
```

### Option B: SSH Tunnel (Secure)

```bash
# Create tunnel from laptop
ssh -L 31415:localhost:31415 pi@dermagraphd.local

# Bridge server points to localhost
DAEMON_URL=http://localhost:31415
```

### Option C: Tailscale (Remote)

```bash
# Install Tailscale on both devices
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up

# Use Tailscale IP
DAEMON_URL=http://100.x.x.x:31415
```

---

## Troubleshooting

### Sensor Not Responding

```bash
# Check permissions
sudo usermod -a -G dialout pi
# Log out and back in

# Check serial port
sudo dmesg | grep tty

# Test with stty
stty -F /dev/ttyAMA0 57600
```

### UART Conflicts

```bash
# If Bluetooth is using UART:
sudo systemctl disable hciuart
sudo systemctl stop hciuart

# Verify in config.txt:
dtoverlay=disable-bt
```

### Power Issues

```
# R503 needs stable 3.3V
# If getting intermittent failures:
- Use Pi's 3.3V pin (not USB power)
- Add 100µF capacitor across VCC/GND
- Check wire connections
```

### Slow Proof Generation

```bash
# On Pi Zero 2 W, expect ~8-10 seconds for proof
# On Pi 4, expect ~4 seconds

# To speed up on Pi 4:
# Enable performance governor
echo performance | sudo tee /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
```

---

## LED Status Indicators

The R503 has an LED ring that indicates status:

| Color | Meaning |
|-------|---------|
| Blue breathing | Ready for finger |
| Blue solid | Finger detected |
| Purple flash | Capture in progress |
| Green | Success |
| Red | Error/No match |

---

## Development Mode (No Sensor)

For development without hardware:

```bash
# Run with mock sensor
cargo run --release -p dermagraphd -- --sensor mock

# This simulates fingerprint captures with random data
# Useful for testing the full flow without Pi
```

---

## Security Considerations

1. **Physical Security**: The Pi should be in a secure enclosure. Anyone with physical access could tamper with stored data.

2. **Network Security**: Never expose port 31415 to the public internet. Use SSH tunnels or VPN.

3. **Storage Encryption**: The daemon encrypts sensitive data with XChaCha20-Poly1305. The encryption key is derived from the biometric itself.

4. **Secure Boot**: Consider enabling secure boot on production devices to prevent tampering.

---

## Appendix: R503 Protocol

The R503 uses a packet-based UART protocol:

```
┌──────┬──────┬──────────┬──────────┬──────────┬──────────┐
│ HEAD │ ADDR │ PKG_TYPE │ PKG_LEN  │ DATA     │ CHECKSUM │
│ 2B   │ 4B   │ 1B       │ 2B       │ Variable │ 2B       │
└──────┴──────┴──────────┴──────────┴──────────┴──────────┘

HEAD = 0xEF01 (fixed)
ADDR = 0xFFFFFFFF (broadcast)
PKG_TYPE = 0x01 (command) / 0x07 (ack)
```

Key commands used by Dermagraph:

| Command | Code | Description |
|---------|------|-------------|
| GenImg | 0x01 | Capture fingerprint image |
| Img2Tz | 0x02 | Generate template from image |
| UpImage | 0x0A | Upload raw image to host |
| ReadSysPara | 0x0F | Read system parameters |
