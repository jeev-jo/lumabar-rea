//! Small system gadgets: the apps using the most CPU and memory, drive space,
//! the Recycle Bin, the microphone, Windows' light/dark theme, and Show desktop.
//! Everything is read on demand when a widget asks; nothing runs in the background.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, FILETIME, LPARAM, WPARAM};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eCapture, eConsole, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{
    GetActiveProcessorCount, GetProcessTimes, OpenProcess, ALL_PROCESSOR_GROUPS, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::apps;

// ---------------------------------------------------------------------------
// Top apps by CPU and memory
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
pub struct AppLoad {
    pub name: String,
    /// For the app's icon; empty for system processes.
    pub exe: String,
    /// Share of all CPU cores, 0–100, like Task Manager.
    pub cpu: f32,
    /// Working set in bytes, summed over the app's processes.
    pub memory: u64,
    pub processes: u32,
}

#[derive(Serialize)]
pub struct TopApps {
    pub by_cpu: Vec<AppLoad>,
    pub by_memory: Vec<AppLoad>,
    /// Seconds since Windows started.
    pub uptime: u64,
}

struct Proc {
    pid: u32,
    name: String,
    cpu_time: u64,
    memory: u64,
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn processes() -> Vec<Proc> {
    let mut out = vec![];
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return out };
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut ok = unsafe { Process32FirstW(snap, &mut entry) }.is_ok();
    while ok {
        let pid = entry.th32ProcessID;
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
        if pid != 0 {
            if let Ok(h) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
                let (mut c, mut e, mut k, mut u) = Default::default();
                let cpu_time = if unsafe { GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) }.is_ok() {
                    ft(k) + ft(u)
                } else {
                    0
                };
                let mut mem = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
                let memory = if unsafe { K32GetProcessMemoryInfo(h, &mut mem, mem.cb) }.as_bool() {
                    mem.WorkingSetSize as u64
                } else {
                    0
                };
                let _ = unsafe { CloseHandle(h) };
                out.push(Proc { pid, name, cpu_time, memory });
            }
        }
        ok = unsafe { Process32NextW(snap, &mut entry) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snap) };
    out
}

/// CPU time per process at the last reading, to turn totals into a rate.
static LAST: Mutex<Option<(Instant, HashMap<u32, u64>)>> = Mutex::new(None);
/// Display name and exe per pid, so each process is only looked up once.
static NAMES: Mutex<Option<HashMap<(u32, String), (String, String)>>> = Mutex::new(None);

fn name_of(pid: u32, image: &str) -> (String, String) {
    let mut guard = NAMES.lock().unwrap();
    let names = guard.get_or_insert_with(Default::default);
    names
        .entry((pid, image.to_string()))
        .or_insert_with(|| match apps::process_exe(pid) {
            Some(exe) => {
                let key = exe.to_lowercase();
                (apps::app_name(&key, &exe, ""), exe)
            }
            None => (image.trim_end_matches(".exe").to_string(), String::new()),
        })
        .clone()
}

