//! Runtime status file shared between the running tether and `usb-android-tethering status`.

use crate::netservice::Info;
use crate::tether::Counters;
use crate::usbnet::Found;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PATH: &str = "/var/run/usb-android-tethering.json";

#[derive(Clone, Copy)]
pub enum Phase {
    Waiting,
    Connecting,
    Online,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Last write, kept in memory: phase start time, and the exact bytes written so an unchanged
/// status (e.g. an idle link) costs no disk I/O at all.
static LAST: Mutex<(&str, u64, String)> = Mutex::new(("", 0, String::new()));

pub fn write(
    phase: Phase,
    dev: Option<&Found>,
    iface: Option<&str>,
    net: Option<&Info>,
    c: Option<&Counters>,
) {
    let phase = match phase {
        Phase::Waiting => "waiting",
        Phase::Connecting => "connecting",
        Phase::Online => "online",
    };
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.0 != phase {
        *last = (phase, now(), String::new());
    }
    let body = json!({
        "pid": std::process::id(),
        "phase": phase,
        "since": last.1,
        "device": dev.map(|d| json!({
            "vid": format!("{:04x}", d.vid),
            "pid": format!("{:04x}", d.pid),
            "protocol": d.protocol.name(),
        })),
        "interface": iface,
        "network": net.map(|n| json!({
            "ip": n.ip.to_string(),
            "gateway": n.router.map(|r| r.to_string()),
            "dns": n.dns.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
        })),
        "traffic": c.map(|c| json!({
            "rx_bytes": c.rx_bytes.load(Ordering::Relaxed),
            "rx_packets": c.rx_packets.load(Ordering::Relaxed),
            "tx_bytes": c.tx_bytes.load(Ordering::Relaxed),
            "tx_packets": c.tx_packets.load(Ordering::Relaxed),
        })),
    })
    .to_string();
    if body == last.2 {
        return;
    }
    // Atomic replace; best effort (only root can write /var/run, and probe runs unprivileged).
    let tmp = format!("{PATH}.tmp");
    if std::fs::write(&tmp, &body).is_ok() && std::fs::rename(&tmp, PATH).is_ok() {
        last.2 = body;
    }
}

pub fn clear() {
    if read().is_some_and(|v| v["pid"] == std::process::id()) {
        let _ = std::fs::remove_file(PATH);
    }
}

/// The status of a *running* instance, if any.
pub fn read() -> Option<Value> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(PATH).ok()?).ok()?;
    let pid = v["pid"].as_u64()? as libc::pid_t;
    is_tether_process(pid).then_some(v)
}

/// True if `pid` is alive *and* is this program — a stale file whose PID was reused by an
/// unrelated process after a crash or reboot must not count as "running".
fn is_tether_process(pid: libc::pid_t) -> bool {
    let mut buf = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n > 0 {
        let exe = String::from_utf8_lossy(&buf[..n as usize]);
        return exe.rsplit('/').next() == Some(env!("CARGO_PKG_NAME"));
    }
    false
}
