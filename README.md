# Govee Humidity Control

This project monitors humidity using a Govee sensor and controls a Govee device based on humidity levels.

## Setup Instructions

### 1. Python Environment
```sh
python3 -m venv venv
source venv/bin/activate
pip install --upgrade pip
pip install -r requirements.txt
```

### 2. Configuration Setup

**Important**: This project uses configuration files to keep sensitive device information secure and out of version control.

1. **API Key Setup**:
   - Create an `api_key.secret` file in the project root
   - Add your Govee API key to this file
   - This file is already gitignored for security

2. **Device Configuration**:
   - Copy the template: `cp devices.config.template devices.config`
   - Edit `devices.config` and replace the placeholder MAC addresses with your actual device information:
     ```
     [humidity_sensor]
     sku = H5179
     device = YOUR_ACTUAL_HUMIDITY_SENSOR_MAC

     [control_device] 
     sku = H5080
     device = YOUR_ACTUAL_CONTROL_DEVICE_MAC
     ```
   - The `devices.config` file is gitignored and will not be committed to the repository

### 3. Running the Application
```sh
python main.py
```

The application will:
- Monitor humidity every 15 minutes
- Turn ON the control device when humidity > 45%
- Turn OFF the control device when humidity ≤ 45%
- Log all activities to `request_logs.log`

# Rust

### Prerequisites
1. **Rust and Cargo Installation on Desktop**:
   Ensure you have Rust installed on your desktop. If not, you can install it using the following command:
   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Install Dependencies**:
   Ensure you have the necessary dependencies for cross-compilation:
   ```sh
   sudo apt-get update
   sudo apt-get install gcc-arm-linux-gnueabihf
   ```

### Steps to Cross-Compile for Raspberry Pi
1. **Add ARM Target**:
   Add the ARM target to your Rust toolchain:
   ```sh
   rustup target add armv7-unknown-linux-gnueabihf
   ```

2. **Create a `.cargo` Directory**:
   In the root of your Rust project, create a `.cargo` directory and add a `config` file with the following content:
   ```toml
   [target.armv7-unknown-linux-gnueabihf]
   linker = "arm-linux-gnueabihf-gcc"
   ```

3. you might need a manual installation of OpenSSL:
   ```sh
   wget https://www.openssl.org/source/openssl-1.1.1.tar.gz
   tar -xf openssl-1.1.1.tar.gz
   cd openssl-1.1.1
   ./config --prefix=/usr/local/ssl --openssldir=/usr/local/ssl shared zlib
   make
   sudo make install
   export OPENSSL_DIR=/usr/local/ssl
   ```

4. **Build the Project for ARM**:
   Build your project for the ARM architecture:
   ```sh
   export OPENSSL_DIR=/usr/local/ssl
   export PKG_CONFIG_PATH=/usr/local/ssl/lib/pkgconfig
   cargo build --release --target=armv7-unknown-linux-gnueabihf
   ```

### Transfer the Binary to Raspberry Pi
1. **Copy the Binary**:
   Copy the compiled binary to your Raspberry Pi. You can use `scp` (secure copy) for this:
   ```sh
   scp target/armv7-unknown-linux-gnueabihf/release/humidity_checker pi@raspberrypi:/home/pi/humidity_checker
   ```

2. **Copy the `api_key.secret` File**:
   Ensure the `api_key.secret` file is also transferred to the Raspberry Pi:
   ```sh
   scp api_key.secret pi@raspberrypi:/home/pi/api_key.secret
   ```

### Running the Application on Raspberry Pi
1. **SSH into Raspberry Pi**:
   SSH into your Raspberry Pi:
   ```sh
   ssh pi@raspberrypi
   ```

2. **Install Required Libraries**:
   Ensure you have the required libraries installed on your Raspberry Pi:
   ```sh
   sudo apt-get update
   sudo apt-get install libssl-dev
   ```

3. **Run the Application**:
   Run the transferred binary:
   ```sh
   ./humidity_checker
   ```

### Optional: Running as a Background Service
To ensure the program runs continuously, you might want to set it up as a systemd service:

1. **Create a Service File**:
   On your Raspberry Pi, create a new service file, e.g., `humidity_checker.service`:
   ```ini
   [Unit]
   Description=Humidity Checker Service
   After=network.target
   [Service]
   ExecStart=/home/pi/humidity_checker
   WorkingDirectory=/home/pi
   StandardOutput=inherit
   StandardError=inherit
   Restart=always
   User=pi
   [Install]
   WantedBy=multi-user.target
   ```

2. **Move the Service File**:
   ```sh
   sudo mv humidity_checker.service /etc/systemd/system/
   ```

3. **Reload Systemd and Enable the Service**:
   ```sh
   sudo systemctl daemon-reload
   sudo systemctl enable humidity_checker.service
   sudo systemctl start humidity_checker.service
   ```

This setup will ensure your Rust application runs continuously and restarts automatically if it crashes or your Raspberry Pi reboots.
