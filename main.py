import configparser
import logging
import os
import signal
import sys
import time
import uuid

import requests

check_interval = 900  # Check every 15 minutes

# Configure logging
log_file = os.path.join(os.path.dirname(__file__), "request_logs.log")
logging.basicConfig(
    filename=log_file,
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s",
)


def load_api_key(file_path):
    with open(file_path, "r") as file:
        api_key = file.read().strip()
    return api_key


def load_device_config(config_path):
    """Load device configuration from config file"""
    if not os.path.exists(config_path):
        logging.error(f"Device config file not found: {config_path}")
        logging.error("Please copy devices.config.template to devices.config and fill in your device information")
        sys.exit(1)
    
    config = configparser.ConfigParser()
    config.read(config_path)
    
    device_config = {
        'humidity_sensor': {
            'sku': config.get('humidity_sensor', 'sku'),
            'device': config.get('humidity_sensor', 'device')
        },
        'control_device': {
            'sku': config.get('control_device', 'sku'),
            'device': config.get('control_device', 'device')
        }
    }
    
    return device_config


def check_humidity(api_key, device_config):
    try:
        url = "https://openapi.api.govee.com/router/api/v1/device/state"
        headers = {"Content-Type": "application/json", "Govee-API-Key": api_key}
        payload = {
            "requestId": "uuid",
            "payload": {
                "sku": device_config['humidity_sensor']['sku'], 
                "device": device_config['humidity_sensor']['device']
            },
        }
        request_id = str(uuid.uuid4())
        payload["requestId"] = request_id
        response = requests.post(url, headers=headers, json=payload, timeout=10)
        if response.status_code == 200:
            data = response.json()
            # Iterate through capabilities to find humidity sensor
            for capability in data.get("payload", {}).get("capabilities", []):
                if capability.get("instance") == "sensorHumidity":
                    # Extract the humidity value directly from 'value'
                    return capability.get("state", {}).get("value")
            logging.error("Humidity capability not found in response")
            return None
        else:
            logging.error(
                f"Failed to check humidity. Status code: {response.status_code}"
            )
            return None
    except Exception as e:
        logging.error(f"Exception occurred in check_humidity: {e}")
        return None


def control_device(api_key, value, device_config):
    try:
        url = "https://openapi.api.govee.com/router/api/v1/device/control"
        headers = {"Content-Type": "application/json", "Govee-API-Key": api_key}
        payload = {
            "requestId": "uuid",
            "payload": {
                "sku": device_config['control_device']['sku'],
                "device": device_config['control_device']['device'],
                "capability": {
                    "type": "devices.capabilities.on_off",
                    "instance": "powerSwitch",
                    "value": value,
                },
            },
        }
        response = requests.post(url, headers=headers, json=payload, timeout=10)
        if response.status_code != 200:
            logging.error(
                f"Failed to control device. Status code: {response.status_code}"
            )
        return response.status_code
    except Exception as e:
        logging.error(f"Exception occurred in control_device: {e}")
        return None


def signal_handler(sig, frame):
    logging.info("Gracefully shutting down...")
    sys.exit(0)


# Register signal handler for graceful shutdown
signal.signal(signal.SIGINT, signal_handler)

# Load API key from file in relative location
file_path = os.path.join(os.path.dirname(__file__), "api_key.secret")
api_key = load_api_key(file_path)

# Load device configuration
config_path = os.path.join(os.path.dirname(__file__), "devices.config")
device_config = load_device_config(config_path)

logging.info("Starting humidity control service...")
logging.info(f"Monitoring humidity sensor: {device_config['humidity_sensor']['sku']}")
logging.info(f"Controlling device: {device_config['control_device']['sku']}")

while True:
    humidity = check_humidity(api_key, device_config)
    if humidity is not None:
        logging.info(f"Current humidity: {humidity}%")
        if humidity > 45:
            result = control_device(api_key, 1, device_config)  # Turn on device
            if result == 200:
                logging.info("Device turned ON (humidity > 45%)")
        else:
            result = control_device(api_key, 0, device_config)  # Turn off device
            if result == 200:
                logging.info("Device turned OFF (humidity <= 45%)")
    else:
        logging.warning("Failed to read humidity data")
    time.sleep(check_interval)
