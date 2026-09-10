use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use btleplug::api::{
    bleuuid::BleUuid, Central, CharPropFlags, Manager as _, Peripheral, ScanFilter, WriteType,
};
use futures::StreamExt;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

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
    let mut block = GenericArray::clone_from_slice(&frame[..16]);
    cipher.encrypt_block(&mut block);
    let mut out = [0u8; 20];
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

// ========================= BLE =========================
async fn adapter() -> btleplug::platform::Adapter {
    let m = btleplug::platform::Manager::new().await.unwrap();
    m.adapters().await.unwrap().into_iter().next().unwrap()
}

async fn find_mac(c: &btleplug::platform::Adapter, mac: &str, secs: u64) -> Result<btleplug::platform::Peripheral, String> {
    let mac = mac.to_uppercase();
    c.start_scan(ScanFilter::default()).await.map_err(|e| format!("scan: {e}"))?;
    let d = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        for p in c.peripherals().await.map_err(|e| format!("periphs: {e}"))? {
            if p.address().to_string().to_uppercase() == mac {
                c.stop_scan().await.ok();
                return Ok(p);
            }
        }
        if tokio::time::Instant::now() >= d { break; }
        sleep(Duration::from_millis(200)).await;
    }
    c.stop_scan().await.ok();
    Err(format!("{mac} not found"))
}

async fn write_ctrl(per: &btleplug::platform::Peripheral, d: &[u8]) -> Result<(), String> {
    for svc in &per.services() {
        for chr in &svc.characteristics {
            if chr.uuid.to_string().to_lowercase().contains("2b11") {
                let wt = if chr.properties.contains(CharPropFlags::WRITE) {
                    WriteType::WithResponse
                } else {
                    WriteType::WithoutResponse
                };
                per.write(chr, d, wt).await.map_err(|e| format!("write: {e}"))?;
                return Ok(());
            }
        }
    }
    Err("write char 2b11 not found".into())
}

async fn sub_notify(per: &btleplug::platform::Peripheral) -> Result<(), String> {
    for svc in &per.services() {
        for chr in &svc.characteristics {
            if chr.uuid.to_string().to_lowercase().contains("2b10") {
                per.subscribe(chr).await.map_err(|e| format!("sub: {e}"))?;
                return Ok(());
            }
        }
    }
    Err("notify char 2b10 not found".into())
}

// ========================= HANDSHAKE + INIT =========================
async fn handshake(per: &btleplug::platform::Peripheral) -> Result<[u8; 16], String> {
    write_ctrl(per, &encrypt(&frame_from(0xE7, 0x01, &[0u8; 16]), KEY_COMM)).await?;
    let mut s = per.notifications().await.map_err(|e| format!("notif: {e}"))?;
    let d = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut sk = None;
    while sk.is_none() && tokio::time::Instant::now() < d {
        let n = tokio::time::timeout(Duration::from_millis(200), s.next()).await.ok().and_then(|x| x);
        if let Some(v) = n {
            let mut buf = [0u8; 20];
            let l = v.value.len().min(20);
            buf[..l].copy_from_slice(&v.value[..l]);
            let dec = decrypt(&buf, KEY_COMM);
            if dec[0] == 0xE7 && dec[1] == 0x01 && verify(&dec) {
                let mut k = [0u8; 16]; k.copy_from_slice(&dec[2..18]); sk = Some(k);
            }
        }
    }
    let sk = sk.ok_or("handshake: no response")?;
    write_ctrl(per, &encrypt(&frame_from(0xE7, 0x02, &[0u8; 16]), KEY_COMM)).await?;
    Ok(sk)
}

async fn init_plug(per: &btleplug::platform::Peripheral, sk: &[u8; 16]) -> Result<(), String> {
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0xEF, &[]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0x33, 0xB2, &[0x3C,0x9C,0x9D,0x89,0x09,0x40,0xB0,0x19]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0x33, 0xB5, &[0x6A,0xA1,0xBB,0xA7,0x01,0xFC]), sk)).await?;
    sleep(Duration::from_millis(500)).await;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0xB0, &[]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0xB0, &[0x00,0x01]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0x12, &[]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0x13, &[]), sk)).await?;
    sleep(Duration::from_millis(500)).await;
    Ok(())
}

