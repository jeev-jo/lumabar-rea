//! Screen time: how long you've used the PC today, hour by hour and per app.
//!
//! Every 15 seconds, if you've touched the mouse or keyboard in the last two
//! minutes, those 15 seconds are added to today's total, the current hour, and
//! the app in front. Kept only on this PC, one small file per day.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::{apps, taskbar};

const TICK: u64 = 15;
const IDLE_AFTER_MS: u32 = 2 * 60 * 1000;

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

fn idle_ms() -> u32 {
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe {
        if !GetLastInputInfo(&mut info).as_bool() {
            return 0;
        }
        GetTickCount().wrapping_sub(info.dwTime)
    }
}

/// The app in front: (grouping key, display name, exe path).
fn front_app() -> Option<(String, String, String)> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let exe = apps::process_exe(apps::window_pid(hwnd))?;
    let key = exe.to_lowercase();
    let name = apps::app_name(&key, &exe, "");
    Some((key, name, exe))
}

fn tick() {
    if idle_ms() > IDLE_AFTER_MS {
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
    /// Most-used apps first.
    apps: Vec<AppUsage>,
}

#[tauri::command]
pub fn screen_time_today() -> Today {
    let (date, _) = today();
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
    apps.truncate(8);
    Today {
        total: day.total,
        hours: if day.hours.len() == 24 { day.hours.clone() } else { vec![0; 24] },
        apps,
    }
}
