//! Liquid glass: the picture behind the dock, for the page to refract.
//!
//! The page asks for a frame; this copies the strip of screen behind the bar
//! (plus a margin, so the rim can bend in light from just outside), scaled down
//! by 2, and returns the raw pixels. If nothing changed since the last frame it
//! returns only a header, so an idle desktop costs a small copy and a hash.
//!
//! While liquid glass is on, the dock window is excluded from screen capture,
//! so it never captures (and refracts) itself. That also hides it from
//! screenshots and screen sharing, so it is only done in this style.

use std::sync::Mutex;

use tauri::ipc::Response;
use tauri::{AppHandle, Manager};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    SetStretchBltMode, StretchBlt, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, COLORONCOLOR, DIB_RGB_COLORS,
    HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, SetWindowDisplayAffinity, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};

/// Physical px of screen around the bar that the rim can pull light from.
const MARGIN: i32 = 24;
/// Frames are captured at 1/SCALE size; the glass blurs them anyway.
const SCALE: i32 = 2;

static LAST_HASH: Mutex<u64> = Mutex::new(0);

/// Exclude (or stop excluding) the dock from screen capture.
pub fn set_excluded(app: &AppHandle, on: bool) {
    let Some(w) = app.get_webview_window("main") else { return };
    let Ok(h) = w.hwnd() else { return };
    let affinity = if on { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    if let Err(e) = unsafe { SetWindowDisplayAffinity(HWND(h.0 as _), affinity) } {
        eprintln!("[glass] display affinity: {e}");
    }
    *LAST_HASH.lock().unwrap() = 0;
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for chunk in bytes.chunks(8) {
        let mut v = [0u8; 8];
        v[..chunk.len()].copy_from_slice(chunk);
        h ^= u64::from_le_bytes(v);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Header (6 × u32, little endian): changed, width, height, bar x, bar y (in
/// frame px), scale. Then width × height BGRA pixels, top row first, if changed.
#[tauri::command]
pub async fn glass_frame(app: AppHandle, force: bool) -> Response {
    let header_only = |changed: u32| {
        let mut v = Vec::with_capacity(24);
        for x in [changed, 0, 0, 0, 0, SCALE as u32] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        Response::new(v)
    };
    let Some(w) = app.get_webview_window("main") else { return header_only(0) };
    let Ok(h) = w.hwnd() else { return header_only(0) };
    let hwnd = HWND(h.0 as _);

    let mut bar = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut bar) }.is_err() {
        return header_only(0);
    }
    // Capture the bar plus a margin, kept on screen.
    let (vx, vy, vw, vh) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    let left = (bar.left - MARGIN).max(vx);
    let top = (bar.top - MARGIN).max(vy);
    let right = (bar.right + MARGIN).min(vx + vw);
    let bottom = (bar.bottom + MARGIN).min(vy + vh);
    let (cw, ch) = (right - left, bottom - top);
    if cw <= 0 || ch <= 0 {
        return header_only(0);
    }
    let (dw, dh) = ((cw / SCALE).max(1), (ch / SCALE).max(1));

    let pixels = unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: dw,
                biHeight: -dh, // top row first
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(Some(mem), &bi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            return header_only(0);
        };
        let old = SelectObject(mem, HGDIOBJ(dib.0));
        // Plain sampling is several times faster than HALFTONE; the shader blurs anyway.
        SetStretchBltMode(mem, COLORONCOLOR);
        let _ = StretchBlt(mem, 0, 0, dw, dh, Some(screen), left, top, cw, ch, SRCCOPY);
        let len = (dw * dh * 4) as usize;
        let copy = std::slice::from_raw_parts(bits as *const u8, len).to_vec();
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        copy
    };

    let hash = fnv(&pixels);
    {
        let mut last = LAST_HASH.lock().unwrap();
        if !force && *last == hash {
            return header_only(0);
        }
        *last = hash;
    }

    let mut out = Vec::with_capacity(24 + pixels.len());
    for x in [
        1u32,
        dw as u32,
        dh as u32,
        ((bar.left - left) / SCALE) as u32,
        ((bar.top - top) / SCALE) as u32,
        SCALE as u32,
    ] {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.extend_from_slice(&pixels);
    Response::new(out)
}
