// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(luma_ai_bar_lib::WATCHDOG_FLAG) {
        // A tiny helper copy of the exe: waits for the dock, then restores the taskbar.
        luma_ai_bar_lib::run_watchdog(args.get(2).map(String::as_str).unwrap_or(""));
        return;
    }
    luma_ai_bar_lib::run()
}
