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


def check_humidity(api_key):
    try:
        url = "https://openapi.api.govee.com/router/api/v1/device/state"
        headers = {"Content-Type": "application/json", "Govee-API-Key": api_key}
        payload = {
            "requestId": "uuid",
            "payload": {"sku": "H5179", "device": "REDACTED_SENSOR_MAC"},
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


def control_device(api_key, value):
    try:
        url = "https://openapi.api.govee.com/router/api/v1/device/control"
        headers = {"Content-Type": "application/json", "Govee-API-Key": api_key}
        payload = {
            "requestId": "uuid",
            "payload": {
                "sku": "H5080",
                "device": "REDACTED_CONTROL_MAC",
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

while True:
    humidity = check_humidity(api_key)
    if humidity is not None:
        if humidity > 45:
            control_device(api_key, 1)  # Turn on H5080 device
        else:
            control_device(api_key, 0)  # Turn off H5080 device
    time.sleep(check_interval)
