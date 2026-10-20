//! # Blaze RPC Message Tracer (--trace-blaze <path>)
//!
//! Writes newline-delimited JSON (NDJSON) records of every Blaze message:
//! `{"direction":"in"|"out","component":X,"command":Y,"tdf_hex":"...","timestamp_us":Z}`

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE_FILE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Enables writing Blaze NDJSON message traces to the specified path.
pub fn set_trace_path<P: AsRef<Path>>(path: P) {
    let p = path.as_ref().to_path_buf();
    *TRACE_FILE.write().unwrap() = Some(p);
    TRACE_ENABLED.store(true, Ordering::SeqCst);
}

/// Checks whether Blaze message tracing is active.
pub fn is_trace_enabled() -> bool {
    TRACE_ENABLED.load(Ordering::Relaxed)
}

fn to_hex_str(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    s
}

/// Appends an NDJSON record to the active trace file.
pub fn record_trace(direction: &str, comp: u16, cmd: u16, tdf_bytes: &[u8]) {
    if !TRACE_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let p_lock = TRACE_FILE.read().unwrap();
    if let Some(ref path) = *p_lock {
        let now_us = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let hex = to_hex_str(tdf_bytes);
        let json_line = format!(
            r#"{{"direction":"{}","component":{},"command":{},"tdf_hex":"{}","timestamp_us":{}}}"#,
            direction.to_lowercase(), comp, cmd, hex, now_us
        );

        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", json_line);
        }
    }
}