// ========================= PLUG CONNECTION (connect + handshake + init + action) =========================
async fn try_plug_inner<T, Fut>(plug_mac: &str, action: impl FnOnce(btleplug::platform::Peripheral, [u8; 16]) -> Fut) -> Result<T, String>
where Fut: Future<Output = Result<T, String>>,
{
    let c = adapter().await;
    let per = find_mac(&c, plug_mac, 10).await?;
    per.connect().await.map_err(|e| format!("conn: {e}"))?;
    sleep(Duration::from_millis(500)).await;
    per.discover_services().await.map_err(|e| format!("disc svc: {e}"))?;
    sub_notify(&per).await?;
    let sk = handshake(&per).await?;
    init_plug(&per, &sk).await?;
    let r = action(per, sk).await;
    drop(c);
    r
}

// ========================= PLUG COMMANDS (each with own retry) =========================
async fn plug_on(plug_mac: &str) -> Result<(), String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(plug_mac, |per, sk| Box::pin(async move {
            let r = write_ctrl(&per, &encrypt(&frame_from(0x33, 0x01, &[0x11]), &sk)).await;
            per.disconnect().await.ok();
            sleep(Duration::from_millis(500)).await;
            r
        })).await {
            Ok(r) => return Ok(r),
            Err(e) => { err = e; log::warn!("on retry {}/3: {err}", a+1); sleep(Duration::from_secs(2u64.pow(a))).await; }
        }
    }
    Err(format!("plug_on failed: {err}"))
}

async fn plug_off(plug_mac: &str) -> Result<(), String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(plug_mac, |per, sk| Box::pin(async move {
            let r = write_ctrl(&per, &encrypt(&frame_from(0x33, 0x01, &[0x10]), &sk)).await;
            per.disconnect().await.ok();
            sleep(Duration::from_millis(500)).await;
            r
        })).await {
            Ok(r) => return Ok(r),
            Err(e) => { err = e; log::warn!("off retry {}/3: {err}", a+1); sleep(Duration::from_secs(2u64.pow(a))).await; }
        }
    }
    Err(format!("plug_off failed: {err}"))
}

async fn plug_status(plug_mac: &str) -> Result<bool, String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(plug_mac, |per, sk| Box::pin(async move {
            write_ctrl(&per, &encrypt(&frame_from(0xAA, 0x01, &[]), &sk)).await?;
            let mut s = per.notifications().await.map_err(|e| format!("notif: {e}"))?;
            let d = tokio::time::Instant::now() + Duration::from_secs(2);
            while tokio::time::Instant::now() < d {
                let n = tokio::time::timeout(Duration::from_millis(200), s.next()).await.ok().and_then(|x| x);
                if let Some(v) = n {
                    let mut buf = [0u8; 20];
                    let l = v.value.len().min(20);
                    buf[..l].copy_from_slice(&v.value[..l]);
                    let dec = decrypt(&buf, &sk);
                    if dec[0] == 0xAA && dec[1] == 0x01 && verify(&dec) {
                        per.disconnect().await.ok();
                        return Ok(dec[2] == 1);
                    }
                }
            }
            per.disconnect().await.ok();
            Err("no state response".into())
        })).await {
            Ok(r) => return Ok(r),
            Err(e) => { err = e; log::warn!("status retry {}/3: {err}", a+1); sleep(Duration::from_secs(2u64.pow(a))).await; }
        }
    }
    Err(format!("plug_status failed: {err}"))
}

// ========================= H5179 =========================
fn parse_h5179(data: &[u8]) -> Option<(f32, u8, u8)> {
    if data.len() < 7 || data[0] != 0x88 || data[1] != 0xEC { return None; }
    let temp = ((data[3] as i16 - 100) as f32) + data[4] as f32 / 10.0;
    Some((temp, data[5], data[6]))
}

async fn read_sensor(mac: &str, secs: u64) -> Result<(f32, u8, u8), String> {
    let c = adapter().await;
    let mac = mac.to_uppercase();
    c.start_scan(ScanFilter::default()).await.map_err(|e| format!("scan: {e}"))?;
    let d = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        for p in c.peripherals().await.map_err(|e| format!("periphs: {e}"))? {
            if p.address().to_string().to_uppercase() != mac { continue; }
            if let Ok(Some(pr)) = p.properties().await {
                if let Some(data) = pr.manufacturer_data.get(&0xEC88) {
                    if let Some(r) = parse_h5179(data) {
                        c.stop_scan().await.ok();
                        return Ok(r);
                    }
                }
            }
        }
        if tokio::time::Instant::now() >= d { break; }
        sleep(Duration::from_millis(200)).await;
    }
    c.stop_scan().await.ok();
    Err("H5179 not found".into())
}

