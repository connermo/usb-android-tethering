//! Installs the tether as a launchd daemon so it runs automatically at boot and picks up
//! the phone whenever USB tethering is switched on.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub const LABEL: &str = "com.usb-android-tethering";
pub const PLIST: &str = "/Library/LaunchDaemons/com.usb-android-tethering.plist";
pub const BIN: &str = "/usr/local/bin/usb-android-tethering";
pub const LOG: &str = "/var/log/usb-android-tethering.log";

fn launchctl(args: &[&str]) -> Result<()> {
    let out = Command::new("/bin/launchctl")
        .args(args)
        .output()
        .context("run launchctl")?;
    if !out.status.success() {
        bail!(
            "launchctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

fn is_loaded() -> bool {
    Command::new("/bin/launchctl")
        .args(["print", &format!("system/{LABEL}")])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Unloads the job and waits until launchd has really let go of it (the daemon's teardown
/// takes a moment; bootstrapping again before that fails with "Input/output error").
fn bootout() {
    let _ = launchctl(&["bootout", &format!("system/{LABEL}")]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while is_loaded() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn bootstrap() -> Result<()> {
    let mut result = Ok(());
    for _ in 0..5 {
        result = launchctl(&["bootstrap", "system", PLIST]);
        if result.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    result
}

pub fn is_installed() -> bool {
    Path::new(PLIST).exists()
}

/// PID of the running daemon, if launchd has it running.
pub fn daemon_pid() -> Option<u32> {
    let out = Command::new("/bin/launchctl")
        .args(["print", &format!("system/{LABEL}")])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.trim().strip_prefix("pid = "))
        .and_then(|p| p.trim().parse().ok())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn install(run_args: &[String], verbose: u8, first: bool) -> Result<()> {
    let exe = std::env::current_exe().context("locate own executable")?;
    if exe != Path::new(BIN) {
        std::fs::create_dir_all("/usr/local/bin")?;
        // Copy to a temp name then rename, so a running daemon's binary is never half-written.
        let tmp = format!("{BIN}.new");
        std::fs::copy(&exe, &tmp).with_context(|| format!("copy binary to {BIN}"))?;
        std::fs::rename(&tmp, BIN)?;
    }
    // launchd runs this as root: it must not be writable by anyone else.
    let c = std::ffi::CString::new(BIN).unwrap();
    if unsafe { libc::chown(c.as_ptr(), 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error()).context(format!("chown {BIN}"));
    }
    std::fs::set_permissions(BIN, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
    let mut args = vec![
        BIN.to_string(),
        "run".into(),
        "--log-file".into(),
        LOG.into(),
    ];
    if verbose > 0 {
        args.push(format!("-{}", "v".repeat(verbose as usize)));
    }
    args.extend_from_slice(run_args);
    let args_xml: String = args
        .iter()
        .map(|a| format!("\t\t<string>{}</string>\n", xml_escape(a)))
        .collect();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
{args_xml}	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>ThrottleInterval</key>
	<integer>5</integer>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardErrorPath</key>
	<string>{LOG}</string>
</dict>
</plist>
"#
    );
    if is_installed() || is_loaded() {
        bootout();
    }
    // Permanent network service (shown as "not connected" while the phone is unplugged).
    crate::netservice::install(&crate::feth::host_name(), first)?;
    std::fs::write(PLIST, plist).with_context(|| format!("write {PLIST}"))?;
    bootstrap()
}

pub fn uninstall(purge_logs: bool) -> Result<bool> {
    let was = is_installed();
    if was {
        bootout();
        std::fs::remove_file(PLIST).with_context(|| format!("remove {PLIST}"))?;
    }
    // The daemon keeps the service while installed; remove it now that the plist is gone.
    crate::netservice::remove()?;
    if Path::new(BIN).exists() {
        std::fs::remove_file(BIN).with_context(|| format!("remove {BIN}"))?;
    }
    if purge_logs {
        let _ = std::fs::remove_file(LOG);
        let _ = std::fs::remove_file(format!("{LOG}.1"));
    }
    Ok(was)
}
