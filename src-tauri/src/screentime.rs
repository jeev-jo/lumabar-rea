//! Screen time: how long you've used the PC today, hour by hour and per app.
//!
//! Today's total and the hourly chart come from Windows itself: the "Active"
//! periods in its power-state log (what Settings > Battery usage counts as
//! screen on), read with `powercfg /batteryreport` every few minutes. Windows
//! doesn't say which app was in front, so the per-app split is counted by the
//! dock with the same rule Windows uses for screen time: every 15 seconds the
//! screen is on and unlocked, the app in front gets those 15 seconds (when the
//! screen turns off or the PC sleeps, the dock isn't running ticks). If the
//! Windows record isn't available, the dock's own count is used for
//! everything. Kept only on this PC.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::{apps, taskbar};

const TICK: u64 = 15;

#[derive(Clone, Default, Serialize, Deserialize)]
struct AppTime {
    name: String,
    exe: String,
    secs: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Day {
    date: String,
    total: u64,
    hours: Vec<u64>,
    apps: HashMap<String, AppTime>,
}

static TODAY: Mutex<Option<Day>> = Mutex::new(None);

fn today() -> (String, usize) {
    let t = unsafe { GetLocalTime() };
    (format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay), t.wHour as usize)
}

fn file(date: &str) -> PathBuf {
    taskbar::data_dir().join("screentime").join(format!("{date}.json"))
}

fn load(date: &str) -> Day {
    fs::read_to_string(file(date))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| Day { date: date.to_string(), hours: vec![0; 24], ..Default::default() })
}

fn save(day: &Day) {
    let p = file(&day.date);
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(p, serde_json::to_string(day).unwrap_or_default());
}


/// The app in front: (grouping key, display name, exe path).
fn front_app() -> Option<(String, String, String)> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let exe = apps::process_exe(apps::window_pid(hwnd))?;
    let key = exe.to_lowercase();
    // The lock screen is not "using the PC".
    if key.ends_with("\\lockapp.exe") {
        return None;
    }
    let name = apps::app_name(&key, &exe, "");
    Some((key, name, exe))
}

fn tick() {
    if front_app().is_none() {
        return;
    }
    let (date, hour) = today();
    let mut guard = TODAY.lock().unwrap();
    if guard.as_ref().map(|d| d.date != date).unwrap_or(true) {
        if let Some(old) = guard.take() {
            save(&old);
        }
        *guard = Some(load(&date));
    }
    let day = guard.as_mut().unwrap();
    if day.hours.len() != 24 {
        day.hours = vec![0; 24];
    }
    day.total += TICK;
    day.hours[hour] += TICK;
    if let Some((key, name, exe)) = front_app() {
        let entry = day.apps.entry(key).or_insert_with(|| AppTime { name, exe, secs: 0 });
        entry.secs += TICK;
    }
    save(day);
}

pub fn init() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(TICK));
        tick();
    });
}

// ---------------------------------------------------------------------------
// Windows' own record of active (screen-on) time
// ---------------------------------------------------------------------------

/// Seconds since midnight for "HH:MM:SS".
fn secs_of_day(hms: &str) -> Option<u64> {
    let mut it = hms.split(':').map(|x| x.parse::<u64>().ok());
    Some(it.next()?? * 3600 + it.next()?? * 60 + it.next()??)
}

/// Pull `name="value"` out of one XML element's text.
fn attr<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = element.find(&key)? + key.len();
    let end = element[start..].find('"')?;
    Some(&element[start..start + end])
}

