//! Pinned apps and open windows: the taskbar's core job.
//!
//! Windows are listed with the same rules Alt-Tab uses, grouped per app by the
//! process's exe path (or, for web apps installed from Chrome or Edge, by the
//! app ID the browser gives their windows), and merged with the user's pins.
//! A WinEvent hook and the shell hook push live updates to the UI as
//! `apps-changed` events.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, WebviewWindow};
use windows::core::{w, Interface, BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, PROPERTYKEY, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    STGM_READ,
};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
use windows::Win32::UI::Shell::{
    DefSubclassProc, IShellItemImageFactory, IShellLinkW, SHCreateItemFromParsingName,
    SetWindowSubclass, ShellExecuteW, ShellLink, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    SLGP_UNCPRIORITY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumChildWindows, EnumWindows, GetAncestor,
    GetClassNameW, GetDesktopWindow, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    PostMessageW, RegisterShellHookWindow, RegisterWindowMessageW, SetForegroundWindow,
    ShowWindow, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_HIDE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, GA_PARENT,
    GWL_EXSTYLE, GW_OWNER, OBJID_WINDOW, SW_MINIMIZE, SW_RESTORE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL,
    WINEVENT_OUTOFCONTEXT, WM_CLOSE, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

use crate::{settings, taskbar};

const SUBCLASS_ID: usize = 0x4C41_4222;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
/// `HSHELL_FLASH` = `HSHELL_REDRAW | HSHELL_HIGHBIT`: a window is asking for attention.
const HSHELL_FLASH: usize = 0x8006;
const ICON_SIZE: i32 = 64;

// ---------------------------------------------------------------------------
// Data sent to the UI
// ---------------------------------------------------------------------------

/// A pinned app: a shortcut (`.lnk`) or an exe.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pin {
    pub path: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct WindowInfo {
    pub hwnd: i64,
    pub title: String,
    pub minimized: bool,
    pub attention: bool,
    /// Position in the desktop's z-order (0 = topmost). Not shown; used for activation.
    #[serde(skip)]
    z: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AppEntry {
    /// Grouping key: the lower-cased exe path (or the shortcut path if it has no exe).
    pub key: String,
    pub name: String,
    pub pinned: bool,
    /// What to open to start the app (shortcut or exe).
    pub launch: String,
    /// What to ask the shell for an icon (shortcut or exe).
    pub icon: String,
    pub windows: Vec<WindowInfo>,
    /// The app's taskbar badge: a count ("3", "9+") or "•" for a status glyph.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    pub active: bool,
    pub attention: bool,
}

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

static NOTIFY: OnceLock<mpsc::Sender<()>> = OnceLock::new();
static ATTENTION: Mutex<Option<HashSet<isize>>> = Mutex::new(None);
/// Every window shown in the last snapshot, so destroy/rename events for
/// unrelated windows can be ignored cheaply.
static KNOWN: Mutex<Option<HashSet<isize>>> = Mutex::new(None);
/// Running-but-unpinned apps, in the order they first appeared.
static ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static PIN_KEYS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
static ICONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
static SHELLHOOK_MSG: AtomicU32 = AtomicU32::new(0);

fn with_set<R>(m: &Mutex<Option<HashSet<isize>>>, f: impl FnOnce(&mut HashSet<isize>) -> R) -> R {
    f(m.lock().unwrap().get_or_insert_with(HashSet::new))
}

fn with_map<V, R>(
    m: &Mutex<Option<HashMap<String, V>>>,
    f: impl FnOnce(&mut HashMap<String, V>) -> R,
) -> R {
    f(m.lock().unwrap().get_or_insert_with(HashMap::new))
}

/// Ask the worker to rebuild the snapshot (debounced).
pub fn notify() {
    if let Some(tx) = NOTIFY.get() {
        let _ = tx.send(());
    }
}

fn com_init() {
    // Already-initialised threads return S_FALSE or RPC_E_CHANGED_MODE; both are fine.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

struct Win {
    hwnd: isize,
    title: String,
    key: String,
    exe: String,
    minimized: bool,
    z: usize,
    /// The app ID the window was given, if any (for taskbar badges).
    app_id: Option<String>,
}

unsafe extern "system" fn collect_hwnd(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    out.push(hwnd);
    BOOL(1)
}

pub(crate) fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

pub(crate) fn process_exe(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Would the Windows taskbar show a button for this window? (The Alt-Tab rules.)
fn is_taskbar_window(hwnd: HWND, own_pid: u32) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_APPWINDOW.0 == 0 {
            if ex & (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) != 0 {
                return false;
            }
            if matches!(GetWindow(hwnd, GW_OWNER), Ok(owner) if !owner.is_invalid()) {
                return false;
            }
        }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            4,
        )
        .is_ok()
            && cloaked != 0
        {
            return false;
        }
        if GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        if matches!(
            class_name(hwnd).as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        ) {
            return false;
        }
        window_pid(hwnd) != own_pid
    }
}

/// Store apps live in ApplicationFrameHost; the real app is a child window's process.
fn store_app_exe(frame: HWND, frame_pid: u32) -> Option<String> {
    let mut children: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumChildWindows(
            Some(frame),
            Some(collect_hwnd),
            LPARAM(&mut children as *mut _ as isize),
        );
    }
    children
        .into_iter()
        .map(window_pid)
        .find(|&pid| pid != frame_pid)
        .and_then(process_exe)
}

