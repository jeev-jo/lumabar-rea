//! Hover previews: a popup above an app's icon with a live DWM thumbnail of
//! each of its windows.
//!
//! The popup's web page lays out the cards and reports where each thumbnail
//! goes; DWM then composites the live window image into those rectangles.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties, DWM_THUMBNAIL_PROPERTIES,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
};

use crate::apps::{self, WindowInfo};
use crate::dock::{self, Edge};

pub const LABEL: &str = "preview";

// Card geometry in CSS px. Keep in sync with preview.css.
const CARD_W: f64 = 232.0;
const CARD_H: f64 = 176.0;
const GAP: f64 = 8.0;
const PAD: f64 = 10.0;
const HIDE_DELAY: Duration = Duration::from_millis(220);

const DWM_TNP_RECTDESTINATION: u32 = 0x1;
const DWM_TNP_OPACITY: u32 = 0x4;
const DWM_TNP_VISIBLE: u32 = 0x8;
const DWM_TNP_SOURCECLIENTAREAONLY: u32 = 0x10;

#[derive(Default)]
struct State {
    thumbs: Vec<isize>,
    over_dock: bool,
    over_preview: bool,
    /// Bumped on every hover change so stale delayed hides do nothing.
    generation: u64,
    key: Option<String>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap().get_or_insert_with(State::default))
}

#[derive(Clone, Serialize)]
struct PreviewData {
    key: String,
    name: String,
    icon: String,
    windows: Vec<WindowInfo>,
}

#[derive(Deserialize)]
pub struct ThumbRect {
    hwnd: i64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn preview_hwnd(app: &AppHandle) -> Option<(WebviewWindow, HWND)> {
    let w = app.get_webview_window(LABEL)?;
    let h = HWND(w.hwnd().ok()?.0 as _);
    Some((w, h))
}

fn clear_thumbs() {
    for id in with(|s| std::mem::take(&mut s.thumbs)) {
        let _ = unsafe { DwmUnregisterThumbnail(id) };
    }
}

fn hide(app: &AppHandle) {
    clear_thumbs();
    with(|s| s.key = None);
    if let Some((_, h)) = preview_hwnd(app) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

/// Called once from setup.
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    if let Some((_, h)) = preview_hwnd(app) {
        dock::make_tool_window(h);
    }
    Ok(())
}

/// Show the popup for `key`, centred over `anchor_x` (CSS px from the dock window's left).
#[tauri::command]
pub fn preview_show(app: AppHandle, key: String, anchor_x: f64) -> Result<(), String> {
    with(|s| {
        s.over_dock = true;
        s.generation += 1;
    });
    let entry = apps::find(&key)?;
    if entry.windows.is_empty() {
        hide(&app);
        return Ok(());
    }
    let dock_win = app.get_webview_window("main").ok_or("no dock window")?;
    let (preview, ph) = preview_hwnd(&app).ok_or("no preview window")?;

    let scale = dock_win.scale_factor().map_err(|e| e.to_string())?;
    let dock_pos = dock_win.outer_position().map_err(|e| e.to_string())?;
    let dock_size = dock_win.outer_size().map_err(|e| e.to_string())?;
    let monitor = dock_win
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("no monitor")?;
    let (mx, mw) = (monitor.position().x, monitor.size().width as i32);

    let n = entry.windows.len() as f64;
    let w = ((n * CARD_W + (n - 1.0) * GAP + 2.0 * PAD) * scale).round() as i32;
    let h = ((CARD_H + 2.0 * PAD) * scale).round() as i32;
    let anchor = dock_pos.x + (anchor_x * scale).round() as i32;
    let x = (anchor - w / 2).clamp(mx + 8, (mx + mw - w - 8).max(mx + 8));
    let y = match dock::current_edge() {
        Edge::Bottom => dock_pos.y - h,
        Edge::Top => dock_pos.y + dock_size.height as i32,
    };

    clear_thumbs();
    with(|s| s.key = Some(key.clone()));
    preview
        .emit_to(
            LABEL,
            "preview-data",
            PreviewData {
                key,
                name: entry.name,
                icon: entry.icon,
                windows: entry.windows,
            },
        )
        .map_err(|e| e.to_string())?;
    unsafe {
        let _ = SetWindowPos(
            ph,
            Some(HWND_TOPMOST),
            x,
            y,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    Ok(())
}

/// The popup reports where each live thumbnail should be drawn (CSS px).
#[tauri::command]
pub fn preview_layout(app: AppHandle, rects: Vec<ThumbRect>) -> Result<(), String> {
    let (preview, ph) = preview_hwnd(&app).ok_or("no preview window")?;
    let scale = preview.scale_factor().map_err(|e| e.to_string())?;
    clear_thumbs();
    let mut ids = Vec::new();
    for r in rects {
        unsafe {
            let Ok(id) = DwmRegisterThumbnail(ph, apps::hwnd(r.hwnd)) else {
                continue;
            };
            // Fit the source into the box, keeping its aspect ratio, centred.
            let (bx, by, bw, bh) = (r.x * scale, r.y * scale, r.w * scale, r.h * scale);
            let (mut dw, mut dh) = (bw, bh);
            if let Ok(src) = DwmQueryThumbnailSourceSize(id) {
                if src.cx > 0 && src.cy > 0 {
                    let k = (bw / src.cx as f64).min(bh / src.cy as f64);
                    dw = src.cx as f64 * k;
                    dh = src.cy as f64 * k;
                }
            }
            let left = (bx + (bw - dw) / 2.0).round() as i32;
            let top = (by + (bh - dh) / 2.0).round() as i32;
            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT {
                    left,
                    top,
                    right: left + dw.round() as i32,
                    bottom: top + dh.round() as i32,
                },
                rcSource: RECT::default(),
                opacity: 255,
                fVisible: true.into(),
                fSourceClientAreaOnly: false.into(),
            };
            let _ = DwmUpdateThumbnailProperties(id, &props);
            ids.push(id);
        }
    }
    with(|s| s.thumbs = ids);
    Ok(())
}

/// The pointer entered or left the dock icon (`source = "dock"`) or the popup.
/// The popup hides shortly after the pointer is over neither.
#[tauri::command]
pub fn preview_hover(app: AppHandle, source: String, inside: bool) {
    let generation = with(|s| {
        if source == "dock" {
            s.over_dock = inside;
        } else {
            s.over_preview = inside;
        }
        s.generation += 1;
        s.generation
    });
    if inside {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(HIDE_DELAY);
        let still_out =
            with(|s| s.generation == generation && !s.over_dock && !s.over_preview);
        if still_out {
            let app2 = app.clone();
            let _ = app.run_on_main_thread(move || hide(&app2));
        }
    });
}

#[tauri::command]
pub fn preview_hide(app: AppHandle) {
    with(|s| {
        s.over_dock = false;
        s.over_preview = false;
        s.generation += 1;
    });
    hide(&app);
}
