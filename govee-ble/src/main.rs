use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use btleplug::api::{
    Central, CharPropFlags, Manager as _, Peripheral, ScanFilter, WriteType,
};
use futures::StreamExt;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

const KEY_COMM: &[u8; 16] = b"MakingLifeSmarte";
const TZ_HOURS: i8 = -4; // sent in 33 B5 SyncTime; only affects on-device timers
const PLUG_MAC: &str = "60:74:F4:BD:4D:E5";
const SENSOR_MAC: &str = "E3:32:81:12:40:A4";

// ========================= NAMED PLUGS =========================
// The physical plugs have no labels (owner: "there are no labels"), so the
// BLE advertisement suffix (ihoment_H5080_XXXX) is the only authoritative
// identity. This table maps OUR short names to MACs - the name is what we
// call it day-to-day; the MAC drives everything. Keys that are None mean the
// plug is V1 (no secret key needed).
//
// Identity confirmed by live click-tests 2026-09-17:
//   dehumidifier = D4:AD:FC:41:E1:DD (advertises as ihoment_H5080_E1DD)
//                  [confirmed: off/on click at the dehumidifier]
//   pi-side     = 60:74:F4:BD:4D:E5 (advertises as ihoment_H5080_4DE5)
//                  [confirmed: clicked ON for the owner, next to the Pi]
//   e245        = D4:AD:FC:42:E2:45 (advertises as ihoment_H5080_E245)
//                  [unbound 09-17, currently unplugged/out of range]
//
// Edit these names to whatever you call the plugs.
const PLUG_NAMES: &[(&str, Option<&str>, Option<&str>)] = &[
    ("dehumidifier", Some("D4:AD:FC:41:E1:DD"), Some("a69f370afd964e0d")),
    ("pi-side",      Some("60:74:F4:BD:4D:E5"), None),
    ("e245",         Some("D4:AD:FC:42:E2:45"), Some("f6e0730a5be545e3")),
];

fn lookup_plug(name: &str) -> Option<(&str, Option<&str>)> {
    for &(n, mac, skey) in PLUG_NAMES {
        if n == name {
            return mac.map(|m| (m, skey));
        }
    }
    None
}

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

async fn init_plug(per: &btleplug::platform::Peripheral, sk: &[u8; 16], skey: Option<&[u8; 8]>) -> Result<(), String> {
    // Send secret key or version data via 33 B2
    if let Some(k) = skey {
        write_ctrl(per, &encrypt(&frame_from(0x33, 0xB2, k), sk)).await?;
    } else {
        // Default version data (works on V1 firmware)
        write_ctrl(per, &encrypt(&frame_from(0x33, 0xB2, &[0x3C,0x9C,0x9D,0x89,0x09,0x40,0xB0,0x19]), sk)).await?;
    }
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0xEF, &[]), sk)).await?;
    sleep(Duration::from_millis(200)).await;
    // 33 B5 = SyncTime: [unix_ts BE x4][01][tz_hours i8][tz_min]
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
    let mut t = ts.to_be_bytes().to_vec();
    t.extend_from_slice(&[0x01, TZ_HOURS as u8, 0x00]);
    write_ctrl(per, &encrypt(&frame_from(0x33, 0xB5, &t), sk)).await?;
    sleep(Duration::from_millis(200)).await;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0xB0, &[]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0xB0, &[0x00,0x01]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0x12, &[]), sk)).await?;
    write_ctrl(per, &encrypt(&frame_from(0xAA, 0x13, &[]), sk)).await?;
    sleep(Duration::from_millis(500)).await;
    Ok(())
}

// ========================= PLUG CONNECTION (connect + handshake + init + action) =========================
async fn try_plug_inner<T, Fut>(plug_mac: &str, skey: Option<&[u8; 8]>, action: impl FnOnce(btleplug::platform::Peripheral, [u8; 16]) -> Fut) -> Result<T, String>
where Fut: Future<Output = Result<T, String>>,
{
    let c = adapter().await;
    let per = find_mac(&c, plug_mac, 10).await?;
    per.connect().await.map_err(|e| format!("conn: {e}"))?;
    sleep(Duration::from_millis(500)).await;
    per.discover_services().await.map_err(|e| format!("disc svc: {e}"))?;
    sub_notify(&per).await?;
    let sk = handshake(&per).await?;
    init_plug(&per, &sk, skey).await?;
    let r = action(per, sk).await;
    drop(c);
    r
}

