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
    ShowWindow, GWL_EXSTYLE, HWND_TOPMOST, MB_ICONASTERISK, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
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
    with_glance(|g| {
        g.pinned = false;
        g.over_dock = false;
        g.over_popover = false;
        g.generation += 1;
    });
    if let Some(h) = popover_hwnd(app) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

/// Hover-glance state. A popover opened by hovering is "unpinned": it closes
/// shortly after the pointer leaves both the widget and the popover. Clicking
/// the widget or into the popover pins it; then it closes on click-away.
#[derive(Default)]
struct Glance {
    pinned: bool,
    over_dock: bool,
    over_popover: bool,
    generation: u64,
}

static GLANCE: std::sync::Mutex<Option<Glance>> = std::sync::Mutex::new(None);

fn with_glance<R>(f: impl FnOnce(&mut Glance) -> R) -> R {
    f(GLANCE.lock().unwrap().get_or_insert_with(Glance::default))
}

/// The popover took focus (the user clicked into it): keep it open.
pub fn popover_focused() {
    with_glance(|g| g.pinned = true);
}

#[derive(Clone, Serialize)]
struct PopoverOpen {
    id: String,
}

/// Place the popover above the widget (CSS px sizes) and show it, taking focus
/// only when `focus` is set.
fn show_popover(app: &AppHandle, id: String, anchor_x: f64, width: f64, height: f64, focus: bool) -> Result<(), String> {
    let ph = popover_hwnd(app).ok_or("no popover window")?;
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
        let flags = if focus { SWP_SHOWWINDOW } else { SWP_SHOWWINDOW | SWP_NOACTIVATE };
        let _ = SetWindowPos(ph, Some(HWND_TOPMOST), x, y, w, h, flags);
        if focus {
            let _ = SetForegroundWindow(ph);
        }
    }
    Ok(())
}

fn is_open_for(app: &AppHandle, id: &str) -> bool {
    let visible = popover_hwnd(app).is_some_and(|h| unsafe { IsWindowVisible(h).as_bool() });
    visible && OPEN_FOR.lock().unwrap().as_deref() == Some(id)
}

/// Click on a widget: open its details with focus (so you can type), pin a
/// hover-opened popover, or close it if it was already pinned open.
#[tauri::command]
pub fn popover_toggle(
    app: AppHandle,
    id: String,
    anchor_x: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if is_open_for(&app, &id) {
        if with_glance(|g| g.pinned) {
            popover_hide_now(&app);
        } else {
            with_glance(|g| g.pinned = true);
            if let Some(h) = popover_hwnd(&app) {
                unsafe {
                    let _ = SetForegroundWindow(h);
                }
            }
        }
        return Ok(());
    }
    with_glance(|g| {
        g.pinned = true;
        g.generation += 1;
    });
    show_popover(&app, id, anchor_x, width, height, true)
}

/// Hover glance: the pointer has rested on a widget. Shows its details without
/// taking focus, unless a pinned popover is already open.
#[tauri::command]
pub fn popover_glance(
    app: AppHandle,
    id: String,
    anchor_x: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if !settings::get().glance {
        return Ok(());
    }
    let pinned_open = OPEN_FOR.lock().unwrap().is_some() && with_glance(|g| g.pinned);
    if pinned_open {
        return Ok(());
    }
    with_glance(|g| {
        g.over_dock = true;
        g.generation += 1;
    });
    if is_open_for(&app, &id) {
        return Ok(());
    }
    show_popover(&app, id, anchor_x, width, height, false)
}

/// The pointer entered or left a widget (`source = "dock"`) or the popover.
/// An unpinned popover hides shortly after the pointer is over neither.
#[tauri::command]
pub fn popover_hover(app: AppHandle, source: String, inside: bool) {
    let generation = with_glance(|g| {
        if source == "dock" {
            g.over_dock = inside;
        } else {
            g.over_popover = inside;
        }
        g.generation += 1;
        g.generation
    });
    if inside {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let close = with_glance(|g| g.generation == generation && !g.pinned && !g.over_dock && !g.over_popover);
        if close {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || popover_hide_now(&app2));
        }
    });
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
