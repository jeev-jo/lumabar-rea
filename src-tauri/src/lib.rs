mod ai_usage;
mod apps;
mod backdrop;
mod dock;
mod preview;
mod secrets;
mod settings;
mod stats;
mod weather;
mod widgets;
mod system;
mod taskbar;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub use taskbar::{launch_watchdog, run_watchdog, WATCHDOG_FLAG, WATCHDOG_LAUNCH_FLAG};

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
            // Closing Settings only hides it, so it opens instantly next time.
            // The widget popover closes when you click anywhere else.
            if window.label() == widgets::POPOVER {
                match event {
                    WindowEvent::Focused(false) => widgets::popover_hide_now(window.app_handle()),
                    WindowEvent::Focused(true) => widgets::popover_focused(),
                    _ => {}
                }
            }
            if window.label() == "settings" || window.label() == widgets::PICKER {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    if window.label() == widgets::PICKER {
                        widgets::picker_closed(window.app_handle());
                    }
                }
            }
        })
        .setup(|app| {
            // Always undo a previous run that never cleaned up, before anything else.
            taskbar::restore();
            taskbar::spawn_watchdog();
            settings::apply_taskbar_mode(settings::get().taskbar_mode);

            let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let widgets_item = MenuItem::with_id(app, "widgets", "Widgets…", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit luma-ai-bar", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&settings_item, &widgets_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("luma-ai-bar")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => settings::open_window(app),
                    "widgets" => widgets::open_picker(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        settings::open_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if let Err(e) = app.global_shortcut().register(panic_shortcut()) {
                eprintln!("[dock] panic hotkey unavailable: {e}");
            }

            let window = app
                .get_webview_window("main")
                .expect("the dock window is declared in tauri.conf.json");
            dock::init(&window)?;
            settings::apply_style(app.handle(), settings::get().style);
            apps::init(app.handle(), &window)?;
            preview::init(app.handle())?;
            system::init(app.handle());
            widgets::init(app.handle());

            // Dev-only test hook: LUMA_DEBUG_SETTINGS=1 opens the settings window at start.
            #[cfg(debug_assertions)]
            if std::env::var_os("LUMA_DEBUG_SETTINGS").is_some() {
                settings::open_window(app.handle());
            }

            // Dev-only test hooks: LUMA_DEBUG_WIDGETS=1 opens the widget picker;
            // LUMA_DEBUG_POPOVER=<kind> opens that widget's expanded view.
            #[cfg(debug_assertions)]
            {
                if std::env::var_os("LUMA_DEBUG_WIDGETS").is_some() {
                    widgets::open_picker(app.handle());
                }
                if let Ok(kind) = std::env::var("LUMA_DEBUG_POPOVER") {
                    let handle = app.handle().clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        let Some(w) = settings::get().widgets.into_iter().find(|w| w.kind == kind) else {
                            eprintln!("[widgets] LUMA_DEBUG_POPOVER: no {kind} widget in the dock");
                            return;
                        };
                        let h = handle.clone();
                        let _ = handle.run_on_main_thread(move || {
                            let size = match w.kind.as_str() {
                                "system" => (360.0, 400.0),
                                "weather" => (340.0, 250.0),
                                _ => (330.0, 380.0),
                            };
                            if let Err(e) = widgets::popover_toggle(h, w.id, 700.0, size.0, size.1) {
                                eprintln!("[widgets] debug popover failed: {e}");
                            }
                        });
                    });
                }
            }

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
            dock::quit_app,
            dock::dock_set_bar_width,
            settings::get_settings,
            settings::update_settings,
            taskbar::taskbar_status,
            settings::reset_pins,
            settings::list_displays,
            settings::get_autostart,
            settings::set_autostart,
            settings::open_settings,
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
            system::system_status,
            system::open_flyout,
            system::volume_step,
            system::volume_toggle_mute,
            system::media_control,
            stats::stats_history,
            weather::weather_get,
            weather::place_search,
            widgets::widgets_add,
            widgets::widgets_remove,
            widgets::widgets_reorder,
            widgets::widget_update,
            widgets::widget_data_get,
            widgets::widget_data_set,
            widgets::play_alert,
            widgets::popover_toggle,
            widgets::popover_hide,
            widgets::popover_glance,
            widgets::popover_hover,
            widgets::open_widget_picker,
            widgets::close_widget_picker,
            widgets::widgets_edit_mode,
            ai_usage::ai_key_status,
            ai_usage::ai_set_key,
            ai_usage::ai_clear_key,
            ai_usage::ai_claude_code_found,
            ai_usage::ai_usage_fetch,
            ai_usage::service_icon,
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
