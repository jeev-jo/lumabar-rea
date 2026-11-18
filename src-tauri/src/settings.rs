//! User settings, stored as JSON in `%LOCALAPPDATA%\luma_ai_bar\settings.json`.
//! Every change is broadcast to all windows as a `settings-changed` event.

use std::ffi::c_void;
use std::fs;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{w, BOOL, HSTRING};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;

use crate::apps::{self, Pin};
use crate::widgets::WidgetInstance;
use crate::dock::{self, Edge};
use crate::taskbar;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskbarMode {
    /// Auto-hide the Windows taskbar while the dock runs.
    Hide,
    /// Hide it outright (falls back to auto-hide if Windows will not allow it).
    Complete,
    /// Leave the Windows taskbar alone.
    Leave,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl Size {
    /// Height of the reserved strip in logical px. Keep in sync with `--strip` in theme.css.
    pub fn strip_height(self) -> i32 {
        match self {
            Size::Small => 52,
            Size::Medium => 60,
            Size::Large => 72,
        }
    }

    /// Height of the bar (the dock window) in logical px. Keep in sync with `--bar`:
    /// tile height (`--btn`) + 4 px inset above and below + the 1 px border.
    pub fn bar_height(self) -> i32 {
        match self {
            Size::Small => 46,
            Size::Medium => 52,
            Size::Large => 62,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// Opaque surfaces.
    Solid,
    /// Frosted glass: Windows blurs what is behind each surface.
    Glass,
    /// Liquid glass: the dock bends and refracts what is behind it (see glass.rs).
    Liquid,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    System,
    Dark,
    Light,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub edge: Edge,
    pub taskbar_mode: TaskbarMode,
    pub size: Size,
    pub theme: Theme,
    pub style: Style,
    /// Device name of the display the dock sits on (`\\.\DISPLAY2`); `None` = primary.
    pub display: Option<String>,
    /// `None` until first run, when it is seeded from the Windows taskbar's pins.
    pub pins: Option<Vec<Pin>>,
    /// Widgets in the dock, in order.
    pub widgets: Vec<WidgetInstance>,
    /// Show a widget's details when the pointer rests on it.
    pub glance: bool,
    /// The thin Show desktop button at the far end, as on the Windows taskbar.
    pub peek_button: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            edge: Edge::Bottom,
            taskbar_mode: TaskbarMode::Hide,
            size: Size::Medium,
            theme: Theme::System,
            style: Style::Glass,
            display: None,
            pins: None,
            widgets: Vec::new(),
            glance: true,
            peek_button: true,
        }
    }
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
    taskbar::data_dir().join("settings.json")
}

pub fn get() -> Settings {
    let mut guard = SETTINGS.lock().unwrap();
    guard
        .get_or_insert_with(|| {
            fs::read_to_string(path())
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default()
        })
        .clone()
}

pub fn update(f: impl FnOnce(&mut Settings)) -> Settings {
    let mut s = get();
    f(&mut s);
    *SETTINGS.lock().unwrap() = Some(s.clone());
    let write = fs::create_dir_all(taskbar::data_dir())
        .and_then(|_| fs::write(path(), serde_json::to_string_pretty(&s).unwrap()));
    if let Err(e) = write {
        eprintln!("[settings] could not save: {e}");
    }
    s
}

pub fn apply_taskbar_mode(mode: TaskbarMode) {
    match mode {
        TaskbarMode::Hide => taskbar::hide(),
        TaskbarMode::Complete => taskbar::hide_completely(),
        TaskbarMode::Leave => taskbar::restore(),
    }
}

/// Give every window the backdrop for `style`.
/// The dock and its popups never take focus, so they get the acrylic blur from
/// `backdrop` (Windows 11's own Acrylic switches off while a window is inactive).
/// Settings and the picker are ordinary windows and get Mica.
pub fn apply_style(app: &AppHandle, style: Style) {
    use tauri::window::{Effect, EffectsBuilder};
    let glass = style != Style::Solid;
    for label in ["main", "preview", "popover"] {
        let Some(w) = app.get_webview_window(label) else { continue };
        let Ok(h) = w.hwnd() else { continue };
        // In liquid style the dock draws its own refracted backdrop instead.
        let frosted = glass && !(label == "main" && style == Style::Liquid);
        if !crate::backdrop::set_glass(windows::Win32::Foundation::HWND(h.0 as _), frosted) && frosted {
            eprintln!("[style] {label}: frosted backdrop unavailable on this Windows");
        }
    }
    crate::glass::set_excluded(app, style == Style::Liquid);
    for label in ["settings", "widgets"] {
        let Some(w) = app.get_webview_window(label) else { continue };
        let result = if glass {
            w.set_effects(EffectsBuilder::new().effect(Effect::Mica).build())
        } else {
            w.set_effects(None)
        };
        if let Err(e) = result {
            eprintln!("[style] {label}: {e}");
        }
    }
}

fn broadcast(app: &AppHandle, s: &Settings) {
    let _ = app.emit("settings-changed", s);
}

#[tauri::command]
pub fn get_settings() -> Settings {
    get()
}

/// Fields the UI may change; anything left out stays as it is.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Patch {
    edge: Option<Edge>,
    taskbar_mode: Option<TaskbarMode>,
    size: Option<Size>,
    theme: Option<Theme>,
    style: Option<Style>,
    glance: Option<bool>,
    peek_button: Option<bool>,
    /// `Some("")` means "primary display".
    display: Option<String>,
}