// ========================= HEALTHCHECK =========================
// ponytail: plain TCP GET, no TLS. Use http:// URLs.
async fn ping_hc(url: &str, fail: bool) {
    if url.is_empty() { return; }
    let path = if fail { format!("{}/fail", url.trim_end_matches('/')) } else { url.to_string() };
    let path = path.strip_prefix("http://").unwrap_or(&path);
    let (host, rest) = path.split_once('/').unwrap_or((path, ""));
    let rp = if rest.is_empty() { "/" } else { &format!("/{rest}") };
    if let Ok(mut s) = tokio::net::TcpStream::connect(format!("{}:80", host)).await {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let req = format!("GET {rp} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        let _ = s.write_all(req.as_bytes()).await;
        let mut b = [0u8; 64];
        let _ = s.read(&mut b).await;
    }
}

// ========================= DAEMON =========================
async fn daemon_loop(interval_s: u64, threshold: u8, hc_url: String,
                     plug_mac: &str, sensor_mac: &str) {
    log::info!("daemon: interval={interval_s}s threshold={threshold}%");
    let mut last_on: Option<bool> = None;
    loop {
        match read_sensor(sensor_mac, 10).await {
            Ok((t, h, b)) => {
                log::info!("sensor: {t:.1}C {h}% batt={b}%");
                let need_on = h > threshold;
                if last_on.map(|o| o != need_on).unwrap_or(true) {
                    log::info!("need {}", if need_on { "ON" } else { "OFF" });
                    if need_on { let _ = plug_on(plug_mac).await; }
                    else { let _ = plug_off(plug_mac).await; }
                    last_on = Some(need_on);
                }
                ping_hc(&hc_url, false).await;
            }
            Err(e) => { log::error!("sensor: {e}"); ping_hc(&hc_url, true).await; }
        }
        sleep(Duration::from_secs(interval_s)).await;
    }
}

// ========================= CLI =========================
fn get_arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i+1)).cloned()
}

fn get_mac(args: &[String], name: &str, default: &str) -> String {
    get_arg(args, name).unwrap_or_else(|| default.to_string())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: govee-ble <read|on|off|status|daemon>");
        eprintln!("  read:   [--mac <addr>]");
        eprintln!("  on/off: [--mac <addr>]");
        eprintln!("  status: [--mac <addr>]");
        eprintln!("  daemon: [--interval SEC] [--threshold PCT] [--hc-url URL]");
        eprintln!("          [--plug-mac <addr>] [--sensor-mac <addr>]");
        eprintln!("  Default plug MAC: {PLUG_MAC}");
        eprintln!("  Default sensor MAC: {SENSOR_MAC}");
        return;
    }
    match args[1].as_str() {
        "read" => match read_sensor(&get_mac(&args, "--mac", SENSOR_MAC), 10).await {
            Ok((t,h,b)) => println!("{t:.1}C {h}% {b}%"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "on" => match plug_on(&get_mac(&args, "--mac", PLUG_MAC)).await {
            Ok(_) => println!("ON"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "off" => match plug_off(&get_mac(&args, "--mac", PLUG_MAC)).await {
            Ok(_) => println!("OFF"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "status" => match plug_status(&get_mac(&args, "--mac", PLUG_MAC)).await {
            Ok(s) => println!("{}", if s { "ON" } else { "OFF" }),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "daemon" => {
            env_logger::init();
            daemon_loop(
                get_arg(&args, "--interval").and_then(|v| v.parse().ok()).unwrap_or(900),
                get_arg(&args, "--threshold").and_then(|v| v.parse().ok()).unwrap_or(45),
                get_arg(&args, "--hc-url").unwrap_or_default(),
                &get_mac(&args, "--plug-mac", PLUG_MAC),
                &get_mac(&args, "--sensor-mac", SENSOR_MAC),
            ).await;
        }
        _ => { eprintln!("unknown: {}", args[1]); std::process::exit(1); }
    }
}