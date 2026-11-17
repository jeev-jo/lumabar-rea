//! Frosted-glass backdrop for the dock and its popups.
//!
//! Windows 11's documented backdrops (Mica, the "transient" Acrylic) switch off
//! while a window is inactive, and the dock never becomes active. The older
//! accent-policy call keeps an acrylic blur on regardless, so it is used here.
//! It is undocumented: it is looked up at run time, and if it is missing the
//! window simply stays without blur (the page's tint still shows).

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{s, w, BOOL};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_DISABLED: u32 = 0;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    /// AABBGGRR tint under the blur. Kept nearly clear: the page adds its own tint.
    gradient: u32,
    animation: u32,
}

#[repr(C)]
struct CompositionData {
    attribute: u32,
    data: *mut c_void,
    size: usize,
}

type SetWindowCompositionAttribute = unsafe extern "system" fn(HWND, *mut CompositionData) -> BOOL;

fn swca() -> Option<SetWindowCompositionAttribute> {
    unsafe {
        let user32 = GetModuleHandleW(w!("user32.dll")).ok()?;
        let f = GetProcAddress(user32, s!("SetWindowCompositionAttribute"))?;
        Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, SetWindowCompositionAttribute>(f))
    }
}

/// Turn the frosted backdrop on or off for `hwnd`. Returns false if Windows
/// doesn't offer the call.
pub fn set_glass(hwnd: HWND, on: bool) -> bool {
    let Some(f) = swca() else { return false };
    let mut policy = AccentPolicy {
        state: if on { ACCENT_ENABLE_ACRYLICBLURBEHIND } else { ACCENT_DISABLED },
        flags: 0,
        gradient: 0x0100_0000,
        animation: 0,
    };
    let mut data = CompositionData {
        attribute: WCA_ACCENT_POLICY,
        data: &mut policy as *mut _ as *mut c_void,
        size: size_of::<AccentPolicy>(),
    };
    unsafe { f(hwnd, &mut data).as_bool() }
}
