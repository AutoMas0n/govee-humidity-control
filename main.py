import os
import requests
import time
import logging

check_interval = 900  # Check every 15 minutes

# Configure logging
log_file = os.path.join(os.path.dirname(__file__), 'request_logs.log')
logging.basicConfig(filename=log_file, level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')

def load_api_key(file_path):
    with open(file_path, 'r') as file:
        api_key = file.read().strip()
    return api_key

def check_humidity(api_key):
    url = "https://openapi.api.govee.com/router/api/v1/device/state"
    headers = {
        "Content-Type": "application/json",
        "Govee-API-Key": api_key
    }
    payload = {
        "requestId": "uuid",
        "payload": {
            "sku": "H5179",
            "device": "REDACTED_SENSOR_MAC"
        }
    }
    response = requests.post(url, headers=headers, json=payload)
    if response.status_code == 200:
        data = response.json()
        humidity = data["payload"]["capabilities"][2]["state"]["value"]["currentHumidity"]
        return humidity
    else:
        logging.error(f"Failed to check humidity. Status code: {response.status_code}")
        return None

def control_device(api_key, value):
    url = "https://openapi.api.govee.com/router/api/v1/device/control"
    headers = {
        "Content-Type": "application/json",
        "Govee-API-Key": api_key
    }
    payload = {
        "requestId": "uuid",
        "payload": {
            "sku": "H5080",
            "device": "REDACTED_CONTROL_MAC",
            "capability": {
                "type": "devices.capabilities.on_off",
                "instance": "powerSwitch",
                "value": value
            }
        }
    }
    response = requests.post(url, headers=headers, json=payload)
    if response.status_code != 200:
        logging.error(f"Failed to control device. Status code: {response.status_code}")
    return response.status_code

# Load API key from file in relative location
file_path = os.path.join(os.path.dirname(__file__), 'api_key.secret')
api_key = load_api_key(file_path)

while True:
    humidity = check_humidity(api_key)
    if humidity is not None:
        if humidity > 45:
            control_device(api_key, 1)  # Turn on H5080 device
        else:
            control_device(api_key, 0)  # Turn off H5080 device
    time.sleep(check_interval)