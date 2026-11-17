//! The widget framework's Rust side:
//! - which widgets are in the dock (stored in settings, broadcast on change),
//! - each widget's own data (todos, notes...), one JSON file per widget,
//! - the popover window that shows a widget's expanded view,
//! - the widget picker window.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, IsWindowVisible, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, MB_ICONASTERISK, SWP_SHOWWINDOW, SW_HIDE,
    WS_EX_TOOLWINDOW,
};

use crate::dock::{self, Edge};
use crate::{settings, taskbar};

pub const POPOVER: &str = "popover";
pub const PICKER: &str = "widgets";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WidgetInstance {
    pub id: String,
    /// Which widget: "system", "weather", "todo", "notes", "pomodoro", "worldclock".
    pub kind: String,
    /// "s", "m" or "l".
    pub size: String,
    /// Per-widget options (city, time zones, durations...).
    #[serde(default)]
    pub config: Value,
    /// Which side of the app icons it sits on: "left" or "right".
    #[serde(default = "default_side")]
    pub side: String,
}

fn default_side() -> String {
    "right".into()
}

fn clean_side(side: Option<String>) -> Option<String> {
    side.filter(|s| s == "left" || s == "right")
}

fn broadcast(app: &AppHandle) {
    let _ = app.emit("settings-changed", settings::get());
}

fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("w{nanos:x}")
}

#[tauri::command]
pub fn widgets_add(
    app: AppHandle,
    kind: String,
    size: String,
    config: Option<Value>,
    side: Option<String>,
) -> String {
    let id = new_id();
    let inst = WidgetInstance {
        id: id.clone(),
        kind,
        size,
        config: config.unwrap_or(Value::Object(Default::default())),
        side: clean_side(side).unwrap_or_else(default_side),
    };
    settings::update(|s| s.widgets.push(inst));
    broadcast(&app);
    id
}

#[tauri::command]
pub fn widgets_remove(app: AppHandle, id: String) {
    settings::update(|s| s.widgets.retain(|w| w.id != id));
    let _ = fs::remove_file(data_file(&id));
    // An AI usage widget's encrypted key and cached reading go with it.
    crate::ai_usage::forget(&id);
    broadcast(&app);
}

/// Put widgets in the order of `ids`.
#[tauri::command]
pub fn widgets_reorder(app: AppHandle, ids: Vec<String>) {
    settings::update(|s| {
        s.widgets
            .sort_by_key(|w| ids.iter().position(|x| *x == w.id).unwrap_or(usize::MAX))
    });
    broadcast(&app);
}

#[tauri::command]
pub fn widget_update(
    app: AppHandle,
    id: String,
    size: Option<String>,
    config: Option<Value>,
    side: Option<String>,
) {
    settings::update(|s| {
        if let Some(w) = s.widgets.iter_mut().find(|w| w.id == id) {
            if let Some(size) = size {
                w.size = size;
            }
            if let Some(side) = clean_side(side) {
                w.side = side;
            }
            if let Some(config) = config {
                w.config = config;
            }
        }
    });
    broadcast(&app);
}

// ---------------------------------------------------------------------------
// Per-widget data
// ---------------------------------------------------------------------------

fn data_file(id: &str) -> PathBuf {
    // Ids are generated here ("w" + hex); refuse anything else so a bad id can't
    // reach outside the folder.
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    taskbar::data_dir().join("widgets").join(format!("{safe}.json"))
}

#[tauri::command]
pub fn widget_data_get(id: String) -> Value {
    fs::read_to_string(data_file(&id))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null)
}

#[derive(Clone, Serialize)]
struct DataChanged {
    id: String,
    data: Value,
}

#[tauri::command]
pub fn widget_data_set(app: AppHandle, id: String, data: Value) -> Result<(), String> {
    let path = data_file(&id);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(&path, serde_json::to_string(&data).unwrap()).map_err(|e| e.to_string())?;
    let _ = app.emit("widget-data-changed", DataChanged { id, data });
    Ok(())
}

