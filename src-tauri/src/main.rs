// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pid = args.get(2).map(String::as_str).unwrap_or("");
    match args.get(1).map(String::as_str) {
        // A tiny helper copy of the exe: waits for the dock, then restores the taskbar.
        Some(f) if f == luma_ai_bar_lib::WATCHDOG_FLAG => return luma_ai_bar_lib::run_watchdog(pid),
        // Starts that helper detached, then exits.
        Some(f) if f == luma_ai_bar_lib::WATCHDOG_LAUNCH_FLAG => return luma_ai_bar_lib::launch_watchdog(pid),
        _ => {}
    }
    luma_ai_bar_lib::run()
}
