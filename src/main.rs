use std::{fs, thread, time::Duration, path::Path};
use log::{info, error, LevelFilter};
use env_logger::Builder;
use serde::{Deserialize, Serialize};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use std::sync::Arc;

const CHECK_INTERVAL: u64 = 900; // Check every 15 minutes

#[derive(Serialize)]
struct HumidityPayload {
    requestId: String,
    payload: DevicePayload,
}

#[derive(Serialize)]
struct DevicePayload {
    sku: String,
    device: String,
}

#[derive(Deserialize)]
struct HumidityResponse {
    payload: HumidityCapabilities,
}

#[derive(Deserialize)]
struct HumidityCapabilities {
    capabilities: Vec<Capability>,
}

#[derive(Deserialize)]
struct Capability {
    state: State,
}

#[derive(Deserialize)]
struct State {
    value: Value,
}

#[derive(Deserialize)]
struct Value {
    currentHumidity: f64,
}

#[derive(Serialize)]
struct ControlPayload {
    requestId: String,
    payload: ControlDevicePayload,
}

#[derive(Serialize)]
struct ControlDevicePayload {
    sku: String,
    device: String,
    capability: CapabilityControl,
}

#[derive(Serialize)]
struct CapabilityControl {
    #[serde(rename = "type")]
    capability_type: String,
    instance: String,
    value: i32,
}

fn load_api_key(file_path: &Path) -> String {
    fs::read_to_string(file_path)
        .expect("Failed to read API key")
        .trim()
        .to_string()
}

fn check_humidity(client: &Client, api_key: &str) -> Option<f64> {
    let url = "https://openapi.api.govee.com/router/api/v1/device/state";
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("Govee-API-Key", HeaderValue::from_str(api_key).unwrap());

    let payload = HumidityPayload {
        requestId: "uuid".to_string(),
        payload: DevicePayload {
            sku: "H5179".to_string(),
            device: "REDACTED_SENSOR_MAC".to_string(),
        },
    };

    let response = client.post(url)
        .headers(headers)
        .json(&payload)
        .send();

    match response {
        Ok(resp) if resp.status().is_success() => {
            let data: HumidityResponse = resp.json().expect("Failed to parse response");
            Some(data.payload.capabilities[2].state.value.currentHumidity)
        },
        Ok(resp) => {
            error!("Failed to check humidity. Status code: {}", resp.status());
            None
        },
        Err(err) => {
            error!("Failed to send request: {}", err);
            None
        }
    }
}

fn control_device(client: &Client, api_key: &str, value: i32) -> bool {
    let url = "https://openapi.api.govee.com/router/api/v1/device/control";
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("Govee-API-Key", HeaderValue::from_str(api_key).unwrap());

    let payload = ControlPayload {
        requestId: "uuid".to_string(),
        payload: ControlDevicePayload {
            sku: "H5080".to_string(),
            device: "REDACTED_CONTROL_MAC".to_string(),
            capability: CapabilityControl {
                capability_type: "devices.capabilities.on_off".to_string(),
                instance: "powerSwitch".to_string(),
                value,
            },
        },
    };

    let response = client.post(url)
        .headers(headers)
        .json(&payload)
        .send();

    match response {
        Ok(resp) if resp.status().is_success() => true,
        Ok(resp) => {
            error!("Failed to control device. Status code: {}", resp.status());
            false
        },
        Err(err) => {
            error!("Failed to send request: {}", err);
            false
        }
    }
}

fn main() {
    // Configure logging
    Builder::new()
        .filter(None, LevelFilter::Info)
        .init();

    let file_path = Path::new("api_key.secret");
    let api_key = Arc::new(load_api_key(file_path));
    let client = Client::new();

    loop {
        let humidity = check_humidity(&client, &api_key);
        if let Some(humidity) = humidity {
            if humidity > 45.0 {
                control_device(&client, &api_key, 1); // Turn on H5080 device
            } else {
                control_device(&client, &api_key, 0); // Turn off H5080 device
            }
        }
        thread::sleep(Duration::from_secs(CHECK_INTERVAL));
    }
}