fn top_apps_now() -> TopApps {
    let mut now = processes();
    let mut last = LAST.lock().unwrap();
    // The first reading has nothing to compare with: take a second one shortly after.
    if last.as_ref().is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(30)) {
        let first: HashMap<u32, u64> = now.iter().map(|p| (p.pid, p.cpu_time)).collect();
        *last = Some((Instant::now(), first));
        drop(last);
        std::thread::sleep(Duration::from_millis(600));
        now = processes();
        last = LAST.lock().unwrap();
    }
    let (at, before) = last.take().unwrap_or((Instant::now(), HashMap::new()));
    let elapsed_100ns = at.elapsed().as_nanos() as f64 / 100.0;
    let cores = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }.max(1) as f64;
    *last = Some((Instant::now(), now.iter().map(|p| (p.pid, p.cpu_time)).collect()));
    drop(last);

    // Group processes into apps, the way Task Manager does.
    let mut grouped: HashMap<String, AppLoad> = HashMap::new();
    let mut live = std::collections::HashSet::new();
    for p in &now {
        live.insert((p.pid, p.name.clone()));
        let (name, exe) = name_of(p.pid, &p.name);
        let delta = before.get(&p.pid).map(|&b| p.cpu_time.saturating_sub(b)).unwrap_or(0) as f64;
        let cpu = if elapsed_100ns > 0.0 { (delta / elapsed_100ns / cores * 100.0) as f32 } else { 0.0 };
        let key = if exe.is_empty() { p.name.to_lowercase() } else { exe.to_lowercase() };
        let e = grouped.entry(key).or_insert_with(|| AppLoad { name, exe, cpu: 0.0, memory: 0, processes: 0 });
        e.cpu += cpu;
        e.memory += p.memory;
        e.processes += 1;
    }
    // Forget names of processes that have exited.
    if let Some(names) = NAMES.lock().unwrap().as_mut() {
        names.retain(|k, _| live.contains(k));
    }

    let apps: Vec<AppLoad> = grouped.into_values().collect();
    let mut by_cpu = apps.clone();
    by_cpu.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    by_cpu.truncate(6);
    for a in &mut by_cpu {
        a.cpu = a.cpu.min(100.0);
    }
    let mut by_memory = apps;
    by_memory.sort_by(|a, b| b.memory.cmp(&a.memory));
    by_memory.truncate(6);
    let uptime = unsafe { windows::Win32::System::SystemInformation::GetTickCount64() } / 1000;
    TopApps { by_cpu, by_memory, uptime }
}

#[tauri::command]
pub async fn top_apps() -> TopApps {
    tauri::async_runtime::spawn_blocking(top_apps_now)
        .await
        .unwrap_or(TopApps { by_cpu: vec![], by_memory: vec![], uptime: 0 })
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Drive {
    /// "C:"
    pub letter: String,
    pub label: String,
    pub total: u64,
    pub free: u64,
    pub removable: bool,
}

#[tauri::command]
pub fn drives() -> Vec<Drive> {
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    let mask = unsafe { GetLogicalDrives() };
    let mut out = vec![];
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = format!("{}:", (b'A' + i as u8) as char);
        let root = HSTRING::from(format!("{letter}\\"));
        let kind = unsafe { GetDriveTypeW(&root) };
        if kind != DRIVE_FIXED && kind != DRIVE_REMOVABLE {
            continue;
        }
        let (mut free, mut total) = (0u64, 0u64);
        if unsafe { GetDiskFreeSpaceExW(&root, None, Some(&mut total), Some(&mut free)) }.is_err() || total == 0 {
            continue; // e.g. an empty card reader
        }
        let mut name = [0u16; 64];
        let label = if unsafe { GetVolumeInformationW(&root, Some(&mut name), None, None, None, None) }.is_ok() {
            let end = name.iter().position(|&c| c == 0).unwrap_or(0);
            String::from_utf16_lossy(&name[..end])
        } else {
            String::new()
        };
        out.push(Drive { letter, label, total, free, removable: kind == DRIVE_REMOVABLE });
    }
    out
}

#[tauri::command]
pub fn open_drive(letter: String) -> Result<(), String> {
    let ok = letter.len() == 2 && letter.as_bytes()[0].is_ascii_uppercase() && letter.ends_with(':');
    if !ok {
        return Err("not a drive".into());
    }
    shell_open(&format!("{letter}\\"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Recycle Bin
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RecycleBin {
    pub items: u64,
    pub bytes: u64,
}

#[tauri::command]
pub fn recycle_bin() -> RecycleBin {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    if unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.is_err() {
        return RecycleBin { items: 0, bytes: 0 };
    }
    RecycleBin { items: info.i64NumItems as u64, bytes: info.i64Size as u64 }
}

/// Empty it with Windows' own confirmation and progress dialogs.
#[tauri::command]
pub fn recycle_bin_empty() {
    use windows::Win32::UI::Shell::SHEmptyRecycleBinW;
    std::thread::spawn(|| {
        let _ = unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), 0) };
    });
}