#[tauri::command]
pub fn play_alert() {
    unsafe {
        let _ = MessageBeep(MB_ICONASTERISK);
    }
}

// ---------------------------------------------------------------------------
// Popover
// ---------------------------------------------------------------------------

static OPEN_FOR: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn popover_hwnd(app: &AppHandle) -> Option<HWND> {
    let w = app.get_webview_window(POPOVER)?;
    Some(HWND(w.hwnd().ok()?.0 as _))
}

/// Called once from setup: keep the popover out of Alt-Tab and the taskbar.
/// Unlike the dock it can take focus, so text can be typed into it.
pub fn init(app: &AppHandle) {
    if let Some(h) = popover_hwnd(app) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_TOOLWINDOW.0 as isize);
        }
        dock::round_corners(h);
    }
}

pub fn popover_hide_now(app: &AppHandle) {
    *OPEN_FOR.lock().unwrap() = None;
    if let Some(h) = popover_hwnd(app) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

#[derive(Clone, Serialize)]
struct PopoverOpen {
    id: String,
}

/// Open the expanded view of widget `id` above it (CSS px sizes), or close it
/// if it is already open for that widget.
#[tauri::command]
pub fn popover_toggle(
    app: AppHandle,
    id: String,
    anchor_x: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let ph = popover_hwnd(&app).ok_or("no popover window")?;
    let visible = unsafe { IsWindowVisible(ph).as_bool() };
    if visible && OPEN_FOR.lock().unwrap().as_deref() == Some(id.as_str()) {
        popover_hide_now(&app);
        return Ok(());
    }

    let dock_win = app.get_webview_window("main").ok_or("no dock window")?;
    let scale = dock_win.scale_factor().map_err(|e| e.to_string())?;
    let dock_pos = dock_win.outer_position().map_err(|e| e.to_string())?;
    let dock_size = dock_win.outer_size().map_err(|e| e.to_string())?;
    let monitor = dock_win
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no monitor")?;
    let (mx, mw) = (monitor.position().x, monitor.size().width as i32);

    let w = (width * scale).round() as i32;
    let h = (height * scale).round() as i32;
    let anchor = dock_pos.x + (anchor_x * scale).round() as i32;
    let x = (anchor - w / 2).clamp(mx + 8, (mx + mw - w - 8).max(mx + 8));
    let y = match dock::current_edge() {
        Edge::Bottom => dock_pos.y - h,
        Edge::Top => dock_pos.y + dock_size.height as i32,
    };

    *OPEN_FOR.lock().unwrap() = Some(id.clone());
    app.emit_to(POPOVER, "popover-open", PopoverOpen { id })
        .map_err(|e| e.to_string())?;
    unsafe {
        let _ = SetWindowPos(ph, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
        let _ = SetForegroundWindow(ph);
    }
    Ok(())
}

#[tauri::command]
pub fn popover_hide(app: AppHandle) {
    popover_hide_now(&app);
}

// ---------------------------------------------------------------------------
// Picker
// ---------------------------------------------------------------------------

/// The dock is in edit mode (drag to arrange, badges to remove) while the
/// picker window is open.
static EDITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn set_editing(app: &AppHandle, on: bool) {
    EDITING.store(on, std::sync::atomic::Ordering::SeqCst);
    let _ = app.emit("edit-mode", on);
}

pub fn open_picker(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PICKER) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        set_editing(app, true);
    }
}

/// Called when the picker window is closed (it is only hidden).
pub fn picker_closed(app: &AppHandle) {
    set_editing(app, false);
}

#[tauri::command]
pub fn widgets_edit_mode() -> bool {
    EDITING.load(std::sync::atomic::Ordering::SeqCst)
}

#[tauri::command]
pub fn close_widget_picker(app: AppHandle) {
    if let Some(w) = app.get_webview_window(PICKER) {
        let _ = w.hide();
    }
    picker_closed(&app);
}

#[tauri::command]
pub fn open_widget_picker(app: AppHandle) {
    open_picker(&app);
}
