use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral, ScanFilter, WriteType};
use btleplug::platform::Adapter;
use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;

// ========================= CONSTANTS =========================
const KEY_COMM: &[u8; 16] = b"MakingLifeSmarte";
const PLUG_MAC: &str = "60:74:F4:BD:4D:E5";
const SENSOR_MAC: &str = "E3:32:81:12:40:A4";

// ========================= CRYPTO =========================
fn rc4(data: &[u8], key: &[u8]) -> Vec<u8> {
    let mut s: [u8; 256] = std::array::from_fn(|i| i as u8);
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        (s[i], s[j as usize]) = (s[j as usize], s[i]);
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|&b| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            (s[i as usize], s[j as usize]) = (s[j as usize], s[i as usize]);
            b ^ s[s[i as usize].wrapping_add(s[j as usize]) as usize]
        })
        .collect()
}

fn encrypt(frame: &[u8; 20], key: &[u8; 16]) -> [u8; 20] {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut out = [0u8; 20];
    let mut block = GenericArray::clone_from_slice(&frame[..16]);
    cipher.encrypt_block(&mut block);
    out[..16].copy_from_slice(&block);
    let rc = rc4(&frame[16..], key);
    out[16..].copy_from_slice(&rc);
    out
}

fn decrypt(payload: &[u8; 20], key: &[u8; 16]) -> [u8; 20] {
    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut out = [0u8; 20];
    let mut block = GenericArray::clone_from_slice(&payload[..16]);
    cipher.decrypt_block(&mut block);
    out[..16].copy_from_slice(&block);
    let rc = rc4(&payload[16..], key);
    out[16..].copy_from_slice(&rc);
    out
}

fn frame_from(cmd: u8, sub: u8, data: &[u8]) -> [u8; 20] {
    let mut f = [0u8; 20];
    f[0] = cmd;
    f[1] = sub;
    let n = data.len().min(17);
    f[2..2 + n].copy_from_slice(&data[..n]);
    let cs = f[..19].iter().fold(0, |a, b| a ^ b);
    f[19] = cs;
    f
}

fn verify(frame: &[u8; 20]) -> bool {
    frame[..19].iter().fold(0, |a, b| a ^ b) == frame[19]
}

// ========================= BLE HELPERS =========================
async fn first_adapter() -> Result<Adapter, String> {
    let manager = btleplug::platform::Manager::new()
        .await
        .map_err(|e| format!("BLE manager: {e}"))?;
    let adapters = manager.adapters().await.map_err(|e| format!("No adapters: {e}"))?;
    adapters.into_iter().next().ok_or("No BLE adapter".into())
}

// ========================= H5179 SENSOR =========================
fn parse_h5179(data: &[u8]) -> Option<(f32, u8, u8)> {
    if data.len() < 7 || data[0] != 0x88 || data[1] != 0xEC {
        return None;
    }
    let temp = ((data[3] as i16 - 100) as f32) + data[4] as f32 / 10.0;
    Some((temp, data[5], data[6]))
}

