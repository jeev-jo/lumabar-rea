//! Hiding and restoring the Windows taskbar.
//!
//! Safety rules (PLAN.md section 5):
//! 1. Before changing anything, the taskbar's original state is written to disk.
//! 2. It is restored on normal exit, on panic, on session end, and by a watchdog
//!    process if the dock is killed.
//! 3. On every launch a leftover state file is restored first.
//! 4. Only the documented auto-hide switch (`ABM_SETSTATE`) is used.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABM_GETSTATE, ABM_SETSTATE, ABS_AUTOHIDE, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

pub const WATCHDOG_FLAG: &str = "--taskbar-watchdog";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Deserialize)]
struct SavedState {
    /// `ABM_GETSTATE` flags before the dock changed them.
    original: u32,
}

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

/// Put the taskbar back the way it was, if we changed it. Safe to call any number
/// of times, from any thread, including the panic hook and the watchdog.
pub fn restore() {
    let path = state_file();
    let Ok(text) = fs::read_to_string(&path) else {
        return;
    };
    match serde_json::from_str::<SavedState>(&text) {
        Ok(saved) => {
            set_flags(saved.original);
            eprintln!("[taskbar] restored original state 0x{:x}", saved.original);
        }
        Err(e) => eprintln!("[taskbar] unreadable state file, leaving taskbar as is: {e}"),
    }
    let _ = fs::remove_file(path);
}

/// Switch the taskbar to auto-hide, remembering what it was.
pub fn hide() {
    // A leftover file means the last run never restored: put things back first,
    // so the "original" we record now is the user's real setting.
    restore();
    let original = get_flags();
    if let Err(e) = fs::create_dir_all(data_dir()).and_then(|_| {
        fs::write(
            state_file(),
            serde_json::to_string(&SavedState { original }).unwrap(),
        )
    }) {
        // Never change the taskbar without a way back.
        eprintln!("[taskbar] could not save state, not hiding the taskbar: {e}");
        return;
    }
    set_flags(original | ABS_AUTOHIDE);
    eprintln!("[taskbar] auto-hide on (was 0x{original:x})");
}

/// Restore the taskbar if a panic takes the dock down.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

/// Start a copy of this exe that waits for the dock to exit and then restores the
/// taskbar. Covers the cases no in-process handler can: the dock being killed or
/// crashing hard.
pub fn spawn_watchdog() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return eprintln!("[taskbar] no watchdog: {e}"),
    };
    let result = Command::new(exe)
        .arg(WATCHDOG_FLAG)
        .arg(std::process::id().to_string())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    if let Err(e) = result {
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
