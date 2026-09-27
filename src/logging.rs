//! Leveled logger: timestamped lines on stderr (coloured on a terminal unless NO_COLOR is
//! set) and optionally appended to a file that is rotated at 5 MB.

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ROTATE_AT: u64 = 5 * 1024 * 1024;

struct Logger {
    level: LevelFilter,
    color: bool,
    to_stderr: bool,
    /// Log file, its path, and bytes written so far (tracked to avoid a stat() per line).
    file: Option<Mutex<(File, PathBuf, u64)>>,
}

fn timestamp() -> String {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::gettimeofday(&mut tv, std::ptr::null_mut());
        libc::localtime_r(&tv.tv_sec, &mut tm);
    }
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        tv.tv_usec / 1000
    )
}

fn open_append(path: &Path) -> std::io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= self.level && m.target().starts_with("usb_android_tethering")
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let ts = timestamp();
        let lvl = match r.level() {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };
        if self.to_stderr {
            let line = if self.color {
                let c = match r.level() {
                    Level::Error => "31;1",
                    Level::Warn => "33;1",
                    Level::Info => "32",
                    Level::Debug => "36",
                    Level::Trace => "90",
                };
                format!("\x1b[90m{ts}\x1b[0m \x1b[{c}m{lvl}\x1b[0m {}\n", r.args())
            } else {
                format!("{ts} {lvl} {}\n", r.args())
            };
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
        if let Some(f) = &self.file {
            let line = format!("{ts} {lvl} {}\n", r.args());
            // A panic elsewhere must never silence (or crash) logging: ignore poisoning.
            let mut g = f.lock().unwrap_or_else(|e| e.into_inner());
            if g.0.write_all(line.as_bytes()).is_ok() {
                g.2 += line.len() as u64;
            }
            if g.2 > ROTATE_AT {
                let path = g.1.clone();
                let _ = std::fs::rename(&path, path.with_extension("log.1"));
                if let Ok(new) = open_append(&path) {
                    g.0 = new;
                    g.2 = 0;
                }
            }
        }
    }

    fn flush(&self) {}
}

/// `verbosity`: -1 quiet (warnings+), 0 info, 1 debug, 2+ trace.
pub fn init(verbosity: i8, file: Option<&Path>) -> anyhow::Result<()> {
    let level = match verbosity {
        i8::MIN..=-1 => LevelFilter::Warn,
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };
    let stderr_tty = std::io::stderr().is_terminal();
    let file = match file {
        Some(p) => {
            let f = open_append(p)
                .map_err(|e| anyhow::anyhow!("cannot open log file {}: {e}", p.display()))?;
            let size = f.metadata().map(|m| m.len()).unwrap_or(0);
            Some(Mutex::new((f, p.to_path_buf(), size)))
        }
        None => None,
    };
    let logger = Logger {
        level,
        color: stderr_tty && std::env::var_os("NO_COLOR").is_none(),
        // Under launchd (no terminal) with a log file, don't write every line twice.
        to_stderr: stderr_tty || file.is_none(),
        file,
    };
    log::set_boxed_logger(Box::new(logger)).map_err(|e| anyhow::anyhow!("{e}"))?;
    log::set_max_level(level);
    Ok(())
}