// ========================= PLUG COMMANDS (each with own retry) =========================
async fn plug_on(plug_mac: &str, skey: Option<&[u8; 8]>) -> Result<(), String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(plug_mac, skey, |per, sk| Box::pin(async move {
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

async fn plug_off(plug_mac: &str, skey: Option<&[u8; 8]>) -> Result<(), String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(plug_mac, skey, |per, sk| Box::pin(async move {
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

async fn plug_status(plug_mac: &str, skey: Option<&[u8; 8]>) -> Result<bool, String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(plug_mac, skey, |per, sk| Box::pin(async move {
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

// ========================= PAIRING =========================
// Wait up to `secs` for a decrypted frame matching cmd/sub.
type NotifStream = std::pin::Pin<Box<dyn futures::Stream<Item = btleplug::api::ValueNotification> + Send>>;
async fn wait_frame(s: &mut NotifStream, sk: &[u8; 16], cmd: u8, sub: u8, secs: u64) -> Option<[u8; 20]> {
    let d = tokio::time::Instant::now() + Duration::from_secs(secs);
    while tokio::time::Instant::now() < d {
        if let Ok(Some(v)) = tokio::time::timeout(Duration::from_millis(200), s.next()).await {
            let mut buf = [0u8; 20];
            let l = v.value.len().min(20);
            buf[..l].copy_from_slice(&v.value[..l]);
            let dec = decrypt(&buf, sk);
            if dec[0] == cmd && dec[1] == sub && verify(&dec) { return Some(dec); }
        }
    }
    None
}

// App-free pairing, mirrors Govee's AbsPairAc4SecretV1:
//   1. The PLUG must ALREADY be in pairing mode (LED slowly blinking blue). A
//      fresh out-of-box plug is pairable; a BOUND plug only re-enters pairing
//      mode after the Govee app's "forget device", which is a cloud unbind
//      over the plug's WiFi link (deleteDevice -> netService4Base.deleteDevice).
//      There is NO BLE command that enters pairing mode — no such controller
//      in the decompiled H5080 module, and no pre-poll frame in any capture.
//   2. Poll AA B1 until plug answers `AA B1 01 <8B key>` — the user SHORT-
//      PRESSES the plug button while it is in pairing mode (AA B1 00 = in
//      pairing mode but not yet confirmed; no reply = plug in normal mode).
//   3. 33 B2 <key> must answer `33 B2 00`. Key is plug-owned and persistent.
// Output tokens: `00` = in pairing mode, awaiting button press; `-` = plug not
// answering (not in pairing mode — it must first be unbound from Govee cloud
// via the app, or be a fresh plug), or out of range.
async fn pair(plug_mac: &str, timeout_s: u64) -> Result<String, String> {
    let c = adapter().await;
    let per = find_mac(&c, plug_mac, 10).await?;
    per.connect().await.map_err(|e| format!("conn: {e}"))?;
    sleep(Duration::from_millis(500)).await;
    per.discover_services().await.map_err(|e| format!("disc svc: {e}"))?;
    sub_notify(&per).await?;
    let sk = handshake(&per).await?;
    let mut s: NotifStream = per.notifications().await.map_err(|e| format!("notif: {e}"))?;
    eprintln!("connected. The plug must be in pairing mode (LED slowly blinking blue).");
    eprintln!("  For a bound plug: the Govee app's 'forget device' is a cloud unbind over WiFi,");
    eprintln!("  so re-pairing needs the app (or a fresh plug). Holding the button won't help.");
    eprintln!("  If it is flashing blue, SHORT-PRESS the button now <<< (waiting {timeout_s}s)");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_s);
    let mut key = None;
    while key.is_none() && tokio::time::Instant::now() < deadline {
        write_ctrl(&per, &encrypt(&frame_from(0xAA, 0xB1, &[]), &sk)).await?;
        match wait_frame(&mut s, &sk, 0xAA, 0xB1, 1).await {
            Some(f) if f[2] == 0x01 => { let mut k = [0u8; 8]; k.copy_from_slice(&f[3..11]); key = Some(k); }
            Some(f) => eprint!("{:02x} ", f[2]),
            None => eprint!("- "),
        }
        sleep(Duration::from_millis(250)).await;
    }
    let r = match key {
        None => Err("timed out: plug never confirmed (button not pressed?)".into()),
        Some(k) => {
            write_ctrl(&per, &encrypt(&frame_from(0x33, 0xB2, &k), &sk)).await?;
            match wait_frame(&mut s, &sk, 0x33, 0xB2, 2).await {
                Some(f) if f[2] == 0x00 => Ok(hex::encode(k)),
                Some(f) => Err(format!("33 B2 rejected key (status {:02x})", f[2])),
                None => Err("no 33 B2 response".into()),
            }
        }
    };
    per.disconnect().await.ok();
    drop(c);
    r
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

// ========================= LOCAL STATUS HTTP SERVER =========================
// ponytail: replaces the external healthcheck with a local status page on the
// Pi (http://192.168.2.21:<port>/). Zero internet, zero TLS, one text/plain
// snapshot per connection. The daemon loop pushes the latest snapshot into a
// tokio watch channel; the server task borrows it on each request.
async fn status_server(port: u16, status_rx: tokio::sync::watch::Receiver<String>) {
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    log::info!("status: http://0.0.0.0:{port}/");
    loop {
        match listener.accept().await {
            Ok((mut s, _)) => {
                let body = status_rx.borrow().clone();
                let resp = format!("HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body);
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let _ = s.write_all(resp.as_bytes()).await;
                drop(s);
            }
            Err(e) => {
                log::error!("status accept: {e}");
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

// ========================= DAEMON =========================
async fn daemon_loop(interval_s: u64, threshold: u8, status_port: u16,
                     plug_mac: &str, sensor_mac: &str, plug_skey: Option<[u8; 8]>) {
    log::info!("daemon: interval={interval_s}s threshold={threshold}%");
    let (status_tx, status_rx) = tokio::sync::watch::channel::<String>("starting...".into());
    if status_port != 0 {
        drop(tokio::spawn(status_server(status_port, status_rx)));
    }
    let mut last_on: Option<bool> = None;
    loop {
        match read_sensor(sensor_mac, 10).await {
            Ok((t, h, b)) => {
                log::info!("sensor: {t:.1}C {h}% batt={b}%");
                let need_on = h > threshold;
                if last_on.map(|o| o != need_on).unwrap_or(true) {
                    log::info!("need {}", if need_on { "ON" } else { "OFF" });
                    if need_on { let _ = plug_on(plug_mac, plug_skey.as_ref()).await; }
                    else { let _ = plug_off(plug_mac, plug_skey.as_ref()).await; }
                    last_on = Some(need_on);
                }
                let plug = if need_on { "ON" } else { "OFF" };
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as u32).unwrap_or(0);
                let _ = status_tx.send(format!(
                    "temp={t:.1}C\nhumidity={h}%\nbattery={b}%\nplug={plug}\nthreshold={threshold}\ninterval={interval_s}\nts={ts}\n"));
            }
            Err(e) => {
                log::error!("sensor: {e}");
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as u32).unwrap_or(0);
                let _ = status_tx.send(format!("error={e}\nts={ts}\n"));
            }
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

// Resolve a plug by --name (PLUG_NAMES table) or --mac/--skey.
// Returns (mac, Option<key>). --name wins if present.
fn resolve_plug(args: &[String]) -> (String, Option<[u8; 8]>) {
    if let Some(n) = get_arg(args, "--name") {
        if let Some((mac, skey)) = lookup_plug(&n) {
            let k = skey.and_then(|h| hex::decode(h).ok()).and_then(|b| {
                if b.len() != 8 { None } else { let mut k=[0u8;8]; k.copy_from_slice(&b); Some(k) }
            });
            return (mac.to_string(), k);
        }
        eprintln!("unknown name: {n}");
        print_names();
        std::process::exit(1);
    }
    (get_mac(args, "--mac", PLUG_MAC), parse_skey(args, "--skey"))
}

fn print_names() {
    eprintln!("known plugs (name -> MAC [key]):");
    for &(n, mac, skey) in PLUG_NAMES {
        if let Some(m) = mac {
            eprintln!("  {n:12} -> {m}  {}", skey.unwrap_or("(no key)"));
        }
    }
}

fn parse_skey(args: &[String], name: &str) -> Option<[u8; 8]> {
    get_arg(args, name).and_then(|s| {
        let b = hex::decode(s).ok()?;
        if b.len() != 8 { return None; }
        let mut k = [0u8; 8];
        k.copy_from_slice(&b);
        Some(k)
    })
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: govee-ble <read|on|off|status|scan|pair|daemon|names>");
        eprintln!("  read:   [--mac <addr>]");
        eprintln!("  on/off/status: [--name <name>] | [--mac <addr>] [--skey <hex8>]");
        eprintln!("  scan:   (no args, lists all nearby BLE devices 10s)");
        eprintln!("  pair:   --name <name> | --mac <addr> [--timeout SEC]  (prints secret key; plug must ALREADY be in");
        eprintln!("          pairing mode — fresh plug or one unbound in the Govee app — then short-press it)");
        eprintln!("  names:  (no args, prints the name->MAC table)");
        eprintln!("  daemon: [--interval SEC] [--threshold PCT] [--status-port PORT]");
        eprintln!("          [--plug-mac <addr>] [--sensor-mac <addr>] [--plug-skey <hex8>]");
        eprintln!("  Default plug MAC: {PLUG_MAC}");
        eprintln!("  Default sensor MAC: {SENSOR_MAC}");
        eprintln!("  Secret key (8 hex bytes): --skey f6e0730a5be545e3");
        eprintln!("  Named plugs (see PLUG_NAMES in source):");
        print_names();
        return;
    }
    match args[1].as_str() {
        "read" => match read_sensor(&get_mac(&args, "--mac", SENSOR_MAC), 10).await {
            Ok((t,h,b)) => println!("{t:.1}C {h}% {b}%"),
            Err(e) => { eprintln!("{e}"); std::process::exit(1); }
        },
        "on" => match resolve_plug(&args) {
            (mac, k) => match plug_on(&mac, k.as_ref()).await {
                Ok(_) => println!("ON"),
                Err(e) => { eprintln!("{e}"); std::process::exit(1); }
            }
        },
        "off" => match resolve_plug(&args) {
            (mac, k) => match plug_off(&mac, k.as_ref()).await {
                Ok(_) => println!("OFF"),
                Err(e) => { eprintln!("{e}"); std::process::exit(1); }
            }
        },
        "status" => match resolve_plug(&args) {
            (mac, k) => match plug_status(&mac, k.as_ref()).await {
                Ok(s) => println!("{}", if s { "ON" } else { "OFF" }),
                Err(e) => { eprintln!("{e}"); std::process::exit(1); }
            }
        },
        "scan" => {
            let c = adapter().await;
            c.start_scan(ScanFilter::default()).await.unwrap();
            println!("scanning for 10 seconds...");
            let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while tokio::time::Instant::now() < deadline {
                for p in c.peripherals().await.unwrap() {
                    let addr = p.address().to_string();
                    if seen.insert(addr.clone()) {
                        if let Ok(Some(pr)) = p.properties().await {
                            let name = pr.local_name.as_deref().unwrap_or("");
                            let mfgs: Vec<String> = pr.manufacturer_data.keys().map(|k| format!("0x{k:04X}")).collect();
                            let mfg = if mfgs.is_empty() { "".into() } else { mfgs.join(",") };
                            println!("{addr}  {:+3}dBm  {name}  mfg=[{mfg}]",
                                pr.rssi.unwrap_or(0));
                        }
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }
            c.stop_scan().await.ok();
        },
        "names" => {
            print_names();
        },
        "pair" | "get-skey" => {
            let (mac, _) = resolve_plug(&args);
            if mac.is_empty() { eprintln!("--name <name> or --mac <addr> required"); std::process::exit(1); }
            let t = get_arg(&args, "--timeout").and_then(|v| v.parse().ok()).unwrap_or(60);
            match pair(&mac, t).await {
                Ok(key) => { eprintln!("paired. use: --skey {key}"); println!("{key}"); }
                Err(e) => { eprintln!("{e}"); std::process::exit(1); }
            }
        },
        "daemon" => {
            env_logger::init();
            daemon_loop(
                get_arg(&args, "--interval").and_then(|v| v.parse().ok()).unwrap_or(900),
                get_arg(&args, "--threshold").and_then(|v| v.parse().ok()).unwrap_or(45),
                get_arg(&args, "--status-port").and_then(|v| v.parse().ok()).unwrap_or(0u16),
                &get_mac(&args, "--plug-mac", PLUG_MAC),
                &get_mac(&args, "--sensor-mac", SENSOR_MAC),
                parse_skey(&args, "--plug-skey"),
            ).await;
        }
        _ => { eprintln!("unknown: {}", args[1]); std::process::exit(1); }
    }
}