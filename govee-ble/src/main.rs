use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use btleplug::api::{
    bleuuid::BleUuid, Central, CharPropFlags, Manager as _, Peripheral, ScanFilter, WriteType,
};
use std::time::Duration;
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
    let mut block = GenericArray::clone_from_slice(&payload[..16]);
    cipher.decrypt_block(&mut block);
    let mut out = [0u8; 20];
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
async fn adapter() -> btleplug::platform::Adapter {
    let m = Manager::new().await.unwrap();
    m.adapters().await.unwrap().into_iter().next().unwrap()
}

async fn find_by_mac(
    central: &btleplug::platform::Adapter,
    mac: &str,
    timeout_s: u64,
) -> Result<btleplug::platform::Peripheral, String> {
    let mac = mac.to_uppercase();
    central
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| format!("scan: {e}"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_s);
    loop {
        for p in central.peripherals().await.map_err(|e| format!("periphs: {e}"))? {
            if p.address().to_string().to_uppercase() == mac {
                central.stop_scan().await.ok();
                return Ok(p);
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    central.stop_scan().await.ok();
    Err(format!("{mac} not found"))
}

async fn write_char(periph: &btleplug::platform::Peripheral, data: &[u8]) -> Result<(), String> {
    for svc in &periph.services() {
        for chr in &svc.characteristics {
            if chr.uuid.to_string().to_lowercase().contains("2b11") {
                let wt = if chr.properties.contains(CharPropFlags::WRITE) {
                    WriteType::WithResponse
                } else {
                    WriteType::WithoutResponse
                };
                periph
                    .write(chr, data, wt)
                    .await
                    .map_err(|e| format!("write: {e}"))?;
                return Ok(());
            }
        }
    }
    Err("write char not found".into())
}

async fn subscribe_notify(
    periph: &btleplug::platform::Peripheral,
) -> Result<Vec<u8>, String> {
    // Find the notify characteristic
    let mut notify_chr = None;
    for svc in &periph.services() {
        for chr in &svc.characteristics {
            if chr.uuid.to_string().to_lowercase().contains("2b10") {
                notify_chr = Some(chr.clone());
                break;
            }
        }
    }
    let chr = notify_chr.ok_or("notify char not found")?;
    periph
        .subscribe(&chr)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;

    // Get the notification stream and wait for one notification
    let mut stream = periph
        .notifications()
        .await
        .map_err(|e| format!("notifs: {e}"))?;

    // Read a single notification with timeout
    let result = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .map_err(|_| "notification timeout".to_string())?
        .ok_or("stream ended".to_string())?;
    Ok(result.value)
}

use futures::StreamExt;

// ========================= H5179 SENSOR =========================
fn parse_h5179(data: &[u8]) -> Option<(f32, u8, u8)> {
    if data.len() < 7 || data[0] != 0x88 || data[1] != 0xEC {
        return None;
    }
    let temp = ((data[3] as i16 - 100) as f32) + data[4] as f32 / 10.0;
    Some((temp, data[5], data[6]))
}

async fn read_sensor(mac: &str, timeout_s: u64) -> Result<(f32, u8, u8), String> {
    let central = adapter().await;
    let mac = mac.to_uppercase();
    central
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| format!("scan: {e}"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_s);
    loop {
        for p in central.peripherals().await.map_err(|e| format!("periphs: {e}"))? {
            if p.address().to_string().to_uppercase() != mac {
                continue;
            }
            if let Ok(Some(props)) = p.properties().await {
                if let Some(data) = props.manufacturer_data.get(&0xEC88) {
                    if let Some(r) = parse_h5179(data) {
                        central.stop_scan().await.ok();
                        return Ok(r);
                    }
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    central.stop_scan().await.ok();
    Err("H5179 not found".into())
}

// ========================= H5080 PLUG =========================
/// Connect, handshake, init, then call `action(periph, session_key)`.
/// Disconnects after action returns.
async fn with_plug<Fut>(
    mac: &str,
    action: impl FnOnce(btleplug::platform::Peripheral, [u8; 16]) -> Fut,
) -> Result<Fut::Output, String>
where
    Fut: std::future::Future,
{
    let mut last_err = String::new();
    for attempt in 0..3 {
        let result = try_with_plug(mac, &action).await;
        match result {
            Ok(a) => return Ok(a),
            Err(e) => {
                last_err = e;
                log::warn!("retry {}/3: {last_err}", attempt + 1);
                sleep(Duration::from_secs(2u64.pow(attempt))).await;
            }
        }
    }
    Err(format!("3 retries exhausted: {last_err}"))
}

async fn try_with_plug<Fut>(
    mac: &str,
    action: impl FnOnce(btleplug::platform::Peripheral, [u8; 16]) -> Fut,
) -> Result<Fut::Output, String>
where
    Fut: std::future::Future,
{
    let central = adapter().await;
    let periph = find_by_mac(&central, mac, 10).await?;
    periph
        .connect()
        .await
        .map_err(|e| format!("connect: {e}"))?;
    sleep(Duration::from_millis(500)).await;

    // Subscribe notify
    let mut notify_chr = None;
    for svc in &periph.services() {
        for chr in &svc.characteristics {
            if chr.uuid.to_string().to_lowercase().contains("2b10") {
                notify_chr = Some(chr.clone());
                break;
            }
        }
    }
    let nc = notify_chr.ok_or("notify char not found")?;
    periph
        .subscribe(&nc)
        .await
        .map_err(|e| format!("sub: {e}"))?;
    let mut stream = periph
        .notifications()
        .await
        .map_err(|e| format!("notifs: {e}"))?;

    // Handshake: E7 01
    write_char(
        &periph,
        &encrypt(&frame_from(0xE7, 0x01, &[0u8; 16]), KEY_COMM),
    )
    .await?;

    // Wait for E7 01 response (session key)
    let mut sk = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while sk.is_none() && tokio::time::Instant::now() < deadline {
        let n = tokio::time::timeout(Duration::from_millis(200), stream.next())
            .await
            .ok()
            .and_then(std::convert::identity);
        if let Some(v) = n {
            let mut buf = [0u8; 20];
            let len = v.value.len().min(20);
            buf[..len].copy_from_slice(&v.value[..len]);
            let d = decrypt(&buf, KEY_COMM);
            if d[0] == 0xE7 && d[1] == 0x01 && verify(&d) {
                let mut k = [0u8; 16];
                k.copy_from_slice(&d[2..18]);
                sk = Some(k);
            }
        }
    }
    let sk = sk.ok_or("handshake failed (no E7 01 response)")?;

    // E7 02 confirm
    write_char(&periph, &encrypt(&frame_from(0xE7, 0x02, &[0u8; 16]), KEY_COMM)).await?;

    // Init sequence
    write_char(&periph, &encrypt(&frame_from(0xAA, 0xEF, &[]), &sk)).await?;
    write_char(
        &periph,
        &encrypt(
            &frame_from(0x33, 0xB2, &[0x3C, 0x9C, 0x9D, 0x89, 0x09, 0x40, 0xB0, 0x19]),
            &sk,
        ),
    )
    .await?;
    write_char(
        &periph,
        &encrypt(
            &frame_from(0x33, 0xB5, &[0x6A, 0xA1, 0xBB, 0xA7, 0x01, 0xFC]),
            &sk,
        ),
    )
    .await?;
    sleep(Duration::from_millis(500)).await;
    write_char(&periph, &encrypt(&frame_from(0xAA, 0xB0, &[]), &sk)).await?;
    write_char(&periph, &encrypt(&frame_from(0xAA, 0xB0, &[0x00, 0x01]), &sk)).await?;
    write_char(&periph, &encrypt(&frame_from(0xAA, 0x12, &[]), &sk)).await?;
    write_char(&periph, &encrypt(&frame_from(0xAA, 0x13, &[]), &sk)).await?;
    sleep(Duration::from_millis(500)).await;

    let result = action(periph.clone(), sk).await;
    periph.disconnect().await.ok();
    Ok(result)
}

/// Check plug state. Returns true if ON.
async fn plug_check_state(
    periph: &btleplug::platform::Peripheral,
    sk: &[u8; 16],
    stream: &mut (impl futures::Stream<Item = btleplug::api::ValueNotification> + std::marker::Unpin),
) -> Result<bool, String> {
    write_char(periph, &encrypt(&frame_from(0xAA, 0x01, &[]), sk)).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let n = tokio::time::timeout(Duration::from_millis(200), stream.next())
            .await
            .ok()
            .and_then(std::convert::identity);
        if let Some(v) = n {
            let mut buf = [0u8; 20];
            let len = v.value.len().min(20);
            buf[..len].copy_from_slice(&v.value[..len]);
            let d = decrypt(&buf, sk);
            if d[0] == 0xAA && d[1] == 0x01 && verify(&d) {
                return Ok(d[2] == 1);
            }
        }
    }
    Err("no state response".into())
}

// ========================= HEALTHCHECK =========================
// ponytail: plain TCP GET, no TLS. Use http:// URLs.
async fn ping_hc(url: &str, fail: bool) {
    if url.is_empty() {
        return;
    }
    let path = if fail {
        format!("{}/fail", url.trim_end_matches('/'))
    } else {
        url.to_string()
    };
    let path = path.strip_prefix("http://").unwrap_or(&path);
    let (host, rest) = path.split_once('/').unwrap_or((path, ""));
    let req_path = if rest.is_empty() {
        "/".to_string()
    } else {
        format!("/{rest}")
    };
    if let Ok(mut stream) = tokio::net::TcpStream::connect(format!("{host}:80")).await {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let req = format!("GET {req_path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(req.as_bytes()).await;
        let mut buf = [0u8; 64];
        let _ = stream.read(&mut buf).await;
    }
}

// ========================= DAEMON =========================
async fn daemon_loop(interval_s: u64, threshold: u8, hc_url: String) {
    log::info!("daemon: interval={interval_s}s threshold={threshold}%");
    let mut last_on: Option<bool> = None;
    loop {
        match read_sensor(SENSOR_MAC, 10).await {
            Ok((temp, hum, batt)) => {
                log::info!("sensor: {temp:.1}°C {hum}% batt={batt}%");
                let need_on = hum > threshold;
                if last_on.map(|on| on != need_on).unwrap_or(true) {
                    log::info!("toggling plug {}", if need_on { "ON" } else { "OFF" });
                    if need_on {
                        let _ = with_plug(PLUG_MAC, |p, sk| Box::pin(async move { plug_on_or_off(p, true, sk).await })).await;
                    } else {
                        let _ = with_plug(PLUG_MAC, |p, sk| Box::pin(async move { plug_on_or_off(p, false, sk).await })).await;
                    }
                    last_on = Some(need_on);
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

// ponytail: writes the toggle command. Doesn't verify the plug actually toggled.
// Add a get_state() + retry if false positives (heresy the plug) ever happen.
async fn plug_on_or_off(p: btleplug::platform::Peripheral, on: bool, sk: [u8; 16]) {
    let data = if on { &[0x11] } else { &[0x10] };
    let _ = write_char(&p, &encrypt(&frame_from(0x33, 0x01, data), &sk)).await;
    sleep(Duration::from_millis(500)).await;
}

// ========================= CLI =========================
fn get_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: govee-ble <read|on|off|status|daemon>");
        eprintln!("       govee-ble daemon [--interval SEC] [--threshold PCT] [--hc-url URL]");
        return;
    }
    match args[1].as_str() {
        "read" => match read_sensor(SENSOR_MAC, 10).await {
            Ok((t, h, b)) => println!("{t:.1}°C {h}% {b}%"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        "on" => match with_plug(PLUG_MAC, |p, sk| Box::pin(plug_on_or_off(p, true, sk))).await {
            Ok(_) => println!("ON"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        "off" => match with_plug(PLUG_MAC, |p, sk| Box::pin(plug_on_or_off(p, false, sk))).await {
            Ok(_) => println!("OFF"),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        "status" => {
            // Need notification stream for status, so inline the connection
            let central = adapter().await;
            match find_by_mac(&central, PLUG_MAC, 10).await {
                Ok(periph) => {
                    periph.connect().await.unwrap();
                    sleep(Duration::from_millis(500)).await;
                    // subscribe + stream
                    let notify_chr = {
                        let mut c = None;
                        for svc in &periph.services() {
                            for chr in &svc.characteristics {
                                if chr.uuid.to_string().to_lowercase().contains("2b10") {
                                    c = Some(chr.clone());
                                    break;
                                }
                            }
                        }
                        c
                    }
                    .expect("notify char");
                    periph.subscribe(&notify_chr).await.unwrap();
                    let mut stream = periph.notifications().await.unwrap();

                    // handshake
                    write_char(
                        &periph,
                        &encrypt(&frame_from(0xE7, 0x01, &[0u8; 16]), KEY_COMM),
                    )
                    .await
                    .unwrap();
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
                    let mut sk = None;
                    while sk.is_none() && tokio::time::Instant::now() < deadline {
                        let n = tokio::time::timeout(Duration::from_millis(200), stream.next())
                            .await
                            .ok()
                            .and_then(std::convert::identity);
                        if let Some(v) = n {
                            let mut buf = [0u8; 20];
                            buf[..v.value.len().min(20)].copy_from_slice(&v.value[..v.value.len().min(20)]);
                            let d = decrypt(&buf, KEY_COMM);
                            if d[0] == 0xE7 && d[1] == 0x01 && verify(&d) {
                                let mut k = [0u8; 16];
                                k.copy_from_slice(&d[2..18]);
                                sk = Some(k);
                            }
                        }
                    }
                    let sk = sk.expect("handshake");
                    write_char(
                        &periph,
                        &encrypt(&frame_from(0xE7, 0x02, &[0u8; 16]), KEY_COMM),
                    )
                    .await
                    .unwrap();
                    // init
                    write_char(&periph, &encrypt(&frame_from(0xAA, 0xEF, &[]), &sk)).await.unwrap();
                    write_char(&periph, &encrypt(&frame_from(0x33, 0xB2, &[0x3C, 0x9C, 0x9D, 0x89, 0x09, 0x40, 0xB0, 0x19]), &sk)).await.unwrap();
                    write_char(&periph, &encrypt(&frame_from(0x33, 0xB5, &[0x6A, 0xA1, 0xBB, 0xA7, 0x01, 0xFC]), &sk)).await.unwrap();
                    sleep(Duration::from_millis(500)).await;
                    write_char(&periph, &encrypt(&frame_from(0xAA, 0xB0, &[]), &sk)).await.unwrap();
                    write_char(&periph, &encrypt(&frame_from(0xAA, 0xB0, &[0x00, 0x01]), &sk)).await.unwrap();
                    write_char(&periph, &encrypt(&frame_from(0xAA, 0x12, &[]), &sk)).await.unwrap();
                    write_char(&periph, &encrypt(&frame_from(0xAA, 0x13, &[]), &sk)).await.unwrap();
                    sleep(Duration::from_millis(500)).await;
                    // query
                    match plug_check_state(&periph, &sk, &mut stream).await {
                        Ok(state) => println!("{}", if state { "ON" } else { "OFF" }),
                        Err(e) => eprintln!("{e}"),
                    }
                    periph.disconnect().await.ok();
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        "daemon" => {
            env_logger::init();
            daemon_loop(
                get_arg(&args, "--interval")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(900),
                get_arg(&args, "--threshold")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(45),
                get_arg(&args, "--hc-url").unwrap_or_default(),
            )
            .await;
        }
        _ => {
            eprintln!("unknown subcommand: {}", args[1]);
            std::process::exit(1);
        }
    }
}