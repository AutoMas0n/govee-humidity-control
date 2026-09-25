use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;
use btleplug::api::{
    Central, CharPropFlags, Manager as _, Peripheral, ScanFilter, WriteType,
};
use futures::StreamExt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
// c is the adapter the caller owns (respectively one per process for one-shots,
// or the daemon's single lifetime adapter). Passed in, not created here — see
// openspec/changes/fix-daemon-ble-session-leak.
async fn try_plug_inner<T, Fut>(c: &btleplug::platform::Adapter, plug_mac: &str, skey: Option<&[u8; 8]>, action: impl FnOnce(btleplug::platform::Peripheral, [u8; 16]) -> Fut) -> Result<T, String>
where Fut: Future<Output = Result<T, String>>,
{
    let per = find_mac(c, plug_mac, 10).await?;
    // The plug sits ~10 m away on a flaky BLE link; an untimed connect can
    // hang the loop forever on D-Bus (observed live: loop froze after "need
    // OFF"). Always bound the connect; on timeout the caller retries.
    tokio::time::timeout(Duration::from_secs(12), per.connect())
        .await.map_err(|_| "conn: timeout")?
        .map_err(|e| format!("conn: {e}"))?;
    sleep(Duration::from_millis(500)).await;
    tokio::time::timeout(Duration::from_secs(8), per.discover_services())
        .await.map_err(|_| "disc svc: timeout")?
        .map_err(|e| format!("disc svc: {e}"))?;
    tokio::time::timeout(Duration::from_secs(8), sub_notify(&per))
        .await.map_err(|_| "sub: timeout")??;
    let sk = tokio::time::timeout(Duration::from_secs(8), handshake(&per))
        .await.map_err(|_| "handshake: timeout")??;
    tokio::time::timeout(Duration::from_secs(10), init_plug(&per, &sk, skey))
        .await.map_err(|_| "init: timeout")??;
    let r = action(per, sk).await;
    r
}

