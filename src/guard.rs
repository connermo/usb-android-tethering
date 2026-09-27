//! Crash guard: a detached helper that waits for the tether process to exit and removes the
//! network service and interface if it died without cleaning up (e.g. SIGKILL or a panic).
//! Without it a dead tether could stay first in the service order until the next start.

use crate::{state, tether};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn spawn() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.args(["__guard", &std::process::id().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // New session: Ctrl+C in the terminal (or launchd stopping the job) doesn't reach it.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    if let Err(e) = cmd.spawn() {
        log::warn!("crash guard not started: {e}");
    }
}

/// Blocks until `pid` exits, using kqueue so the guard costs nothing while it waits.
fn wait_exit(pid: libc::pid_t) {
    unsafe {
        let kq = libc::kqueue();
        if kq >= 0 {
            let mut ev: libc::kevent = std::mem::zeroed();
            ev.ident = pid as usize;
            ev.filter = libc::EVFILT_PROC;
            ev.flags = libc::EV_ADD | libc::EV_ONESHOT;
            ev.fflags = libc::NOTE_EXIT;
            let registered =
                libc::kevent(kq, &ev, 1, std::ptr::null_mut(), 0, std::ptr::null()) == 0;
            // ESRCH: the process is already gone. Read errno before close() can change it.
            let already_gone =
                !registered && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            if registered {
                let mut out: libc::kevent = std::mem::zeroed();
                while libc::kevent(kq, std::ptr::null(), 0, &mut out, 1, std::ptr::null()) < 0
                    && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
                {
                }
            }
            libc::close(kq);
            if registered || already_gone {
                return;
            }
        }
        // Fallback: poll.
        while libc::kill(pid, 0) == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

pub fn run(pid: u32) {
    wait_exit(pid as libc::pid_t);
    // A newer instance (e.g. restarted by launchd) owns the network now: leave it alone.
    if state::read().is_some() {
        return;
    }
    tether::cleanup_leftovers();
    let _ = std::fs::remove_file(state::PATH);
}
