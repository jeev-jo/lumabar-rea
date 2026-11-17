mod apps;
mod dock;
mod preview;
mod settings;
mod taskbar;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub use taskbar::{run_watchdog, WATCHDOG_FLAG};

/// Ctrl+Alt+Shift+Q: quit the dock and give the taskbar back, whatever state the UI is in.
fn panic_shortcut() -> Shortcut {
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
        Code::KeyQ,
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    taskbar::install_panic_hook();

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if shortcut == &panic_shortcut() && event.state() == ShortcutState::Pressed {
                        eprintln!("[dock] panic hotkey pressed - quitting");
                        app.exit(0);
                    }
                })
                .build(),
        )
        // The dock window is the app: if it goes away for any reason, exit (which
        // restores the taskbar) rather than lingering invisibly.
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .setup(|app| {
            // Always undo a previous run that never cleaned up, before anything else.
            taskbar::restore();
            taskbar::spawn_watchdog();
            if settings::get().taskbar_mode == settings::TaskbarMode::Hide {
                taskbar::hide();
            }

            if let Err(e) = app.global_shortcut().register(panic_shortcut()) {
                eprintln!("[dock] panic hotkey unavailable: {e}");
            }

            let window = app
                .get_webview_window("main")
                .expect("the dock window is declared in tauri.conf.json");
            dock::init(&window)?;
            apps::init(app.handle(), &window)?;
            preview::init(app.handle())?;

            // Dev-only test hook: LUMA_DEBUG_PREVIEW=chrome opens Chrome's preview
            // after start-up, so previews can be checked without moving the mouse.
            #[cfg(debug_assertions)]
            if let Ok(needle) = std::env::var("LUMA_DEBUG_PREVIEW") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    let found = apps::snapshot_for_debug()
                        .into_iter()
                        .find(|a| a.key.contains(&needle.to_lowercase()) && !a.windows.is_empty());
                    if let Some(entry) = found {
                        let h = handle.clone();
                        let _ = handle.run_on_main_thread(move || {
                            let width = h
                                .get_webview_window("main")
                                .and_then(|w| Some(w.inner_size().ok()?.width as f64 / w.scale_factor().ok()?))
                                .unwrap_or(800.0);
                            if let Err(e) = preview::preview_show(h, entry.key, width / 2.0) {
                                eprintln!("[preview] debug show failed: {e}");
                            }
                        });
                    } else {
                        eprintln!("[preview] LUMA_DEBUG_PREVIEW: no running app matches {needle}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dock::dock_set_edge,
            dock::quit_app,
            settings::get_settings,
            settings::set_taskbar_mode,
            apps::apps_snapshot,
            apps::app_icon,
            apps::app_activate,
            apps::app_launch,
            apps::window_focus,
            apps::window_close,
            apps::app_close_all,
            apps::app_pin,
            apps::app_unpin,
            apps::pins_reorder,
            preview::preview_show,
            preview::preview_layout,
            preview::preview_hover,
            preview::preview_hide,
        ])
        .build(tauri::generate_context!())
        .expect("error while building luma-ai-bar")
        .run(|_app, event| {
            if let RunEvent::Exit = event {
                dock::shutdown();
                taskbar::restore();
            }
        });
}
