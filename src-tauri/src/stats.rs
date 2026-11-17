//! CPU, memory and network throughput, sampled once a second for the system
//! monitor widget. The last 60 samples are kept so a window opened later can
//! draw a full chart straight away. Sampling starts the first time a widget asks.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

const HISTORY: usize = 60;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Sample {
    /// Milliseconds since the Unix epoch.
    pub t: u64,
    /// Total CPU use, 0–100.
    pub cpu: f32,
    /// Memory in use, 0–100.
    pub ram: f32,
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    /// Bytes per second across physical adapters.
    pub down: f64,
    pub up: f64,
}

static STARTED: AtomicBool = AtomicBool::new(false);
static HISTORY_BUF: Mutex<VecDeque<Sample>> = Mutex::new(VecDeque::new());

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn cpu_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    // Kernel time includes idle time.
    Some((ft(idle), ft(kernel) + ft(user)))
}

fn memory() -> (f32, f32, f32) {
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_err() || m.ullTotalPhys == 0 {
        return (0.0, 0.0, 0.0);
    }
    let gb = 1024.0 * 1024.0 * 1024.0;
    let used = (m.ullTotalPhys - m.ullAvailPhys) as f64;
    (
        (used / m.ullTotalPhys as f64 * 100.0) as f32,
        (used / gb) as f32,
        (m.ullTotalPhys as f64 / gb) as f32,
    )
}

/// Total bytes received and sent by physical, connected adapters.
fn net_octets() -> (u64, u64) {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).is_err() || table.is_null() {
            return (0, 0);
        }
        let n = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), n);
        let (mut down, mut up) = (0u64, 0u64);
        for r in rows {
            // Bit 0 of the flags = HardwareInterface; skips virtual and filter
            // adapters that would otherwise count the same traffic twice.
            let hardware = r.InterfaceAndOperStatusFlags._bitfield & 1 != 0;
            if hardware && r.Type != IF_TYPE_SOFTWARE_LOOPBACK && r.OperStatus == IfOperStatusUp {
                down += r.InOctets;
                up += r.OutOctets;
            }
        }
        FreeMibTable(table as *const _);
        (down, up)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn start(app: AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let mut prev_cpu = cpu_times();
        let mut prev_net = net_octets();
        let mut prev_at = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let cpu_now = cpu_times();
            let net_now = net_octets();
            let secs = prev_at.elapsed().as_secs_f64().max(0.001);
            prev_at = Instant::now();

            let cpu = match (prev_cpu, cpu_now) {
                (Some((i0, t0)), Some((i1, t1))) if t1 > t0 => {
                    (100.0 * (1.0 - (i1 - i0) as f64 / (t1 - t0) as f64)).clamp(0.0, 100.0) as f32
                }
                _ => 0.0,
            };
            prev_cpu = cpu_now;
            // Counters can reset when an adapter reconnects; treat that as zero.
            let down = net_now.0.saturating_sub(prev_net.0) as f64 / secs;
            let up = net_now.1.saturating_sub(prev_net.1) as f64 / secs;
            prev_net = net_now;

            let (ram, ram_used_gb, ram_total_gb) = memory();
            let sample = Sample {
                t: now_ms(),
                cpu,
                ram,
                ram_used_gb,
                ram_total_gb,
                down,
                up,
            };
            {
                let mut h = HISTORY_BUF.lock().unwrap();
                h.push_back(sample);
                while h.len() > HISTORY {
                    h.pop_front();
                }
            }
            let _ = app.emit("stats", sample);
        }
    });
}

/// Start sampling (if not already) and return what has been collected so far.
#[tauri::command]
pub fn stats_history(app: AppHandle) -> Vec<Sample> {
    start(app);
    HISTORY_BUF.lock().unwrap().iter().copied().collect()
}