async fn read_sensor(mac: &str, timeout_s: u64) -> Result<(f32, u8, u8), String> {
    let central = first_adapter().await?;
    central.start_scan(ScanFilter::default()).await.map_err(|e| format!("Scan: {e}"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_s);

    while tokio::time::Instant::now() < deadline {
        for p in central.peripherals().await.map_err(|e| format!("Periphs: {e}"))? {
            if p.address().await.map(|a| a.to_string().to_uppercase() == mac).unwrap_or(false) {
                if let Some(mfr) = p.properties().await.and_then(|p| p.manufacturer_data) {
                    if let Some(data) = mfr.get(&0xEC88) {
                        if let Some(r) = parse_h5179(data) {
                            central.stop_scan().await.ok();
                            return Ok(r);
                        }
                    }
                }
            }
        }
        sleep(Duration::from_millis(200)).await;
    }
    central.stop_scan().await.ok();
    Err("H5179 not found".into())
}

// ========================= H5080 PLUG =========================
struct PlugCtrl {
    peripheral: btleplug::platform::Peripheral,
    notifs: Arc<Mutex<Vec<[u8; 20]>>>,
    sk: Option<[u8; 16]>,
}

impl PlugCtrl {
    async fn new(mac: &str) -> Result<Self, String> {
        let central = first_adapter().await?;
        central.start_scan(ScanFilter::default()).await.map_err(|e| format!("Scan: {e}"))?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

        let peripheral = loop {
            for p in central.peripherals().await.map_err(|e| format!("Periphs: {e}"))? {
                if p.address().await.map(|a| a.to_string().to_uppercase() == mac).unwrap_or(false) {
                    break p;
                }
            }
            if tokio::time::Instant::now() >= deadline {
                central.stop_scan().await.ok();
                return Err("Plug not found".into());
            }
            sleep(Duration::from_millis(200)).await;
        };
        central.stop_scan().await.ok();

        let notifs = Arc::new(Mutex::new(Vec::new()));
        {
            let n = notifs.clone();
            peripheral.on_notification(Some(Box::new(move |data: &[u8]| {
                let mut buf = [0u8; 20];
                let len = data.len().min(20);
                buf[..len].copy_from_slice(&data[..len]);
                // ponytail: blocking_lock in notifications — risk of brief stalling under heavy load.
                // Swap to channel if btleplug ever floods, but for one device this is fine.
                n.blocking_lock().push(buf);
            })));
        }
        peripheral.connect().await.map_err(|e| format!("Connect: {e}"))?;
        sleep(Duration::from_millis(500)).await;

        // subscribe to the notify characteristic
        for svc in peripheral.services().await.map_err(|e| format!("Svcs: {e}"))? {
            for chr in &svc.characteristics {
                let uuid = chr.uuid.to_string().to_lowercase();
                if uuid.contains("2b10") {
                    peripheral.subscribe(chr).await.map_err(|e| format!("Subscribe: {e}"))?;
                    break;
                }
            }
        }

        Ok(PlugCtrl { peripheral, notifs, sk: None })
    }

    async fn write_raw(&self, data: &[u8]) -> Result<(), String> {
        for svc in self.peripheral.services().await.map_err(|e| format!("Svcs: {e}"))? {
            for chr in &svc.characteristics {
                let uuid = chr.uuid.to_string().to_lowercase();
                if uuid.contains("2b11") {
                    let wt = if chr.properties.contains(CharPropFlags::WRITE) {
                        WriteType::WithResponse
                    } else {
                        WriteType::WithoutResponse
                    };
                    self.peripheral.write(chr, data, wt).await.map_err(|e| format!("Write: {e}"))?;
                    return Ok(());
                }
            }
        }
        Err("Write char not found".into())
    }

    async fn write_enc(&self, key: &[u8; 16], frame: &[u8; 20]) -> Result<(), String> {
        self.write_raw(&encrypt(frame, key)).await
    }

    async fn handshake(&mut self) -> Result<(), String> {
        let r1 = frame_from(0xE7, 0x01, &[0u8; 16]);
        self.write_enc(KEY_COMM, &r1).await?;
        sleep(Duration::from_millis(1500)).await;

        let mut n = self.notifs.lock().await;
        for notif in n.drain(..) {
            let d = decrypt(&notif, KEY_COMM);
            if d[0] == 0xE7 && d[1] == 0x01 && verify(&d) {
                let mut sk = [0u8; 16];
                sk.copy_from_slice(&d[2..18]);
                self.sk = Some(sk);
                break;
            }
        }

        let sk = self.sk.ok_or("Handshake: no E7 01 response")?;
        let r2 = frame_from(0xE7, 0x02, &[0u8; 16]);
        self.write_enc(KEY_COMM, &r2).await?;
        self.sk = Some(sk);
        Ok(())
    }

    async fn init(&self) -> Result<(), String> {
        let sk = self.sk.ok_or("No session key")?;
        self.write_enc(&sk, &frame_from(0xAA, 0xEF, &[])).await?;
        self.write_enc(&sk, &frame_from(0x33, 0xB2, &[0x3C, 0x9C, 0x9D, 0x89, 0x09, 0x40, 0xB0, 0x19])).await?;
        self.write_enc(&sk, &frame_from(0x33, 0xB5, &[0x6A, 0xA1, 0xBB, 0xA7, 0x01, 0xFC])).await?;
        sleep(Duration::from_millis(500)).await;
        self.write_enc(&sk, &frame_from(0xAA, 0xB0, &[])).await?;
        self.write_enc(&sk, &frame_from(0xAA, 0xB0, &[0x00, 0x01])).await?;
        self.write_enc(&sk, &frame_from(0xAA, 0x12, &[])).await?;
        self.write_enc(&sk, &frame_from(0xAA, 0x13, &[])).await?;
        sleep(Duration::from_millis(500)).await;
        Ok(())
    }

    async fn get_state(&mut self) -> Result<bool, String> {
        let sk = self.sk.ok_or("No session key")?;
        self.write_enc(&sk, &frame_from(0xAA, 0x01, &[])).await?;
        sleep(Duration::from_millis(1000)).await;
        let mut n = self.notifs.lock().await;
        for notif in n.drain(..) {
            let d = decrypt(&notif, &sk);
            if d[0] == 0xAA && d[1] == 0x01 && verify(&d) {
                return Ok(d[2] == 1);
            }
        }
        Err("No state response".into())
    }

    async fn turn(&self, on: bool) -> Result<(), String> {
        let sk = self.sk.ok_or("No session key")?;
        self.write_enc(&sk, &frame_from(0x33, 0x01, if on { &[0x11] } else { &[0x10] })).await
    }

    async fn disconnect(self) {
        let _ = self.peripheral.disconnect().await;
    }
}

// ========================= RETRY WRAPPER =========================
async fn with_plug<Fut>(mac: &str, f: impl Fn(&mut PlugCtrl) -> Fut) -> Result<Fut::Output, String>
where
    Fut: std::future::Future,
{
    let mut last_err = String::new();
    for attempt in 0..3 {
        let mut plug = match PlugCtrl::new(mac).await {
            Ok(p) => p,
            Err(e) => { last_err = e; sleep(Duration::from_secs(2u64.pow(attempt))).await; continue; }
        };
        if let Err(e) = plug.handshake().await {
            plug.disconnect().await;
            last_err = e;
            sleep(Duration::from_secs(2u64.pow(attempt))).await;
            continue;
        }
        if let Err(e) = plug.init().await {
            plug.disconnect().await;
            last_err = e;
            sleep(Duration::from_secs(2u64.pow(attempt))).await;
            continue;
        }
        let result = f(&mut plug).await;
        plug.disconnect().await;
        return Ok(result);
    }
    Err(format!("3 retries exhausted: {last_err}"))
}

// ========================= HEALTHCHECK =========================
async fn ping_hc(url: &str, fail: bool) {
    if url.is_empty() { return; }
    let full = if fail { format!("{url}/fail") } else { url.to_string() };
    // ponytail: no TLS lib -> http only. User should use http healthchecks or set up behind a proxy.
    let url = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or(url);
    let (host, path) = url.split_once('/').unwrap_or((url, ""));
    let addr = if fail {
        format!("{host}/fail")
    } else {
        host.to_string()
    };
    // Actually, simpler: just use format! and construct the request properly
    let path_and_query = if fail {
        format!("/{path}/fail")
    } else if path.is_empty() {
        "/".to_string()
    } else {
        format!("/{path}")
    };
    
    // ponytail: plain TCP GET. Won't work over TLS — user needs http:// URL or a local relay.
    // Most healthcheck services (healthchecks.io, uptimerobot) support http.
    let addr = format!("{}:80", host);
    if let Ok(mut stream) = tokio::net::TcpStream::connect(&addr).await {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let req = format!("GET {path_and_query} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        if stream.write_all(req.as_bytes()).await.is_ok() {
            let mut buf = [0u8; 128];
            let _ = stream.read(&mut buf).await;
        }
    }
}

// ========================= DAEMON =========================
async fn daemon_loop(interval_s: u64, threshold: u8, hc_url: String) {
    log::info!("daemon start: interval={interval_s}s threshold={threshold}%");
    let mut last_on: Option<bool> = None;
    loop {
        match read_sensor(SENSOR_MAC, 10).await {
            Ok((temp, hum, batt)) => {
                log::info!("sensor: {temp:.1}°C {hum}% batt={batt}%");
                let need_on = hum > threshold;
                let should_toggle = match last_on {
                    None => true,
                    Some(on) => on != need_on,
                };
                if should_toggle {
                    let action = if need_on { "ON" } else { "OFF" };
                    log::info!("toggling plug {action}");
                    match with_plug(PLUG_MAC, |plug| async move { plug.turn(need_on).await }).await {
                        Ok(_) => { last_on = Some(need_on); log::info!("-> {action} ok"); }
                        Err(e) => log::error!("toggle failed: {e}"),
                    }
                }
                ping_hc(&hc_url, false).await;
            }
            Err(e) => {
                log::error!("sensor: {e}");
                ping_hc(&hc_url, true).await;
            }
        }
        sleep(Duration::from_secs(interval_s)).await;
    }
}

// ========================= CLI =========================
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: govee-ble <read|on|off|status|daemon> [--interval N] [--threshold N] [--hc-url URL]");
        return;
    }
    let get_arg = |name: &str| {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
    };
    match args[1].as_str() {
        "read" => match read_sensor(SENSOR_MAC, 10).await {
            Ok((t, h, b)) => println!("{t:.1}°C {h}% {b}%"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "on" => match with_plug(PLUG_MAC, |plug| async move { plug.turn(true).await }).await {
            Ok(_) => println!("ON"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "off" => match with_plug(PLUG_MAC, |plug| async move { plug.turn(false).await }).await {
            Ok(_) => println!("OFF"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "status" => match with_plug(PLUG_MAC, |plug| async move { plug.get_state().await }).await {
            Ok(s) => println!("{}", if s { "ON" } else { "OFF" }),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "daemon" => {
            env_logger::init();
            daemon_loop(
                get_arg("--interval").and_then(|v| v.parse().ok()).unwrap_or(900),
                get_arg("--threshold").and_then(|v| v.parse().ok()).unwrap_or(45),
                get_arg("--hc-url").unwrap_or_default(),
            ).await;
        }
        _ => { eprintln!("unknown: {}", args[1]); std::process::exit(1); }
    }
}