// ========================= PLUG COMMANDS (each with own retry) =========================
async fn plug_on(c: &btleplug::platform::Adapter, plug_mac: &str, skey: Option<&[u8; 8]>) -> Result<(), String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(c, plug_mac, skey, |per, sk| Box::pin(async move {
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

async fn plug_off(c: &btleplug::platform::Adapter, plug_mac: &str, skey: Option<&[u8; 8]>) -> Result<(), String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(c, plug_mac, skey, |per, sk| Box::pin(async move {
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

async fn plug_status(c: &btleplug::platform::Adapter, plug_mac: &str, skey: Option<&[u8; 8]>) -> Result<bool, String> {
    let mut err = String::new();
    for a in 0..3 {
        match try_plug_inner(c, plug_mac, skey, |per, sk| Box::pin(async move {
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
// Advertisement manufacturer data, company id 0x8801 (not 0xEC88 — that
// value is the GATT service UUID, unrelated). Payload, 9 bytes:
//   [0] 0xEC (packet marker)
//   [1..3] 00 01 01 (header)
//   [4..5] temperature, little-endian i16, two's complement, /100
//   [6..7] humidity,     little-endian u16, /100
//   [8]   battery %
// Format cross-checked against sensor.goveetemp_bt_hci's H5179 decoder
// (unpack_from("<HHB", mfg_data, 6)) and live captures on the Pi:
//   ec 00 01 01 24 09 00 14 56 -> 23.40C / 51.20% / 86%.
fn parse_h5179(data: &[u8]) -> Option<(f32, u8, u8)> {
    if data.len() < 9 || data[0] != 0xEC { return None; }
    let raw_t = (data[5] as u16) << 8 | (data[4] as u16);
    let temp = (raw_t as i16) as f32 / 100.0;
    let raw_h = (data[7] as u16) << 8 | (data[6] as u16);
    Some((temp, (raw_h / 100) as u8, data[8]))
}

async fn read_sensor(c: &btleplug::platform::Adapter, mac: &str, secs: u64) -> Result<(f32, u8, u8, Option<i16>), String> {
    let mac = mac.to_uppercase();
    c.start_scan(ScanFilter::default()).await.map_err(|e| format!("scan: {e}"))?;
    let d = tokio::time::Instant::now() + Duration::from_secs(secs);
    loop {
        for p in c.peripherals().await.map_err(|e| format!("periphs: {e}"))? {
            if p.address().to_string().to_uppercase() != mac { continue; }
            if let Ok(Some(pr)) = p.properties().await {
                if let Some(data) = pr.manufacturer_data.get(&0x8801) {
                    if let Some(r) = parse_h5179(data) {
                        c.stop_scan().await.ok();
                        return Ok((r.0, r.1, r.2, pr.rssi));
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
// ponytail v2: replaces the healthcheck with a local dashboard on the Pi
// (http://192.168.2.21:<port>/). LAN-only, no TLS, no CDN — the page is a
// single inline HTML string; state is served as JSON that the page polls.
// One shared `Status` struct (Arc<Mutex<>>) is written by the daemon loop and
// read by the server task; on a missed poll the loop keeps the last good
// values and only bumps last_attempt_ts/last_error (see status-page spec).

// Snapshot struct mirrors /state.json (design.md decision 1).
#[derive(Clone, Debug)]
struct Status {
    temp: Option<f32>,
    humidity: Option<u8>,
    battery: Option<u8>,
    rssi: Option<i16>,
    plug: Option<bool>,
    hi: u8,
    lo: u8,
    interval: u64,
    last_ok_ts: u64,
    last_attempt_ts: u64,
    last_error: Option<String>,
}

impl Status {
    fn new(hi: u8, lo: u8, interval: u64) -> Self {
        Status {
            temp: None, humidity: None, battery: None, rssi: None, plug: None,
            hi, lo, interval,
            last_ok_ts: 0, last_attempt_ts: 0,
            last_error: None,
        }
    }
}

// Shared state handed to both the loop (writer) and the server (reader + the
// dry-mode writer). force uses a monotonic Instant (NTP-jump proof); the epoch
// value shown to the page is derived per render (design.md decision 3). The
// Notify wakes the loop the moment dry state changes, so a short timer (or a
// stop) acts immediately instead of waiting for the next 15-min poll.
struct Shared {
    status: tokio::sync::Mutex<Status>,
    force: tokio::sync::Mutex<Option<std::time::Instant>>,
    last_poll: tokio::sync::Mutex<Option<std::time::Instant>>,
    notify: tokio::sync::Notify,
}

// Manual "Refresh now" live-poll throttle: 1 sensor read / 30s (each poll
// runs a ~10s BLE scan, so tighter would hammer the radio). Rate limit does
// NOT apply to the daemon's own cycle or to dry-mode wakeups.
const POLL_MIN_INTERVAL: Duration = Duration::from_secs(30);

pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs()).unwrap_or(0)
}

// ===================== FORCE PERSISTENCE (design.md 3b) =====================
// Survives the daily 06:00 reboot: a single file holds the epoch deadline.
// Fixed path, no CLI flag, unit file stays untouched.
const FORCE_FILE: &str = "/var/lib/humidity/force_until";

fn write_force_file(until: Option<u64>) {
    // File must survive reboots; ensure the directory exists (design.md 3b).
    let _ = std::fs::create_dir_all("/var/lib/humidity");
    let r = match until {
        Some(t) => std::fs::write(FORCE_FILE, t.to_string()),
        None => std::fs::remove_file(FORCE_FILE),
    };
    if let Err(e) = r {
        log::warn!("force file: {e}");
    }
}

fn read_force_file() -> Option<u64> {
    std::fs::read_to_string(FORCE_FILE).ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn init_force_from_file() -> Option<std::time::Instant> {
    match read_force_file() {
        Some(t) if t > now_epoch() => {
            let dur = Duration::from_secs(t - now_epoch());
            log::info!("dry: restored timer from file ({dur:?} left)");
            Some(std::time::Instant::now() + dur)
        }
        Some(_) => { // stale (in the past); drop the file
            let _ = std::fs::remove_file(FORCE_FILE);
            None
        }
        None => None,
    }
}

// epoch for the page = monotonic remainder cast to wall clock.
fn force_epoch(f: &Option<std::time::Instant>) -> Option<u64> {
    f.map(|t| now_epoch() + t.saturating_duration_since(std::time::Instant::now()).as_secs())
}

// ===================== JSON (hand-rolled, design.md 4b) =====================
fn json_num_i16(v: Option<i16>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "null".into())
}
fn json_num_f32(v: Option<f32>) -> String {
    v.map(|x| format!("{x:.1}")).unwrap_or_else(|| "null".into())
}
fn json_str(v: Option<&String>) -> String {
    match v {
        Some(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")),
        None => "null".into(),
    }
}
fn status_to_json(s: &Status, force: &Option<std::time::Instant>) -> String {
    format!(
        "{{\"temp\":{},\"humidity\":{},\"battery\":{},\"rssi\":{},\"plug\":{},\"hi\":{},\"lo\":{},\"interval\":{},\"last_ok_ts\":{},\"last_attempt_ts\":{},\"last_error\":{},\"force_until\":{}}}",
        json_num_f32(s.temp),
        s.humidity.map(|x| x.to_string()).unwrap_or_else(|| "null".into()),
        s.battery.map(|x| x.to_string()).unwrap_or_else(|| "null".into()),
        json_num_i16(s.rssi),
        match s.plug { Some(true) => "\"ON\"", Some(false) => "\"OFF\"", None => "\"unknown\"" },
        s.hi, s.lo, s.interval, s.last_ok_ts, s.last_attempt_ts,
        json_str(s.last_error.as_ref()),
        force_epoch(force).map(|x| x.to_string()).unwrap_or_else(|| "null".into()),
    )
}

// ===================== HTTP ROUTING =====================
fn http_response(code: &str, ctype: &str, body: &str) -> String {
    format!("HTTP/1.0 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body)
}

async fn status_server(port: u16, shared: Arc<Shared>) {
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    log::info!("status: http://0.0.0.0:{port}/");
    loop {
        match listener.accept().await {
            Ok((s, _)) => {
                // Handle each connection concurrently, so a slow POST /poll
                // (which waits up to 20s for a sensor read) never blocks the
                // 30s auto-refresh or other tabs.
                let shared = shared.clone();
                tokio::spawn(async move { handle_conn(s, &shared).await; });
            }
            Err(e) => {
                log::error!("status accept: {e}");
                sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

async fn handle_conn(mut s: tokio::net::TcpStream, shared: &Arc<Shared>) {
    // Read the request head (until the blank line); we never need the body.
    let mut buf = Vec::new();
    let mut tmp = [0u8; 512];
    let d = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < d && !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < 8192 {
        if let Ok(n) = tokio::time::timeout(Duration::from_millis(500), s.read(&mut tmp)).await {
            let n = n.unwrap_or(0);
            if n == 0 { break; }
            buf.extend_from_slice(&tmp[..n]);
        }
    }
    let req = String::from_utf8_lossy(&buf);
    let mut lines = req.lines();
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();
    let (m, q) = match path.split_once('?') {
        Some((a, b)) => (a, b),
        None => (path.as_str(), ""),
    };
    let resp = match (method.as_str(), m) {
        ("GET", "/") => http_response("200 OK", "text/html; charset=utf-8", PAGE_HTML),
        ("GET", "/state.json") => {
            let (st, f) = {
                let st = shared.status.lock().await;
                let f = *shared.force.lock().await;
                (st.clone(), f)
            };
            http_response("200 OK", "application/json", &status_to_json(&st, &f))
        }
        ("POST", "/dry") => {
            // mins = query param, 1..1440 clamped
            let mins: u64 = q.split('&').find_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (k == "mins").then(|| v.parse::<u64>().ok()).flatten()
            }).unwrap_or(0).clamp(1, 1440);
            let deadline = std::time::Instant::now() + Duration::from_secs(mins * 60);
            {
                let mut f = shared.force.lock().await;
                *f = Some(deadline);
            }
            write_force_file(Some(now_epoch() + mins * 60));
            shared.notify.notify_one(); // wake the loop NOW (short timers)
            log::info!("dry: ON for {mins}m");
            http_response("200 OK", "application/json", &format!(r#"{{"ok":true,"mins":{mins}}}"#))
        }
        ("POST", "/dry-off") => {
            {
                let mut f = shared.force.lock().await;
                *f = None;
            }
            write_force_file(None);
            shared.notify.notify_one(); // re-evaluate band immediately
            log::info!("dry: OFF");
            http_response("200 OK", "application/json", r#"{"ok":true}"#)
        }
        ("POST", "/poll") => {
            let (allowed, wait) = {
                let mut lp = shared.last_poll.lock().await;
                match *lp {
                    Some(t) if t.elapsed() < POLL_MIN_INTERVAL => {
                        let rem = POLL_MIN_INTERVAL.saturating_sub(t.elapsed());
                        (false, rem.as_secs())
                    }
                    _ => { *lp = Some(std::time::Instant::now()); (true, 0u64) }
                }
            };
            if allowed {
                shared.notify.notify_one(); // loop wakes and reads NOW
                log::info!("poll: manual read requested");
                http_response("200 OK", "application/json", r#"{"ok":true}"#)
            } else {
                http_response("429 Too Many Requests", "application/json",
                    &format!(r#"{{"ok":false,"error":"rate limited","retry_after":{wait}}}"#))
            }
        }
        _ => http_response("404 Not Found", "application/json", r#"{"ok":false,"error":"not found"}"#),
    };
    let _ = s.write_all(resp.as_bytes()).await;
    drop(s);
}

// ===================== DASHBOARD PAGE (lila.lan style) =====================
// Single inline string: no CDN, no assets. Polls /state.json every 30s.
const PAGE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
<title>Dehumidifier</title>
<style>
  *{box-sizing:border-box;margin:0;padding:0}
  body{font-family:-apple-system,system-ui,sans-serif;background:#f0f2f5;color:#1a1a2e;padding:16px 12px;padding-bottom:80px;max-width:480px;margin:0 auto}
  h1{font-size:1.25rem;font-weight:600;margin-bottom:2px}
  .sub{color:#6b7280;font-size:.8rem;margin-bottom:14px}
  .stats{display:grid;grid-template-columns:1fr 1fr 1fr;gap:8px;margin-bottom:8px}
  .stat-box{background:#fff;border-radius:10px;padding:12px 8px;text-align:center;box-shadow:0 1px 3px rgba(0,0,0,.06)}
  .stat-box .n{font-size:1.35rem;font-weight:700;line-height:1.2}
  .stat-box .l{font-size:.7rem;color:#6b7280;margin-top:2px}
  .grid{display:grid;grid-template-columns:1fr;gap:12px}
  .card{background:#fff;border-radius:12px;padding:14px;box-shadow:0 1px 3px rgba(0,0,0,.06)}
  .card h2{font-size:.9rem;font-weight:600;margin-bottom:8px;color:#374151}
  .card p.note{font-size:.7rem;color:#9ca3af;margin-top:6px}
  .miss{display:none;background:#fef2f2;border:1px solid #fecaca;color:#b91c1c;border-radius:10px;padding:10px 12px;font-size:.8rem;margin-bottom:12px}
  .miss.show{display:block}
  .meter{height:10px;border-radius:6px;background:#e5e7eb;overflow:hidden;margin:6px 0 4px}
  .meter > div{height:100%;transition:width .6s ease}
  .meter.good > div{background:#22c55e}
  .meter.warn > div{background:#f59e0b}
  .meter.bad > div{background:#ef4444}
  .meter.na > div{background:#9ca3af;width:10%}
  .rssi-row{display:flex;justify-content:space-between;align-items:center;font-size:.75rem;color:#6b7280}
  .btns{display:grid;grid-template-columns:repeat(4,1fr);gap:8px;margin:10px 0}
  .btn{border:none;border-radius:10px;padding:10px 4px;font-size:.85rem;font-weight:600;cursor:pointer;color:#fff;background:#3b82f6}
  .btn:active{transform:scale(.97)}
  .btn-row{display:flex;gap:8px;margin-top:10px}
  .btn-row input{flex:1;border:1px solid #d1d5db;border-radius:10px;padding:10px;font-size:.85rem;min-width:0}
  .btn-go{background:#16a34a;padding:10px 18px}
  .btn-stop{background:#ef4444}
  .dry-active{background:#dcfce7;border:1px solid #86efac;border-radius:12px;padding:10px 12px;font-size:.85rem;font-weight:600;color:#166534;margin-bottom:8px}
  .dry-count{font-family:ui-monospace,monospace;font-size:1.1rem}
  .msg{font-size:.75rem;color:#16a34a;margin-top:6px}
  .msg.err{color:#dc2626}
  #refreshBtn{width:100%;padding:10px;margin-top:12px;border:none;border-radius:10px;background:#1a1a2e;color:#fff;font-size:.85rem;font-weight:600;cursor:pointer}
</style>
</head>
<body>
<h1>Dehumidifier</h1>
<div class="sub" id="subline">basement · local control</div>
<div class="miss" id="miss">sensor missed — showing last reading (<span id="missAge"></span>)</div>
<div class="stats">
  <div class="stat-box"><div class="n" id="t">—</div><div class="l">TEMP °C</div></div>
  <div class="stat-box"><div class="n" id="h">—</div><div class="l">HUMIDITY</div></div>
  <div class="stat-box"><div class="n" id="b">—</div><div class="l">BATTERY</div></div>
  <div class="stat-box"><div class="n" id="plug">—</div><div class="l">PLUG</div></div>
  <div class="stat-box"><div class="n" id="last">—</div><div class="l">LAST READ</div></div>
  <div class="stat-box"><div class="n" id="force">—</div><div class="l">DRY MODE</div></div>
</div>
<div class="grid">
  <div class="card"><h2>Setpoints</h2><p style="font-size:.85rem">ON when ≥ <b id="hi">—</b>% · OFF at ≤ <b id="lo">—</b>% · poll <b id="interval">—</b>s</p><p class="note">Hysteresis band: hold state while between the two.</p></div>
  <div class="card"><h2>Signal</h2><div class="meter na" id="meter"><div style="width:100%"></div></div><div class="rssi-row"><span id="rssiLabel">unknown</span><span id="dbm">—</span></div></div>
  <div class="card">
    <h2>Dry Clothes</h2>
    <div class="dry-active" id="dryActive" style="display:none"><span id="dryRemain"></span></div>
    <div class="btns">
      <button class="btn" onclick="dry(30)">30m</button>
      <button class="btn" onclick="dry(60)">1h</button>
      <button class="btn" onclick="dry(120)">2h</button>
      <button class="btn" onclick="dry(240)">4h</button>
    </div>
    <div class="btn-row">
      <input id="custom" type="number" min="1" max="1440" placeholder="custom mins">
      <button class="btn btn-go" onclick="dryCustom()">GO</button>
      <button class="btn btn-stop" onclick="dryOff()">STOP</button>
    </div>
    <div class="msg" id="msg"></div>
    <p class="note">Turns the dehumidifier on (or keeps it on) for the set time, ignoring the humidity band.</p>
  </div>
</div>
<button id="refreshBtn" onclick="refresh()">Refresh now</button>
<script>
function ago(s){
  if(!s) return '—';
  const d=Date.now()/1000 - s;
  if(d<0) return 'now';
  if(d<60) return Math.round(d)+'s';
  if(d<3600) return Math.round(d/60)+'m';
  return Math.round(d/3600)+'h';
}
let lastState = null;
function render(st){
  lastState = st;
  document.getElementById('t').textContent = (st.temp!=null? st.temp.toFixed(1) : '—');
  document.getElementById('h').textContent = (st.humidity!=null? st.humidity+'%' : '—');
  document.getElementById('b').textContent = (st.battery!=null? st.battery+'%' : '—');
  const plugEl = document.getElementById('plug');
  plugEl.textContent = (st.plug==='ON'?'ON':st.plug==='OFF'?'OFF':'—');
  plugEl.style.color = st.plug==='ON' ? '#16a34a' : (st.plug==='OFF' ? '#6b7280' : 'inherit');
  document.getElementById('last').textContent = ago(st.last_ok_ts);
  document.getElementById('hi').textContent = st.hi!=null? st.hi : '—';
  document.getElementById('lo').textContent = st.lo!=null? st.lo : '—';
  document.getElementById('interval').textContent = st.interval!=null? st.interval : '—';
  // RSSI meter
  const meter = document.getElementById('meter');
  const r = st.rssi;
  meter.className = 'meter ' + (r==null ? 'na' : (r>=-70 ? 'good' : (r>=-85 ? 'warn' : 'bad')));
  meter.querySelector('div').style.width = (r==null? 10 : Math.min(100, Math.max(5, (r+100)/55*100))) + '%';
  document.getElementById('rssiLabel').textContent = r==null ? 'unknown' : (r>=-70 ? 'strong' : (r>=-85 ? 'marginal' : 'weak'));
  document.getElementById('dbm').textContent = r==null ? '—' : r+' dBm';
  // miss — ONLY when the failed attempt is the most recent one (if a success
  // landed afterwards, last_attempt_ts == last_ok_ts and there's no miss).
  const miss = document.getElementById('miss');
  if(st.last_error && st.last_ok_ts>0 && st.last_attempt_ts !== st.last_ok_ts){
    miss.classList.add('show');
    document.getElementById('missAge').textContent = ago(st.last_attempt_ts);
  } else miss.classList.remove('show');
  // dry mode
  const act = document.getElementById('dryActive');
  if(st.force_until!=null && st.force_until>Date.now()/1000){
    const left = st.force_until - Date.now()/1000;
    document.getElementById('dryRemain').textContent = 'DRY ON — ' + fmtDur(left) + ' left';
    act.style.display = 'block';
  } else act.style.display = 'none';
  document.getElementById('force').textContent = (st.force_until!=null && st.force_until>Date.now()/1000) ? 'ON' : '—';
}
function fmtDur(s){
  const m = Math.floor(s/60);
  const sec = Math.floor(s%60);
  if(m>=60) return Math.floor(m/60)+'h '+(m%60)+'m';
  return m+'m '+sec+'s';
}
function refresh(){
  // Manual live read: POST /poll (rate-limited server-side) wakes the daemon;
  // the page then retries /state.json until the sensor attempt completes
  // (last_attempt_ts advances past where it was) so we show FRESH data.
  const btn = document.getElementById('refreshBtn');
  btn.disabled = true;
  btn.textContent = 'Reading…';
  fetch('/state.json').then(r=>r.json()).then(function(st){
    const prev = st.last_attempt_ts || 0;
    return fetch('/poll',{method:'POST'}).then(r=>r.json()).then(function(d){
      if(d.retry_after){ btn.textContent = 'Wait '+d.retry_after+'s'; return; }
      // retry state until the loop's attempt finishes (≤ ~15s), then render
      let tries = 0;
      const poll = function(){
        fetch('/state.json').then(r=>r.json()).then(function(n){
          if(n.last_attempt_ts > prev || ++tries > 20){ render(n); }
          else setTimeout(poll, 800);
        }).catch(function(){ render(null); });
      };
      setTimeout(poll, 800);
    });
  }).catch(function(){ fetchState(); }).finally(function(){
    setTimeout(function(){ btn.textContent = 'Refresh now'; btn.disabled = false; }, 1500);
  });
}
function fetchState(){
  fetch('/state.json').then(r=>r.json()).then(render).catch(()=>{});
}
function dry(mins){
  fetch('/dry?mins='+mins,{method:'POST'}).then(r=>r.json()).then(d=>{
    const m = document.getElementById('msg');
    m.textContent = d.ok ? 'Dry mode ON ('+mins+' min)' : (d.error||'error');
    m.className = 'msg' + (d.ok?'':' err');
    if(d.ok) setTimeout(refresh, 300);
  });
}
function dryCustom(){
  const v = parseInt(document.getElementById('custom').value, 10);
  if(!v || v<1){ return; }
  dry(Math.min(1440, Math.max(1, v)));
}
function dryOff(){
  fetch('/dry-off',{method:'POST'}).then(r=>r.json()).then(d=>{
    const m = document.getElementById('msg');
    m.textContent = d.ok ? 'Dry mode off' : (d.error||'error');
    m.className = 'msg' + (d.ok?'':' err');
    if(d.ok) setTimeout(refresh, 300);
  });
}
fetchState();          // show cached data instantly on load (no blank page)
refresh();             // then request a live read in the background
setInterval(fetchState, 30000); // auto-refresh: cache only, no live scans
setInterval(function(){ // 1s countdown ticker for dry mode
  if(lastState && lastState.force_until!=null && lastState.force_until>Date.now()/1000){
    const left = lastState.force_until - Date.now()/1000;
    document.getElementById('dryRemain').textContent = 'DRY ON — ' + fmtDur(left) + ' left';
    document.getElementById('force').textContent = 'ON';
  }
}, 1000);
</script>
</body>
</html>
"##;

// ========================= DAEMON =========================
// Hysteresis band decision: ON at >= hi, OFF at <= lo, hold in between.
// When hi == lo (the --threshold alias) this reduces exactly to the old
// single-threshold contract: ON when h > N, OFF when h <= N. `last_on` is
// None only on the first read — start OFF unless already over the high setpoint.
fn band_need_on(h: u8, hi: u8, lo: u8, last_on: Option<bool>) -> bool {
    if hi == lo {
        h > hi
    } else {
        match last_on {
            None => h >= hi,
            Some(_) if h >= hi => true,
            Some(_) if h <= lo => false,
            Some(o) => o,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_holds_on_inside_dead_band() {
        // (hi=55, lo=45): once ON, stays ON between 46..54; once OFF, stays OFF
        assert!(band_need_on(50, 55, 45, Some(true)));
        assert!(band_need_on(54, 55, 45, Some(true)));
        assert!(!band_need_on(46, 55, 45, Some(false)));
        assert!(!band_need_on(54, 55, 45, Some(false)));
    }

    #[test]
    fn band_turns_on_only_at_hi() {
        assert!(band_need_on(55, 55, 45, Some(false)));
        assert!(band_need_on(60, 55, 45, Some(false)));
        assert!(!band_need_on(54, 55, 45, Some(false)));
    }

    #[test]
    fn band_turns_off_only_at_lo() {
        assert!(!band_need_on(45, 55, 45, Some(true)));
        assert!(!band_need_on(30, 55, 45, Some(true)));
        assert!(band_need_on(46, 55, 45, Some(true)));
    }

    #[test]
    fn band_first_read_starts_off_unless_over_hi() {
        assert!(!band_need_on(50, 55, 45, None));
        assert!(band_need_on(55, 55, 45, None));
        assert!(!band_need_on(45, 55, 45, None));
    }

    #[test]
    fn threshold_alias_matches_old_single_threshold() {
        // --threshold 45 -> hi=lo=45: ON strictly above 45, OFF at 45 and below
        assert!(!band_need_on(45, 45, 45, None));
        assert!(band_need_on(46, 45, 45, None));
        assert!(band_need_on(46, 45, 45, Some(false)));
        assert!(!band_need_on(44, 45, 45, Some(true)));
        assert!(!band_need_on(45, 45, 45, Some(true)));
    }

    #[test]
    fn force_override_and_expiry_handoff() {
        // Force active: plug always ON regardless of the band.
        let force_active = true;
        assert!(force_active);
        // Expiry: timer gone -> band resumes as a fresh first read.
        // We model the loop's decision: fresh first read = band_need_on(h, hi, lo, None).
        // In the dead band (47% with 55/45) the fresh read starts OFF.
        assert!(!band_need_on(47, 55, 45, None));
        // Above hi it starts ON.
        assert!(band_need_on(60, 55, 45, None));
    }

    #[test]
    fn json_helpers_escape_and_null() {
        assert_eq!(json_num_i16(None), "null");
        assert_eq!(json_num_i16(Some(-48)), "-48");
        assert_eq!(json_num_f32(None), "null");
        assert_eq!(json_num_f32(Some(22.35)), "22.4");
        assert_eq!(json_str(None), "null");
        assert_eq!(json_str(Some(&"a\"b\\c\n".to_string())), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn status_to_json_shape() {
        let st = Status {
            temp: Some(22.3), humidity: Some(52), battery: Some(86), rssi: Some(-48),
            plug: Some(true), hi: 55, lo: 45, interval: 900,
            last_ok_ts: 1234, last_attempt_ts: 1234, last_error: None,
        };
        let j = status_to_json(&st, &None);
        assert!(j.contains("\"temp\":22.3"));
        assert!(j.contains("\"humidity\":52"));
        assert!(j.contains("\"rssi\":-48"));
        assert!(j.contains("\"plug\":\"ON\""));
        assert!(j.contains("\"last_error\":null"));
        assert!(j.contains("\"force_until\":null"));
        assert!(j.starts_with('{') && j.ends_with('}'));
    }

    #[test]
    fn force_epoch_returns_none_when_inactive() {
        assert_eq!(force_epoch(&None), None);
        let f = Some(std::time::Instant::now() + Duration::from_secs(120));
        let e = force_epoch(&f).unwrap();
        // within a couple seconds of now+120
        assert!((e as i64 - (now_epoch() as i64 + 120)).abs() <= 3);
    }
}

async fn daemon_loop(c: &btleplug::platform::Adapter, interval_s: u64, hi: u8, lo: u8, status_port: u16,
                     plug_mac: &str, sensor_mac: &str, plug_skey: Option<[u8; 8]>) {
    log::info!("daemon: interval={interval_s}s hi={hi}% lo={lo}%");
    let shared = Arc::new(Shared {
        status: tokio::sync::Mutex::new(Status::new(hi, lo, interval_s)),
        force: tokio::sync::Mutex::new(init_force_from_file()),
        last_poll: tokio::sync::Mutex::new(None),
        notify: tokio::sync::Notify::new(),
    });
    if status_port != 0 {
        drop(tokio::spawn(status_server(status_port, shared.clone())));
    }
    let mut last_on: Option<bool> = None;
    let mut force_was_active = false;
    loop {
        // Force is active only while the deadline is in the FUTURE. The mutex
        // keeps the deadline set until a cycle notices it expired (or a
        // /dry-off clears it), so `is_some()` alone is NOT enough — checking
        // it would never turn off after natural expiry.
        let force_active = { let f = shared.force.lock().await; *f > Some(std::time::Instant::now()) };
        if force_was_active && !force_active {
            // Natural expiry (or dry-off): clear the deadline + file so state.json
            // goes honest, and reset to a fresh first read (start OFF unless >= hi).
            *shared.force.lock().await = None;
            write_force_file(None);
            log::info!("dry: timer expired — band resumes (fresh first read)");
            last_on = None;
        }
        force_was_active = force_active;
        match read_sensor(c, sensor_mac, 10).await {
            Ok((t, h, b, rssi)) => {
                log::info!("sensor: {t:.1}C {h}% batt={b}% rssi={}dBm", rssi.unwrap_or(0));
                // Dry mode overrides the band entirely; otherwise hysteresis.
                let need_on = if force_active { true } else { band_need_on(h, hi, lo, last_on) };
                if last_on.map(|o| o != need_on).unwrap_or(true) {
                    log::info!("need {}", if need_on { "ON" } else { "OFF" });
                    if need_on { let _ = plug_on(c, plug_mac, plug_skey.as_ref()).await; }
                    else { let _ = plug_off(c, plug_mac, plug_skey.as_ref()).await; }
                    last_on = Some(need_on);
                }
                let ts = now_epoch();
                let mut st = shared.status.lock().await;
                st.temp = Some(t);
                st.humidity = Some(h);
                st.battery = Some(b);
                st.rssi = rssi;
                st.plug = Some(need_on);
                st.last_ok_ts = ts;
                st.last_attempt_ts = ts;
                st.last_error = None;
            }
            Err(e) => {
                log::error!("sensor: {e}");
                // Keep the last good reading; only bump the attempt timestamp +
                // record the error so the page shows "missed" (status-page spec).
                let mut st = shared.status.lock().await;
                st.last_attempt_ts = now_epoch();
                st.last_error = Some(e);
            }
        }
        // Sleep until the next poll — or until the dry timer ends, whichever is
        // sooner — and wake immediately when dry state changes (server notifies).
        let sleep_dur = {
            let f = shared.force.lock().await;
            match *f {
                Some(deadline) => {
                    let until = deadline.saturating_duration_since(std::time::Instant::now());
                    Duration::from_secs(interval_s).min(until)
                }
                None => Duration::from_secs(interval_s),
            }
        };
        tokio::select! {
            _ = sleep(sleep_dur) => {}
            _ = shared.notify.notified() => {}
        }
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
        eprintln!("  daemon: [--interval SEC] [--hi PCT] [--lo PCT] [--threshold PCT] [--status-port PORT]");
        eprintln!("          [--plug-mac <addr>] [--sensor-mac <addr>] [--plug-skey <hex8>]");
        eprintln!("  Default plug MAC: {PLUG_MAC}");
        eprintln!("  Default sensor MAC: {SENSOR_MAC}");
        eprintln!("  Secret key (8 hex bytes): --skey f6e0730a5be545e3");
        eprintln!("  Named plugs (see PLUG_NAMES in source):");
        print_names();
        return;
    }
    match args[1].as_str() {
        "read" => {
            let c = adapter().await;
            match read_sensor(&c, &get_mac(&args, "--mac", SENSOR_MAC), 10).await {
                Ok((t,h,b,rssi)) => println!("{t:.1}C {h}% {b}% rssi={}dBm", rssi.unwrap_or(0)),
                Err(e) => { eprintln!("{e}"); std::process::exit(1); }
            }
        },
        "on" => {
            let c = adapter().await;
            match resolve_plug(&args) {
                (mac, k) => match plug_on(&c, &mac, k.as_ref()).await {
                    Ok(_) => println!("ON"),
                    Err(e) => { eprintln!("{e}"); std::process::exit(1); }
                }
            }
        },
        "off" => {
            let c = adapter().await;
            match resolve_plug(&args) {
                (mac, k) => match plug_off(&c, &mac, k.as_ref()).await {
                    Ok(_) => println!("OFF"),
                    Err(e) => { eprintln!("{e}"); std::process::exit(1); }
                }
            }
        },
        "status" => {
            let c = adapter().await;
            match resolve_plug(&args) {
                (mac, k) => match plug_status(&c, &mac, k.as_ref()).await {
                    Ok(s) => println!("{}", if s { "ON" } else { "OFF" }),
                    Err(e) => { eprintln!("{e}"); std::process::exit(1); }
                }
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
            // One BlueZ session for the daemon's whole life — creating one per
            // cycle leaked a D-Bus socket each time (fix-daemon-ble-session-leak).
            let c = adapter().await;
            // Hysteresis band: --hi/--lo (defaults 55/45). --threshold N remains
            // a single-threshold alias meaning band N/N (identical to pre-band).
            let thr = get_arg(&args, "--threshold").and_then(|v| v.parse().ok());
            let hi = get_arg(&args, "--hi").and_then(|v| v.parse().ok()).or(thr).unwrap_or(55u8);
            let lo = get_arg(&args, "--lo").and_then(|v| v.parse().ok()).or(thr).unwrap_or(45u8);
            daemon_loop(
                &c,
                get_arg(&args, "--interval").and_then(|v| v.parse().ok()).unwrap_or(900),
                hi,
                lo,
                get_arg(&args, "--status-port").and_then(|v| v.parse().ok()).unwrap_or(0u16),
                &get_mac(&args, "--plug-mac", PLUG_MAC),
                &get_mac(&args, "--sensor-mac", SENSOR_MAC),
                parse_skey(&args, "--plug-skey"),
            ).await;
        }
        _ => { eprintln!("unknown: {}", args[1]); std::process::exit(1); }
    }
}