#[tauri::command]
pub fn update_settings(app: AppHandle, patch: Patch) -> Settings {
    let before = get();
    let s = update(|s| {
        if let Some(v) = patch.edge {
            s.edge = v;
        }
        if let Some(v) = patch.taskbar_mode {
            s.taskbar_mode = v;
        }
        if let Some(v) = patch.size {
            s.size = v;
        }
        if let Some(v) = patch.theme {
            s.theme = v;
        }
        if let Some(v) = patch.style {
            s.style = v;
        }
        if let Some(v) = patch.glance {
            s.glance = v;
        }
        if let Some(v) = patch.peek_button {
            s.peek_button = v;
        }
        if let Some(v) = patch.display {
            s.display = (!v.is_empty()).then_some(v);
        }
    });
    if s.taskbar_mode != before.taskbar_mode {
        apply_taskbar_mode(s.taskbar_mode);
    }
    if s.style != before.style {
        apply_style(&app, s.style);
    }
    if s.edge != before.edge || s.size != before.size || s.display != before.display {
        dock::set_edge(s.edge);
        let _ = app.run_on_main_thread(dock::reposition);
    }
    broadcast(&app, &s);
    s
}

/// Forget the dock's pins and import the Windows taskbar's pins again.
#[tauri::command]
pub fn reset_pins(app: AppHandle) {
    let s = update(|s| s.pins = None);
    apps::notify();
    broadcast(&app, &s);
}

// ---------------------------------------------------------------------------
// Displays
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct Display {
    /// `\\.\DISPLAY1`
    pub id: String,
    pub label: String,
    pub primary: bool,
    /// Monitor rectangle in physical px.
    #[serde(skip)]
    pub rect: RECT,
}

unsafe extern "system" fn collect_monitor(m: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
    let out = unsafe { &mut *(l.0 as *mut Vec<HMONITOR>) };
    out.push(m);
    BOOL(1)
}

pub fn displays() -> Vec<Display> {
    let mut monitors: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
        .into_iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let ok = unsafe { GetMonitorInfoW(m, &mut info as *mut _ as *mut MONITORINFO) };
            if !ok.as_bool() {
                return None;
            }
            let end = info.szDevice.iter().position(|&c| c == 0).unwrap_or(32);
            let rect = info.monitorInfo.rcMonitor;
            let primary = info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0;
            Some(Display {
                id: String::from_utf16_lossy(&info.szDevice[..end]),
                label: format!(
                    "Display {}{} — {}×{}",
                    i + 1,
                    if primary { " (main)" } else { "" },
                    rect.right - rect.left,
                    rect.bottom - rect.top
                ),
                primary,
                rect,
            })
        })
        .collect()
}

#[tauri::command]
pub fn list_displays() -> Vec<Display> {
    displays()
}

// ---------------------------------------------------------------------------
// Start with Windows (HKCU Run key)
// ---------------------------------------------------------------------------

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: windows::core::PCWSTR = w!("luma-ai-bar");

#[tauri::command]
pub fn get_autostart() -> bool {
    let mut size = 0u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    r.is_ok() && size > 0
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<bool, String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command: Vec<u16> = format!("\"{}\"", exe.display())
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let r = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(command.as_ptr() as *const c_void),
                (command.len() * 2) as u32,
            )
        };
        r.ok().map_err(|e| e.to_string())?;
    } else {
        let r = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) };
        // Already absent is fine.
        if r.is_err() && r.0 != 2 {
            return Err(HSTRING::from(format!("registry error {}", r.0)).to_string());
        }
    }
    Ok(get_autostart())
}

/// Keep "Start with Windows" pointing at this exe, so it still works after
/// an update or a reinstall to another folder. Dev builds leave it alone.
pub fn refresh_autostart() {
    if cfg!(not(debug_assertions)) && get_autostart() {
        let _ = set_autostart(true);
    }
}

/// Called just before an update installs: the installer closes the app
/// without the usual exit path, so put the taskbar back first.
#[tauri::command]
pub fn prepare_update() {
    crate::taskbar::restore();
}

// ---------------------------------------------------------------------------
// Settings window
// ---------------------------------------------------------------------------

pub fn open_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    open_window(&app);
}
