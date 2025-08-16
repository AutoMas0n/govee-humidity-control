#!/bin/bash

# Govee Humidity Control Setup Script
# This script helps you set up the project with your device configuration

echo "=== Govee Humidity Control Setup ==="
echo

# Check if devices.config exists
if [ -f "devices.config" ]; then
    echo "✓ devices.config already exists"
else
    echo "Setting up device configuration..."
    
    if [ ! -f "devices.config.template" ]; then
        echo "❌ Error: devices.config.template not found!"
        exit 1
    fi
    
    # Copy template
    cp devices.config.template devices.config
    echo "✓ Created devices.config from template"
    echo
    echo "⚠️  IMPORTANT: Please edit devices.config and add your actual device MAC addresses"
    echo "   You need to replace:"
    echo "   - YOUR_HUMIDITY_SENSOR_MAC_ADDRESS with your H5179 device MAC"
    echo "   - YOUR_CONTROL_DEVICE_MAC_ADDRESS with your H5080 device MAC"
    echo
fi

# Check if API key exists
if [ -f "api_key.secret" ]; then
    echo "✓ api_key.secret already exists"
else
    echo "⚠️  API key file not found. Please create 'api_key.secret' with your Govee API key"
fi

# Check Python environment
echo
echo "Checking Python environment..."

if [ -d "venv" ]; then
    echo "✓ Virtual environment exists"
else
    echo "Creating Python virtual environment..."
    python3 -m venv venv
    echo "✓ Virtual environment created"
fi

# Activate venv and install requirements
echo "Installing Python dependencies..."
source venv/bin/activate
pip install --upgrade pip
pip install -r requirements.txt
echo "✓ Dependencies installed"

echo
echo "=== Setup Complete! ==="
echo
echo "Next steps:"
echo "1. Edit 'devices.config' with your actual device MAC addresses"
echo "2. Create 'api_key.secret' with your Govee API key"
echo "3. Run the application with: source venv/bin/activate && python main.py"
echo
echo "For service setup, see Service.md"