fn collect_windows() -> Vec<Win> {
    let own_pid = std::process::id();
    let mut all: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_hwnd), LPARAM(&mut all as *mut _ as isize));
    }
    all.into_iter()
        .enumerate()
        .filter(|(_, h)| is_taskbar_window(*h, own_pid))
        .filter_map(|(z, hwnd)| {
            let pid = window_pid(hwnd);
            let title = window_title(hwnd);
            let mut exe = process_exe(pid)?;
            let mut key = exe.to_lowercase();
            let app_id = window_prop(hwnd, AUMID);
            if let Some(id) = app_id.as_ref().filter(|id| is_web_app(id)) {
                key = format!("{WEB}{}", id.to_lowercase());
            } else if key.ends_with("\\applicationframehost.exe") {
                match store_app_exe(hwnd, pid) {
                    Some(real) => {
                        key = real.to_lowercase();
                        exe = real;
                    }
                    // Suspended store app: no child to ask, group it on its own.
                    None => key = format!("store:{title}"),
                }
            }
            if !key.starts_with(WEB) && !key.starts_with("store:") {
                if let Some(id) = package_app_id(&exe) {
                    key = format!("{PKG}{}", id.to_lowercase());
                }
            }
            Some(Win {
                hwnd: hwnd.0 as isize,
                minimized: unsafe { IsIconic(hwnd).as_bool() },
                title,
                key,
                exe,
                z,
                app_id,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Web apps (installed from Chrome, Edge and other Chromium browsers)
// ---------------------------------------------------------------------------

/// Key prefix for a web app's windows; the rest is its lower-cased app ID.
const WEB: &str = "web:";
const PKEY_APPUSERMODEL: windows::core::GUID =
    windows::core::GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3);
/// System.AppUserModel.ID.
const AUMID: u32 = 5;
/// System.AppUserModel.RelaunchIconResource.
const RELAUNCH_ICON: u32 = 3;

/// Chromium names each installed web app's windows and shortcuts
/// "<Browser>._crx_<app>[.<profile>]", so the taskbar can tell them apart.
fn is_web_app(id: &str) -> bool {
    id.contains("._crx_")
}

fn prop_string(store: &IPropertyStore, pid: u32) -> Option<String> {
    let key = PROPERTYKEY { fmtid: PKEY_APPUSERMODEL, pid };
    unsafe {
        let value = store.GetValue(&key).ok()?;
        let text = PropVariantToStringAlloc(&value).ok()?;
        let out = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const c_void));
        out.filter(|t| !t.is_empty())
    }
}

/// An app-model property a window was given by its app (none for most windows).
fn window_prop(hwnd: HWND, pid: u32) -> Option<String> {
    com_init();
    let store: IPropertyStore = unsafe { SHGetPropertyStoreForWindow(hwnd) }.ok()?;
    prop_string(&store, pid)
}

/// The app ID stored in a shortcut, if any.
fn lnk_app_id(path: &str) -> Option<String> {
    com_init();
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        link.cast::<IPersistFile>().ok()?.Load(&HSTRING::from(path), STGM_READ).ok()?;
        prop_string(&link.cast::<IPropertyStore>().ok()?, AUMID)
    }
}

/// Web-app shortcuts the browsers made (Start menu and desktop), by app ID.
static WEB_LINKS: Mutex<Option<(Instant, HashMap<String, String>)>> = Mutex::new(None);

