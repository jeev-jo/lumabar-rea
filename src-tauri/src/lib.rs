mod apps;
mod dock;
mod settings;
mod taskbar;

use tauri::{Manager, RunEvent};
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