/// Today's active time from Windows: (total seconds, seconds per hour).
fn windows_active_today(date: &str, now: u64) -> Option<(u64, Vec<u64>)> {
    use std::os::windows::process::CommandExt;
    let out = taskbar::data_dir().join("screentime").join("battery-report.xml");
    let _ = fs::create_dir_all(out.parent()?);
    let status = std::process::Command::new("powercfg")
        .args(["/batteryreport", "/xml", "/output"])
        .arg(&out)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let xml = fs::read_to_string(&out).ok()?;
    let recent = &xml[xml.find("<RecentUsage")?..xml.find("</RecentUsage>")?];

    // (date, seconds since midnight, type) in order.
    let entries: Vec<(String, u64, String)> = recent
        .split("<UsageEntry")
        .skip(1)
        .filter_map(|e| {
            let ts = attr(e, "LocalTimestamp")?;
            let (d, t) = ts.split_once('T')?;
            Some((d.to_string(), secs_of_day(t)?, attr(e, "EntryType")?.to_string()))
        })
        .collect();
    if entries.is_empty() {
        return None;
    }

    let mut hours = vec![0u64; 24];
    let mut total = 0;
    for (i, (d, start, kind)) in entries.iter().enumerate() {
        if kind != "Active" {
            continue;
        }
        // Runs until the next entry (or now, if it is the last one).
        let (end_date, end) = entries
            .get(i + 1)
            .map(|(d2, e2, _)| (d2.as_str(), *e2))
            .unwrap_or((date, now));
        if end_date < date {
            continue; // finished before today
        }
        let from = if d.as_str() < date { 0 } else { *start };
        let to = if end_date > date { 86_400 } else { end.min(now) };
        let mut t = from;
        while t < to {
            let hour_end = ((t / 3600) + 1) * 3600;
            let chunk = hour_end.min(to) - t;
            hours[(t / 3600) as usize] += chunk;
            total += chunk;
            t += chunk;
        }
    }
    Some((total, hours))
}

/// Windows' figure, re-read at most every 5 minutes (powercfg takes a second).
static SYSTEM: Mutex<Option<(std::time::Instant, String, Option<(u64, Vec<u64>)>)>> = Mutex::new(None);

fn system_today(date: &str, now: u64) -> Option<(u64, Vec<u64>)> {
    {
        let cached = SYSTEM.lock().unwrap();
        if let Some((at, d, value)) = cached.as_ref() {
            if d == date && at.elapsed() < Duration::from_secs(300) {
                // Grow the last reading by the time since, while it is fresh.
                return value.clone().map(|(total, mut hours)| {
                    let extra = at.elapsed().as_secs();
                    let h = ((now / 3600) as usize).min(23);
                    hours[h] += extra;
                    (total + extra, hours)
                });
            }
        }
    }
    let value = windows_active_today(date, now);
    *SYSTEM.lock().unwrap() = Some((std::time::Instant::now(), date.to_string(), value.clone()));
    value
}

#[derive(Serialize)]
pub struct AppUsage {
    name: String,
    /// For the app's icon.
    exe: String,
    secs: u64,
}

#[derive(Serialize)]
pub struct Today {
    total: u64,
    /// Seconds used in each hour, 0–23.
    hours: Vec<u64>,
    /// Most-used apps first (top 8).
    apps: Vec<AppUsage>,
    /// Time in apps beyond the top 8.
    other_apps: u64,
    /// Total time the dock counted per app today (the rest of `total` was
    /// before the dock was running).
    tracked: u64,
    /// "windows" when the total and hours come from Windows' record, "dock" otherwise.
    source: &'static str,
}

#[tauri::command]
pub async fn screen_time_today() -> Today {
    let (date, _) = today();
    let t = unsafe { GetLocalTime() };
    let now = t.wHour as u64 * 3600 + t.wMinute as u64 * 60 + t.wSecond as u64;
    let date2 = date.clone();
    let system = tauri::async_runtime::spawn_blocking(move || system_today(&date2, now))
        .await
        .ok()
        .flatten();

    let mut guard = TODAY.lock().unwrap();
    if guard.as_ref().map(|d| d.date != date).unwrap_or(true) {
        *guard = Some(load(&date));
    }
    let day = guard.as_ref().unwrap();
    let mut apps: Vec<AppUsage> = day
        .apps
        .values()
        .map(|a| AppUsage { name: a.name.clone(), exe: a.exe.clone(), secs: a.secs })
        .collect();
    apps.sort_by(|a, b| b.secs.cmp(&a.secs));
    let tracked: u64 = apps.iter().map(|a| a.secs).sum();
    let other_apps: u64 = apps.iter().skip(8).map(|a| a.secs).sum();
    apps.truncate(8);
    match system {
        // Windows' total can't be below what the dock itself saw.
        Some((total, hours)) => Today { total: total.max(tracked), hours, apps, other_apps, tracked, source: "windows" },
        None => Today {
            total: day.total,
            hours: if day.hours.len() == 24 { day.hours.clone() } else { vec![0; 24] },
            apps,
            other_apps,
            tracked,
            source: "dock",
        },
    }
}