fn scan_links(dir: &Path, depth: u8, out: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            if depth > 0 {
                scan_links(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            let p = path.to_string_lossy().into_owned();
            if let Some(id) = lnk_app_id(&p).filter(|id| is_web_app(id)) {
                out.entry(id.to_lowercase()).or_insert(p);
            }
        }
    }
}

/// The shortcut for a web app, which gives the dock its name, icon and a way
/// to start it. Rescanned at most every 30 seconds while an app isn't known.
fn web_shortcut(key: &str) -> Option<String> {
    let id = key.strip_prefix(WEB)?;
    let mut guard = WEB_LINKS.lock().unwrap();
    let stale = match guard.as_ref() {
        Some((at, m)) => !m.contains_key(id) && at.elapsed() > Duration::from_secs(30),
        None => true,
    };
    if stale {
        let mut m = HashMap::new();
        let menu = r"Microsoft\Windows\Start Menu\Programs";
        for (var, sub) in [("APPDATA", menu), ("ProgramData", menu), ("USERPROFILE", "Desktop")] {
            if let Some(base) = std::env::var_os(var) {
                scan_links(&Path::new(&base).join(sub), 3, &mut m);
            }
        }
        *guard = Some((Instant::now(), m));
    }
    guard.as_ref()?.1.get(id).cloned()
}

/// Name, launch target and icon for a running web app's dock button.
fn web_app_entry(key: &str, first: &Win) -> (String, String, String) {
    if let Some(lnk) = web_shortcut(key) {
        let name = Path::new(&lnk)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        return (name, lnk.clone(), lnk);
    }
    // No shortcut (it was deleted): use the icon the browser gave the window.
    let icon = window_prop(HWND(first.hwnd as *mut c_void), RELAUNCH_ICON)
        .map(|r| match r.rsplit_once(',') {
            Some((p, n)) if n.trim().parse::<i32>().is_ok() => p.to_string(),
            _ => r,
        })
        .filter(|p| Path::new(p).is_file())
        .unwrap_or_else(|| first.exe.clone());
    (first.title.clone(), first.exe.clone(), icon)
}

// ---------------------------------------------------------------------------
// Store / MSIX apps (installed under Program Files\WindowsApps)
// ---------------------------------------------------------------------------

/// Key prefix for a packaged app; the rest is its lower-cased app ID
/// ("claude_pzs8sxrjxfjjc!claude"), which stays the same across updates.
const PKG: &str = "pkg:";
/// The shell's name for an installed app, which it can draw and open.
const APPS_FOLDER: &str = "shell:AppsFolder\\";

static PKG_IDS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

/// The app ID of a packaged app's exe ("Claude_pzs8sxrjxfjjc!Claude"), read
/// from its package folder name and manifest. The exe itself can't be read
/// for an icon, but the shell draws the package's logo from this ID.
fn package_app_id(exe: &str) -> Option<String> {
    let lower = exe.to_lowercase();
    let at = lower.find("\\windowsapps\\")? + "\\windowsapps\\".len();
    if let Some(hit) = with_map(&PKG_IDS, |m| m.get(&lower).cloned()) {
        return hit;
    }
    let found = (|| {
        let rest = &exe[at..];
        let (folder, inner) = rest.split_once('\\')?;
        // "<Name>_<Version>_<Arch>_<Resource>_<PublisherId>"; names have no "_".
        let (head, publisher) = folder.rsplit_once("__").or_else(|| folder.rsplit_once('_'))?;
        let name = head.split('_').next()?;
        let manifest = std::fs::read_to_string(Path::new(&exe[..at]).join(folder).join("AppxManifest.xml")).ok()?;
        let attr = |tag: &str, key: &str| -> Option<String> {
            let start = tag.find(&format!(" {key}=\""))? + key.len() + 3;
            Some(tag[start..].split('"').next()?.to_string())
        };
        let apps: Vec<(String, String)> = manifest
            .split("<Application ")
            .skip(1)
            .filter_map(|t| {
                let tag = format!(" {}", t.split('>').next()?);
                Some((attr(&tag, "Id")?, attr(&tag, "Executable").unwrap_or_default()))
            })
            .collect();
        let id = apps
            .iter()
            .find(|(_, e)| e.eq_ignore_ascii_case(inner))
            .or_else(|| apps.first())
            .map(|(id, _)| id.clone())?;
        Some(format!("{name}_{publisher}!{id}"))
    })();
    with_map(&PKG_IDS, |m| m.insert(lower, found.clone()));
    found
}

