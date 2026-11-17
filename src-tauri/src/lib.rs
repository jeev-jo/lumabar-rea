mod dock;

use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("the dock window is declared in tauri.conf.json");
            dock::init(&window)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dock::dock_get_edge,
            dock::dock_set_edge,
            dock::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building luma-ai-bar")
        .run(|_app, event| {
            if let RunEvent::Exit = event {
                dock::shutdown();
            }
        });
}
