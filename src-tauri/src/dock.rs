//! The dock window: a borderless strip on the top or bottom screen edge,
//! registered with Windows as an AppBar so maximized windows stop at its edge.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, WebviewWindow};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{
    DefSubclassProc, SHAppBarMessage, SetWindowSubclass, ABE_BOTTOM, ABE_TOP, ABM_NEW,
    ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, ABN_FULLSCREENAPP, ABN_POSCHANGED, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, RegisterWindowMessageW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
    HWND_BOTTOM, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use crate::{settings, taskbar};

const SUBCLASS_ID: usize = 0x4C41_4221;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
}

#[derive(Clone, Copy)]
struct DockState {
    hwnd: isize,
    edge: Edge,
    callback_msg: u32,
    /// Broadcast by Explorer when it (re)creates the taskbar, e.g. after a crash.
    taskbar_created_msg: u32,
    registered: bool,
}

static STATE: Mutex<DockState> = Mutex::new(DockState {
    hwnd: 0,
    edge: Edge::Bottom,
    callback_msg: 0,
    taskbar_created_msg: 0,
    registered: false,
});

fn snapshot() -> DockState {
    *STATE.lock().unwrap()
}

fn appbar_data(state: &DockState) -> APPBARDATA {
    APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        hWnd: HWND(state.hwnd as _),
        uCallbackMessage: state.callback_msg,
        ..Default::default()
    }
}

/// Called once from Tauri's setup hook, on the main thread.
pub fn init(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = HWND(window.hwnd()?.0 as _);
    make_tool_window(hwnd);
    unsafe {
        let callback_msg = RegisterWindowMessageW(w!("LumaAiBar.AppBarCallback"));
        let taskbar_created_msg = RegisterWindowMessageW(w!("TaskbarCreated"));
        {
            let mut s = STATE.lock().unwrap();
            s.hwnd = hwnd.0 as isize;
            s.callback_msg = callback_msg;
            s.taskbar_created_msg = taskbar_created_msg;
            s.edge = settings::get().edge;
        }
        register();

        let _ = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0);
    }
    apply_position();
    Ok(())
}

/// Keep a window out of Alt-Tab and stop clicks on it from stealing focus.
pub fn make_tool_window(hwnd: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize,
        );
    }
}

pub fn current_edge() -> Edge {
    snapshot().edge
}

/// Register with the shell as an AppBar (again, after an Explorer restart).
fn register() {
    let mut s = STATE.lock().unwrap();
    let mut abd = appbar_data(&s);
    let ok = unsafe { SHAppBarMessage(ABM_NEW, &mut abd) } != 0;
    s.registered = ok;
    if !ok {
        log("[dock] ABM_NEW failed - the dock will not reserve screen space");
    }
}

/// Ask the shell for our strip on the current edge, then move the window into it.
fn apply_position() {
    let state = snapshot();
    if state.hwnd == 0 {
        return;
    }
    let hwnd = HWND(state.hwnd as _);
    unsafe {
        let prefs = settings::get();
        let displays = settings::displays();
        // The chosen display if it is still connected, else the main one.
        let Some(display) = prefs
            .display
            .as_ref()
            .and_then(|id| displays.iter().find(|d| &d.id == id))
            .or_else(|| displays.iter().find(|d| d.primary))
            .or(displays.first())
        else {
            log("[dock] no display found");
            return;
        };
        // A move to a display with another scale sends WM_DPICHANGED, which re-runs this.
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let height = prefs.size.strip_height() * dpi / 96;

        let mut rc = display.rect;
        match state.edge {
            Edge::Bottom => rc.top = rc.bottom - height,
            Edge::Top => rc.bottom = rc.top + height,
        }

        if state.registered {
            let mut abd = appbar_data(&state);
            abd.uEdge = match state.edge {
                Edge::Bottom => ABE_BOTTOM,
                Edge::Top => ABE_TOP,
            };
            abd.rc = rc;
            // The shell may move the rect to clear other app bars; keep our height.
            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            match state.edge {
                Edge::Bottom => abd.rc.top = abd.rc.bottom - height,
                Edge::Top => abd.rc.bottom = abd.rc.top + height,
            }
            SHAppBarMessage(ABM_SETPOS, &mut abd);
            rc = abd.rc;
        }

        let RECT { left, top, right, bottom } = rc;
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            left,
            top,
            right - left,
            bottom - top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let state = snapshot();
    if state.callback_msg != 0 && msg == state.callback_msg {
        match wparam.0 as u32 {
            ABN_POSCHANGED => apply_position(),
            ABN_FULLSCREENAPP => {
                // A fullscreen app covers our display: drop behind it; come back after.
                let after = if lparam.0 != 0 { HWND_BOTTOM } else { HWND_TOPMOST };
                let _ = unsafe {
                    SetWindowPos(
                        hwnd,
                        Some(after),
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    )
                };
            }
            _ => {}
        }
        return LRESULT(0);
    }
    if state.taskbar_created_msg != 0 && msg == state.taskbar_created_msg {
        // Explorer restarted and forgot every AppBar: register and place again.
        log("[dock] taskbar recreated - re-registering");
        taskbar::on_taskbar_recreated();
        STATE.lock().unwrap().registered = false;
        register();
        apply_position();
    }
    if msg == WM_ENDSESSION && wparam.0 != 0 {
        // Logoff or shutdown: the process may be ended without an Exit event.
        taskbar::restore();
    }
    let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED {
        apply_position();
    }
    result
}

/// Give the reserved screen space back. Called on exit.
pub fn shutdown() {
    let mut s = STATE.lock().unwrap();
    if s.registered {
        let mut abd = appbar_data(&s);
        unsafe { SHAppBarMessage(ABM_REMOVE, &mut abd) };
        s.registered = false;
    }
}

fn log(msg: &str) {
    eprintln!("{msg}");
}

pub fn set_edge(edge: Edge) {
    STATE.lock().unwrap().edge = edge;
}

/// Re-place the dock after a settings change. Must run on the main thread.
pub fn reposition() {
    apply_position();
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