#[tauri::command]
pub fn recycle_bin_open() {
    shell_open("shell:RecycleBinFolder");
}

// ---------------------------------------------------------------------------
// Microphone
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Mic {
    /// 0–100
    pub level: u32,
    pub muted: bool,
    pub device: String,
}

fn mic() -> Option<(IAudioEndpointVolume, String)> {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::System::Com::STGM_READ;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = devices.GetDefaultAudioEndpoint(eCapture, eConsole).ok()?;
        let name = device
            .OpenPropertyStore(STGM_READ)
            .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
            .and_then(|v| PropVariantToStringAlloc(&v))
            .map(|p| {
                let s = p.to_string().unwrap_or_default();
                windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
                s
            })
            .unwrap_or_default();
        // "Microphone (Realtek Audio)" reads better as just "Microphone".
        let short = name.split(" (").next().unwrap_or(&name).to_string();
        Some((device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()?, short))
    }
}

#[tauri::command]
pub fn mic_status() -> Option<Mic> {
    let (v, device) = mic()?;
    unsafe {
        Some(Mic {
            level: (v.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32,
            muted: v.GetMute().ok()?.as_bool(),
            device,
        })
    }
}

#[tauri::command]
pub fn mic_set(level: u32) -> Result<Option<Mic>, String> {
    let (v, _) = mic().ok_or("No microphone is connected.")?;
    unsafe {
        v.SetMasterVolumeLevelScalar(level.min(100) as f32 / 100.0, std::ptr::null()).map_err(|e| e.to_string())?;
    }
    Ok(mic_status())
}

#[tauri::command]
pub fn mic_toggle_mute() -> Result<Option<Mic>, String> {
    let (v, _) = mic().ok_or("No microphone is connected.")?;
    unsafe {
        let muted = v.GetMute().map_err(|e| e.to_string())?.as_bool();
        v.SetMute(!muted, std::ptr::null()).map_err(|e| e.to_string())?;
    }
    Ok(mic_status())
}

// ---------------------------------------------------------------------------
// Light / dark theme
// ---------------------------------------------------------------------------

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

fn read_dword(name: PCWSTR) -> Option<u32> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let mut value = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, PERSONALIZE, name, RRF_RT_REG_DWORD, None, Some(&mut value as *mut u32 as _), Some(&mut size))
            .ok()
            .ok()?;
    }
    Some(value)
}

/// "light" or "dark", for apps (what most windows follow).
#[tauri::command]
pub fn theme_get() -> &'static str {
    if read_dword(w!("AppsUseLightTheme")).unwrap_or(1) == 1 { "light" } else { "dark" }
}

/// Switch Windows and apps to light or dark, like Settings > Personalization > Colors.
#[tauri::command]
pub fn theme_set(mode: String) -> Result<&'static str, String> {
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    let light: u32 = match mode.as_str() {
        "light" => 1,
        "dark" => 0,
        _ => return Err("mode must be light or dark".into()),
    };
    for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, PERSONALIZE, name, REG_DWORD.0, Some(&light as *const u32 as _), 4)
                .ok()
                .map_err(|e| e.to_string())?;
        }
    }
    // Tell open windows (Explorer, apps, this dock) the colour mode changed.
    let what = w!("ImmersiveColorSet");
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(what.as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
    Ok(theme_get())
}

// ---------------------------------------------------------------------------
// Show desktop, Task Manager
// ---------------------------------------------------------------------------

/// Minimise everything (or bring it back), like the corner of the Windows taskbar.
#[tauri::command]
pub fn show_desktop() {
    crate::system::chord(&[
        windows::Win32::UI::Input::KeyboardAndMouse::VK_LWIN,
        windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(b'D' as u16),
    ]);
}

#[tauri::command]
pub fn open_task_manager() {
    shell_open("taskmgr.exe");
}

fn shell_open(target: &str) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(target), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}