// ---------------------------------------------------------------------------
// Taskbar badges (the count Teams, Outlook and others put on their icon)
// ---------------------------------------------------------------------------

/// Current badges and waiting notifications, by lower-cased app ID.
static BADGES: Mutex<Option<crate::extras::NotificationCounts>> = Mutex::new(None);
/// App IDs of desktop apps, from their Start menu shortcuts: exe path -> IDs.
static EXE_IDS: Mutex<Option<HashMap<String, Vec<String>>>> = Mutex::new(None);

/// Desktop apps' Start menu shortcuts often carry the app ID they send
/// notifications under (Slack, classic Outlook...), which their windows don't.
fn scan_exe_ids(dir: &Path, depth: u8, out: &mut HashMap<String, Vec<String>>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            if depth > 0 {
                scan_exe_ids(&path, depth - 1, out);
            }
        } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
            let p = path.to_string_lossy().into_owned();
            if let (Some(id), Some(target)) = (lnk_app_id(&p), lnk_target(&p)) {
                let ids = out.entry(target.to_lowercase()).or_default();
                let id = id.to_lowercase();
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
}

/// Apps set badges through Windows, which keeps them with its notifications.
/// Re-read them whenever Windows changes that store (checked every 3 seconds),
/// and the Start menu's app IDs every 10 minutes.
fn watch_badges() {
    std::thread::spawn(|| {
        com_init();
        let mut stamp = None;
        let mut scanned: Option<Instant> = None;
        loop {
            if scanned.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) {
                let mut m = HashMap::new();
                let menu = r"Microsoft\Windows\Start Menu\Programs";
                for var in ["APPDATA", "ProgramData"] {
                    if let Some(base) = std::env::var_os(var) {
                        scan_exe_ids(&Path::new(&base).join(menu), 3, &mut m);
                    }
                }
                *EXE_IDS.lock().unwrap() = Some(m);
                scanned = Some(Instant::now());
                stamp = None;
            }
            let now = crate::extras::notification_store_stamp();
            if now != stamp {
                stamp = now;
                if let Ok(counts) = crate::extras::read_notification_counts() {
                    let changed = {
                        let mut b = BADGES.lock().unwrap();
                        let changed = b.as_ref() != Some(&counts);
                        *b = Some(counts);
                        changed
                    };
                    if changed {
                        notify();
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
    });
}

// ---------------------------------------------------------------------------
// Pins
// ---------------------------------------------------------------------------

/// The shortcuts pinned to the Windows taskbar, used to seed our pins on first run.
fn windows_taskbar_pins() -> Vec<Pin> {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return Vec::new();
    };
    let dir = Path::new(&appdata)
        .join("Microsoft\\Internet Explorer\\Quick Launch\\User Pinned\\TaskBar");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut pins: Vec<Pin> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .map(|p| Pin {
            name: p.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
            path: p.to_string_lossy().into_owned(),
        })
        .collect();
    pins.sort_by_key(|p| p.name.to_lowercase());
    // The folder often holds stale copies ("Google Chrome (2).lnk"); keep one per app.
    let mut seen = HashSet::new();
    pins.retain(|p| seen.insert(pin_key(p)));
    pins
}

fn pins() -> Vec<Pin> {
    if let Some(p) = settings::get().pins {
        return p;
    }
    let seeded = windows_taskbar_pins();
    settings::update(|s| s.pins = Some(seeded.clone()));
    seeded
}

fn lnk_target(path: &str) -> Option<String> {
    com_init();
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(&HSTRING::from(path), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), SLGP_UNCPRIORITY.0 as u32)
            .ok()?;
        let target = from_wide(&buf);
        (!target.is_empty()).then_some(target)
    }
}

