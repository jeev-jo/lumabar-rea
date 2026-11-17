//! Hiding and restoring the Windows taskbar.
//!
//! Two strengths:
//! - auto-hide: the documented `ABM_SETSTATE` switch. The taskbar still slides in
//!   for Start, Search and when the pointer touches the screen edge.
//! - complete: auto-hide *plus* hiding the taskbar windows themselves (main and
//!   every secondary-monitor taskbar). Undocumented, so it is verified after every
//!   attempt and falls back to auto-hide if Windows does not cooperate.
//!
//! Safety rules (PLAN.md section 5):
//! 1. Before changing anything, the taskbar's original state is written to disk.
//! 2. It is restored on normal exit, on panic, on session end, and by a watchdog
//!    process if the dock is killed.
//! 3. On every launch a leftover state file is restored first.
//! 4. Explorer is never killed or patched; restoring always shows the taskbar
//!    windows again and puts the original auto-hide flags back.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetClassNameW, IsWindowVisible, ShowWindow, SW_HIDE, SW_SHOWNA,
};

pub const WATCHDOG_FLAG: &str = "--taskbar-watchdog";
/// Starts the watchdog and exits at once, so the watchdog is an orphan that
/// "end process tree" or a dev-server restart of the dock cannot reach.
pub const WATCHDOG_LAUNCH_FLAG: &str = "--taskbar-watchdog-launch";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

#[derive(Serialize, Deserialize)]
struct SavedState {
    /// `ABM_GETSTATE` flags before the dock changed them.
    original: u32,
    /// The taskbar windows were hidden too, so restoring must show them again.
    #[serde(default)]
    complete: bool,
}

/// Complete hiding is in force (re-applied whenever Explorer re-shows the taskbar).
static COMPLETE_ACTIVE: AtomicBool = AtomicBool::new(false);
/// The last attempt at complete hiding did not stick; auto-hide is in use instead.
static COMPLETE_FAILED: AtomicBool = AtomicBool::new(false);

/// `%LOCALAPPDATA%\luma_ai_bar` (no Tauri needed, so the watchdog can use it too).
pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("luma_ai_bar")
}

fn state_file() -> PathBuf {
    data_dir().join("taskbar-state.json")
}

fn appbar_data() -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.unwrap_or(HWND::default()),
        ..Default::default()
    }
}

fn get_flags() -> u32 {
    let mut abd = appbar_data();
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut abd) as u32 }
}

fn set_flags(flags: u32) {
    let mut abd = appbar_data();
    abd.lParam = LPARAM(flags as isize);
    unsafe { SHAppBarMessage(ABM_SETSTATE, &mut abd) };
}

pub fn is_taskbar_class(hwnd: HWND) -> bool {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) }.max(0) as usize;
    matches!(
        String::from_utf16_lossy(&buf[..n]).as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

unsafe extern "system" fn collect_taskbar(hwnd: HWND, l: LPARAM) -> BOOL {
    if is_taskbar_class(hwnd) {
        unsafe { (*(l.0 as *mut Vec<HWND>)).push(hwnd) };
    }
    BOOL(1)
}

/// The main taskbar and every secondary-monitor taskbar.
fn taskbar_windows() -> Vec<HWND> {
    let mut out: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_taskbar), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

fn show_taskbars() {
    for h in taskbar_windows() {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNA);
        }
    }
}

/// Hide every taskbar window; true if all of them ended up hidden.
fn hide_taskbars() -> bool {
    let bars = taskbar_windows();
    if bars.is_empty() {
        return false;
    }
    for &h in &bars {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
    bars.iter().all(|&h| unsafe { !IsWindowVisible(h).as_bool() })
}

fn save(state: &SavedState) -> std::io::Result<()> {
    fs::create_dir_all(data_dir())?;
    fs::write(state_file(), serde_json::to_string(state).unwrap())
}

/// Put the taskbar back the way it was, if we changed it. Safe to call any number
/// of times, from any thread, including the panic hook and the watchdog.
pub fn restore() {
    COMPLETE_ACTIVE.store(false, Ordering::SeqCst);
    let path = state_file();
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    match serde_json::from_str::<SavedState>(&text) {
        Ok(saved) => {
            if saved.complete {
                show_taskbars();
            }
            set_flags(saved.original);
            eprintln!(
                "[taskbar] restored original state 0x{:x}{}",
                saved.original,
                if saved.complete { " and showed the taskbar" } else { "" }
            );
        }
        Err(e) => {
            // Never leave the taskbar hidden, even if the file is damaged.
            show_taskbars();
            eprintln!("[taskbar] unreadable state file, showed the taskbar: {e}");
        }
    }
    let _ = fs::remove_file(path);
}

/// Switch the taskbar to auto-hide (and, if `complete`, hide it outright),
/// remembering what it was.
fn apply(complete: bool) {
    // A leftover file means the last run never restored: put things back first,
    // so the "original" we record now is the user's real setting.
    restore();
    COMPLETE_FAILED.store(false, Ordering::SeqCst);
    let original = get_flags();
    if let Err(e) = save(&SavedState { original, complete }) {
        // Never change the taskbar without a way back.
        eprintln!("[taskbar] could not save state, not hiding the taskbar: {e}");
        return;
    }
    // Auto-hide also frees the screen space the taskbar reserves.
    set_flags(original | ABS_AUTOHIDE);
    if !complete {
        eprintln!("[taskbar] auto-hide on (was 0x{original:x})");
        return;
    }
    if hide_taskbars() {
        COMPLETE_ACTIVE.store(true, Ordering::SeqCst);
        eprintln!("[taskbar] hidden completely (was 0x{original:x})");
    } else {
        show_taskbars();
        COMPLETE_FAILED.store(true, Ordering::SeqCst);
        eprintln!("[taskbar] complete hiding did not stick - using auto-hide instead");
    }
}

pub fn hide() {
    apply(false);
}

pub fn hide_completely() {
    apply(true);
}

/// Explorer re-showed a taskbar window (for Start, after a restart...): hide it again.
pub fn on_taskbar_shown(hwnd: HWND) {
    if COMPLETE_ACTIVE.load(Ordering::SeqCst) && is_taskbar_class(hwnd) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Explorer restarted and rebuilt the taskbar: put complete hiding back.
pub fn on_taskbar_recreated() {
    if COMPLETE_ACTIVE.load(Ordering::SeqCst) && !hide_taskbars() {
        COMPLETE_ACTIVE.store(false, Ordering::SeqCst);
        COMPLETE_FAILED.store(true, Ordering::SeqCst);
        show_taskbars();
        eprintln!("[taskbar] could not re-hide after Explorer restart - auto-hide only");
    }
}

#[derive(Serialize)]
pub struct TaskbarStatus {
    /// "complete", "auto-hide" or "none"
    pub applied: &'static str,
    /// Complete hiding was asked for but Windows would not keep it hidden.
    pub complete_failed: bool,
}

#[tauri::command]
pub fn taskbar_status() -> TaskbarStatus {
    let applied = if COMPLETE_ACTIVE.load(Ordering::SeqCst) {
        "complete"
    } else if state_file().exists() {
        "auto-hide"
    } else {
        "none"
    };
    TaskbarStatus {
        applied,
        complete_failed: COMPLETE_FAILED.load(Ordering::SeqCst),
    }
}

/// Restore the taskbar if a panic takes the dock down.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

/// Start a detached copy of this exe with `flag <pid>`. Tries to leave any job
/// object too (so a job that kills its members on close can't take it down).
fn spawn_detached(flag: &str, pid: u32) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let base = CREATE_NO_WINDOW | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    let run = |flags: u32| {
        Command::new(&exe)
            .arg(flag)
            .arg(pid.to_string())
            .creation_flags(flags)
            .spawn()
            .map(|_| ())
    };
    // Breaking away is refused when the job does not allow it; then stay in it.
    run(base | CREATE_BREAKAWAY_FROM_JOB).or_else(|_| run(base))
}

/// Start the watchdog: a copy of this exe that waits for the dock to exit and then
/// restores the taskbar. Covers the cases no in-process handler can: the dock
/// being killed (even with its whole process tree) or crashing hard.
pub fn spawn_watchdog() {
    if let Err(e) = spawn_detached(WATCHDOG_LAUNCH_FLAG, std::process::id()) {
        eprintln!("[taskbar] no watchdog: {e}");
    }
}

/// Entry point for `--taskbar-watchdog-launch <pid>`: start the real watchdog and exit.
pub fn launch_watchdog(pid: &str) {
    let Ok(pid) = pid.parse::<u32>() else { return };
    if let Err(e) = spawn_detached(WATCHDOG_FLAG, pid) {
        eprintln!("[taskbar] no watchdog: {e}");
    }
}

/// Entry point when started with `--taskbar-watchdog <pid>`.
pub fn run_watchdog(pid: &str) {
    let Ok(pid) = pid.parse::<u32>() else { return };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            // Dock already gone: restore straight away.
            restore();
            return;
        };
        WaitForSingleObject(handle, INFINITE);
        let _ = CloseHandle(handle);
    }
    restore();
}