/// The grouping key a pin's windows will have.
fn pin_key(pin: &Pin) -> String {
    if let Some(k) = with_map(&PIN_KEYS, |m| m.get(&pin.path).cloned()) {
        return k;
    }
    // Store apps are keyed by app ID, whichever way they were pinned.
    let pkg = |exe: &str| package_app_id(exe).map(|id| format!("{PKG}{}", id.to_lowercase()));
    let lower = pin.path.to_lowercase();
    let key = if let Some(id) = lower.strip_prefix(&APPS_FOLDER.to_lowercase()).filter(|id| id.contains('!')) {
        format!("{PKG}{id}")
    } else if lower.ends_with(".exe") {
        pkg(&pin.path).unwrap_or(lower)
    } else if let Some(id) = lnk_app_id(&pin.path).filter(|id| is_web_app(id)) {
        format!("{WEB}{}", id.to_lowercase())
    } else if let Some(target) = lnk_target(&pin.path) {
        pkg(&target).unwrap_or_else(|| target.to_lowercase())
    } else if pin.name.to_lowercase().starts_with("file explorer") {
        // The File Explorer pin points at a shell folder, not an exe.
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        format!("{windir}\\explorer.exe").to_lowercase()
    } else {
        pin.path.to_lowercase()
    };
    with_map(&PIN_KEYS, |m| m.insert(pin.path.clone(), key.clone()));
    key
}

// ---------------------------------------------------------------------------
// Names and icons
// ---------------------------------------------------------------------------

fn file_description(path: &str) -> Option<String> {
    unsafe {
        let hpath = HSTRING::from(path);
        let size = GetFileVersionInfoSizeW(&hpath, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&hpath, None, size, data.as_mut_ptr() as *mut c_void).ok()?;
        let mut ptr: *mut c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if !VerQueryValueW(
            data.as_ptr() as *const c_void,
            w!("\\VarFileInfo\\Translation"),
            &mut ptr,
            &mut len,
        )
        .as_bool()
            || len < 4
        {
            return None;
        }
        let lang = *(ptr as *const u16);
        let codepage = *(ptr as *const u16).add(1);
        let query = HSTRING::from(format!(
            "\\StringFileInfo\\{lang:04x}{codepage:04x}\\FileDescription"
        ));
        if !VerQueryValueW(data.as_ptr() as *const c_void, &query, &mut ptr, &mut len).as_bool()
            || len == 0
        {
            return None;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(
            ptr as *const u16,
            len as usize,
        ));
        let text = text.trim_end_matches('\0').trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

pub(crate) fn app_name(key: &str, exe: &str, title: &str) -> String {
    if key.starts_with("store:") {
        return title.to_string();
    }
    if let Some(n) = with_map(&NAMES, |m| m.get(key).cloned()) {
        return n;
    }
    let name = file_description(exe).unwrap_or_else(|| {
        Path::new(exe)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| exe.to_string())
    });
    with_map(&NAMES, |m| m.insert(key.to_string(), name.clone()));
    name
}

/// Render the shell's icon for a file as a PNG data URL.
fn icon_data_url(path: &str) -> Option<String> {
    image_data_url(path, ICON_SIZE, true)
}

/// The shell's picture of a file as a PNG data URL: its icon, or (for images)
/// a thumbnail of the picture itself.
pub(crate) fn image_data_url(path: &str, size: i32, icon_only: bool) -> Option<String> {
    com_init();
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let flags = if icon_only { SIIGBF_ICONONLY.0 | SIIGBF_BIGGERSIZEOK.0 } else { SIIGBF_BIGGERSIZEOK.0 };
        let hbmp = factory
            .GetImage(SIZE { cx: size, cy: size }, SIIGBF(flags))
            .ok()?;
        let hgdi = HGDIOBJ(hbmp.0);
        let mut bm = BITMAP::default();
        GetObjectW(
            hgdi,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut c_void),
        );
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w.max(0) * h.max(0) * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(
            dc,
            hbmp,
            0,
            h as u32,
            Some(px.as_mut_ptr() as *mut c_void),
            &mut bi,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        let _ = DeleteObject(hgdi);
        if rows == 0 || w <= 0 || h <= 0 {
            return None;
        }
        encode_png(&mut px, w as u32, h as u32)
    }
}

/// BGRA (possibly premultiplied) → straight RGBA PNG data URL.
fn encode_png(px: &mut [u8], w: u32, h: u32) -> Option<String> {
    let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
    let premultiplied = has_alpha
        && px
            .chunks_exact(4)
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2); // BGRA → RGBA
        if !has_alpha {
            p[3] = 255;
        } else if premultiplied && p[3] > 0 && p[3] < 255 {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(px).ok()?;
    }
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out)
    ))
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

fn snapshot() -> Vec<AppEntry> {
    let wins = collect_windows();
    let fg = unsafe { GetForegroundWindow() }.0 as isize;
    let attention = with_set(&ATTENTION, |s| s.clone());
    let counts = BADGES.lock().unwrap().clone().unwrap_or_default();
    let exe_ids = EXE_IDS.lock().unwrap().clone().unwrap_or_default();

    let mut groups: HashMap<&str, Vec<&Win>> = HashMap::new();
    for w in &wins {
        groups.entry(w.key.as_str()).or_default().push(w);
    }

    let entry = |key: &str, name: String, pinned: bool, launch: String, icon: String| {
        let mut windows: Vec<WindowInfo> = groups
            .get(key)
            .map(|ws| {
                ws.iter()
                    .map(|w| WindowInfo {
                        hwnd: w.hwnd as i64,
                        title: w.title.clone(),
                        minimized: w.minimized,
                        attention: attention.contains(&w.hwnd),
                        z: w.z,
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Stable order for display and for click-to-cycle.
        windows.sort_by_key(|w| w.hwnd);
        // Every ID the app's notifications might be filed under: the package's
        // own, ones its windows carry, ones its Start menu shortcut carries,
        // and its exe path (for desktop apps without an ID).
        let mut ids: Vec<String> = vec![key.to_string()];
        if let Some(id) = key.strip_prefix(PKG).or_else(|| key.strip_prefix(WEB)) {
            ids.push(id.to_string());
        }
        if let Some(ws) = groups.get(key) {
            ids.extend(ws.iter().filter_map(|w| w.app_id.as_ref().map(|i| i.to_lowercase())));
        }
        if let Some(more) = exe_ids.get(key) {
            ids.extend(more.iter().cloned());
        }
        let wants_attention = windows.iter().any(|w| w.attention);
        // The app's own badge first. If it has none but is asking for
        // attention, show how many of its notifications are waiting.
        let badge = ids.iter().find_map(|id| counts.badges.get(id).cloned()).or_else(|| {
            let n = ids.iter().filter_map(|id| counts.waiting.get(id)).max().copied().unwrap_or(0);
            (wants_attention && n > 0).then(|| if n > 9 { "9+".to_string() } else { n.to_string() })
        });
        AppEntry {
            key: key.to_string(),
            name,
            pinned,
            launch,
            icon,
            badge,
            active: windows.iter().any(|w| w.hwnd as isize == fg),
            attention: windows.iter().any(|w| w.attention),
            windows,
        }
    };

    let mut out = Vec::new();
    let mut pinned_keys = HashSet::new();
    for pin in pins() {
        let key = pin_key(&pin);
        if pinned_keys.insert(key.clone()) {
            out.push(entry(&key, pin.name.clone(), true, pin.path.clone(), pin.path));
        }
    }

    let mut order = ORDER.lock().unwrap();
    order.retain(|k| groups.contains_key(k.as_str()) && !pinned_keys.contains(k));
    for w in &wins {
        if !pinned_keys.contains(&w.key) && !order.contains(&w.key) {
            order.push(w.key.clone());
        }
    }
    for key in order.iter() {
        let first = groups[key.as_str()][0];
        if key.starts_with(WEB) {
            let (name, launch, icon) = web_app_entry(key, first);
            out.push(entry(key, name, false, launch, icon));
            continue;
        }
        if let Some(id) = key.starts_with(PKG).then(|| package_app_id(&first.exe)).flatten() {
            // The shell draws the package's logo and opens the app from its ID.
            let shell = format!("{APPS_FOLDER}{id}");
            out.push(entry(key, app_name(key, &first.exe, &first.title), false, shell.clone(), shell));
            continue;
        }
        let name = app_name(key, &first.exe, &first.title);
        out.push(entry(key, name, false, first.exe.clone(), first.exe.clone()));
    }
    drop(order);

    with_set(&KNOWN, |k| {
        k.clear();
        k.extend(wins.iter().map(|w| w.hwnd));
    });
    out
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

fn launch(path: &str) -> Result<(), String> {
    com_init();
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("could not open {path} (code {})", r.0 as isize))
    }
}

pub(crate) fn focus(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            return;
        }
        // Windows refused: borrow the current foreground thread's input state.
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let me = GetCurrentThreadId();
        let attached = AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
}

/// Windows minimised by the last Show desktop, top first.
static DESKTOP_SHOWN: Mutex<Vec<isize>> = Mutex::new(Vec::new());

/// Show desktop: minimise every open window; a second click, while nothing
/// has been opened or restored since, brings the same windows back.
pub(crate) fn toggle_desktop() {
    let wins = collect_windows();
    let mut shown = DESKTOP_SHOWN.lock().unwrap();
    let open: Vec<&Win> = wins.iter().filter(|w| !w.minimized).collect();
    if open.is_empty() && !shown.is_empty() {
        // Bottom first, so the window that was on top ends up on top again.
        let still: HashSet<isize> = wins.iter().map(|w| w.hwnd).collect();
        let back: Vec<isize> = shown.drain(..).filter(|h| still.contains(h)).collect();
        for &h in back.iter().rev() {
            unsafe {
                let _ = ShowWindow(HWND(h as *mut c_void), SW_SHOWNOACTIVATE);
            }
        }
        if let Some(&top) = back.first() {
            focus(HWND(top as *mut c_void));
        }
    } else {
        let mut open = open;
        open.sort_by_key(|w| w.z);
        *shown = open.iter().map(|w| w.hwnd).collect();
        for w in &open {
            unsafe {
                let _ = ShowWindow(HWND(w.hwnd as *mut c_void), SW_SHOWMINNOACTIVE);
            }
        }
    }
    drop(shown);
    notify();
}

pub(crate) fn find(key: &str) -> Result<AppEntry, String> {
    snapshot()
        .into_iter()
        .find(|a| a.key == key)
        .ok_or_else(|| format!("no app {key}"))
}

pub(crate) fn hwnd(h: i64) -> HWND {
    HWND(h as isize as *mut c_void)
}

#[cfg(debug_assertions)]
pub(crate) fn snapshot_for_debug() -> Vec<AppEntry> {
    snapshot()
}

#[tauri::command]
pub async fn apps_snapshot() -> Vec<AppEntry> {
    snapshot()
}

#[tauri::command]
pub async fn app_icon(path: String) -> Option<String> {
    if let Some(cached) = with_map(&ICONS, |m| m.get(&path).cloned()) {
        return cached;
    }
    let icon = icon_data_url(&path);
    with_map(&ICONS, |m| m.insert(path, icon.clone()));
    icon
}

/// Left click: open, focus, minimize, or cycle through the app's windows.
#[tauri::command]
pub async fn app_activate(key: String) -> Result<(), String> {
    let app = find(&key)?;
    if app.windows.is_empty() {
        return launch(&app.launch);
    }
    let fg = unsafe { GetForegroundWindow() }.0 as i64;
    match app.windows.iter().position(|w| w.hwnd == fg) {
        Some(_) if app.windows.len() == 1 => unsafe {
            let _ = ShowWindow(hwnd(fg), SW_MINIMIZE);
        },
        Some(i) => focus(hwnd(app.windows[(i + 1) % app.windows.len()].hwnd)),
        None => {
            let top = app.windows.iter().min_by_key(|w| w.z).unwrap();
            focus(hwnd(top.hwnd));
        }
    }
    notify();
    Ok(())
}

/// Middle click / "New window": start another instance.
#[tauri::command]
pub async fn app_launch(key: String) -> Result<(), String> {
    launch(&find(&key)?.launch)
}

#[tauri::command]
pub async fn window_focus(hwnd_value: i64) {
    focus(hwnd(hwnd_value));
}

#[tauri::command]
pub async fn window_close(hwnd_value: i64) {
    unsafe {
        let _ = PostMessageW(Some(hwnd(hwnd_value)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

#[tauri::command]
pub async fn app_close_all(key: String) -> Result<(), String> {
    for w in find(&key)?.windows {
        unsafe {
            let _ = PostMessageW(Some(hwnd(w.hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn app_pin(key: String) -> Result<(), String> {
    let app = find(&key)?;
    if app.pinned || key.starts_with("store:") {
        return Ok(());
    }
    if key.starts_with(WEB) && !app.launch.to_lowercase().ends_with(".lnk") {
        return Err("This web app has no shortcut to pin. Reinstall it from the browser's Apps menu.".into());
    }
    let mut list = pins();
    list.push(Pin {
        path: app.launch,
        name: app.name,
    });
    settings::update(|s| s.pins = Some(list));
    notify();
    Ok(())
}

#[tauri::command]
pub async fn app_unpin(key: String) {
    let list: Vec<Pin> = pins().into_iter().filter(|p| pin_key(p) != key).collect();
    settings::update(|s| s.pins = Some(list));
    notify();
}

/// Reorder pins to follow `keys`; pins not mentioned keep their relative order at the end.
#[tauri::command]
pub async fn pins_reorder(keys: Vec<String>) {
    let mut list = pins();
    list.sort_by_key(|p| {
        let k = pin_key(p);
        keys.iter().position(|x| *x == k).unwrap_or(usize::MAX)
    });
    settings::update(|s| s.pins = Some(list));
    notify();
}

// ---------------------------------------------------------------------------
// Live updates
// ---------------------------------------------------------------------------

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if id_object != OBJID_WINDOW.0 || id_child != 0 || hwnd.is_invalid() {
        return;
    }
    if event == EVENT_OBJECT_SHOW {
        // In "hide completely" mode, Explorer re-showing its taskbar is undone at once.
        taskbar::on_taskbar_shown(hwnd);
    }
    let h = hwnd.0 as isize;
    let known = with_set(&KNOWN, |k| k.contains(&h));
    let relevant = match event {
        EVENT_SYSTEM_FOREGROUND => {
            with_set(&ATTENTION, |a| a.remove(&h));
            true
        }
        EVENT_OBJECT_DESTROY => {
            with_set(&ATTENTION, |a| a.remove(&h));
            known
        }
        EVENT_OBJECT_NAMECHANGE | EVENT_OBJECT_HIDE => known,
        // Only top-level windows can become taskbar buttons.
        _ => unsafe { GetAncestor(hwnd, GA_PARENT) == GetDesktopWindow() },
    };
    if relevant {
        notify();
    }
}

unsafe extern "system" fn shell_hook_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    let shellhook = SHELLHOOK_MSG.load(Ordering::Relaxed);
    if shellhook != 0 && msg == shellhook && wparam.0 == HSHELL_FLASH {
        with_set(&ATTENTION, |a| a.insert(lparam.0));
        notify();
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// Start live tracking. Must run on the main thread (the hooks need its message loop).
pub fn init(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
    let (tx, rx) = mpsc::channel::<()>();
    let _ = NOTIFY.set(tx);
    watch_badges();

    let app = app.clone();
    std::thread::spawn(move || {
        com_init();
        let mut last: Vec<AppEntry> = Vec::new();
        while rx.recv().is_ok() {
            // Debounce bursts (a window opening fires several events).
            std::thread::sleep(Duration::from_millis(80));
            while rx.try_recv().is_ok() {}
            let snap = snapshot();
            if snap != last {
                let _ = app.emit("apps-changed", &snap);
                last = snap;
            }
        }
    });

    unsafe {
        for (min, max) in [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
            (EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_NAMECHANGE),
            (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
        ] {
            let hook = SetWinEventHook(
                min,
                max,
                None,
                Some(win_event),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            );
            if hook.is_invalid() {
                eprintln!("[apps] SetWinEventHook {min:#x}..{max:#x} failed");
            }
        }

        let dock = HWND(window.hwnd()?.0 as _);
        SHELLHOOK_MSG.store(RegisterWindowMessageW(w!("SHELLHOOK")), Ordering::Relaxed);
        if !RegisterShellHookWindow(dock).as_bool() {
            eprintln!("[apps] RegisterShellHookWindow failed - no attention flashing");
        }
        let _ = SetWindowSubclass(dock, Some(shell_hook_proc), SUBCLASS_ID, 0);
    }
    notify();
    Ok(())
}

#[cfg(test)]
mod badge_tests {
    use super::*;

    /// Reads this PC's notification store and Start menu (nothing is changed).
    #[test]
    #[ignore]
    fn live_counts() {
        com_init();
        let c = crate::extras::read_notification_counts().unwrap();
        let mut waiting: Vec<_> = c.waiting.iter().collect();
        waiting.sort();
        println!("badges: {:?}", c.badges);
        println!("waiting: {waiting:?}");
        let mut m = HashMap::new();
        let menu = r"Microsoft\Windows\Start Menu\Programs";
        for var in ["APPDATA", "ProgramData"] {
            if let Some(base) = std::env::var_os(var) {
                scan_exe_ids(&Path::new(&base).join(menu), 3, &mut m);
            }
        }
        println!("exe ids: {}", m.len());
        for (exe, ids) in m.iter().filter(|(_, ids)| ids.iter().any(|i| c.waiting.contains_key(i) || c.badges.contains_key(i))) {
            println!("  matched {exe} -> {ids:?}");
        }
        // Claude (a Store app) is keyed by its app ID.
        let claude = "claude_pzs8sxrjxfjjc!claude";
        println!("claude waiting: {:?}", c.waiting.get(claude));
    }
}
