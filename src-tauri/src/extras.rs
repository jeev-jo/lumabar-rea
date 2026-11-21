//! More PC gadgets: keyboard language, temperature, brightness, mobile hotspot,
//! airplane mode, notifications, on-screen keyboard, recently opened files,
//! search, a command launcher over the Start menu, and screenshots.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LWIN, VK_SNAPSHOT, VK_SPACE};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

const NO_WINDOW: u32 = 0x0800_0000;

fn shell_open(target: &str, args: Option<&str>) {
    let args = args.map(HSTRING::from);
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(target),
            args.as_ref().map(|a| PCWSTR(a.as_ptr())).unwrap_or(PCWSTR::null()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

// ---- Keyboard language --------------------------------------------------------

#[derive(Serialize)]
pub struct Language {
    /// Short code as on the taskbar, e.g. "ENG".
    short: String,
    /// e.g. "English (United States)"
    name: String,
    /// Every installed layout's name.
    all: Vec<String>,
}

fn locale_info(langid: u16) -> (String, String) {
    use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SISO639LANGNAME2, LOCALE_SLOCALIZEDDISPLAYNAME};
    let mut name = [0u16; 85];
    let n = unsafe { LCIDToLocaleName(langid as u32, Some(&mut name), 0) };
    if n == 0 {
        return (format!("{langid:04X}"), format!("Layout {langid:04X}"));
    }
    let locale = PCWSTR(name.as_ptr());
    let get = |what| {
        let mut buf = [0u16; 128];
        let len = unsafe { GetLocaleInfoEx(locale, what, Some(&mut buf)) };
        String::from_utf16_lossy(&buf[..(len.max(1) - 1) as usize])
    };
    (get(LOCALE_SISO639LANGNAME2).to_uppercase(), get(LOCALE_SLOCALIZEDDISPLAYNAME))
}

#[tauri::command]
pub fn language_get() -> Language {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardLayoutList};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let hkl = unsafe {
        let fg = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(fg, None);
        GetKeyboardLayout(thread)
    };
    let (short, name) = locale_info((hkl.0 as usize & 0xFFFF) as u16);
    let count = unsafe { GetKeyboardLayoutList(None) };
    let mut list = vec![Default::default(); count.max(0) as usize];
    unsafe { GetKeyboardLayoutList(Some(&mut list)) };
    let mut all: Vec<String> = list.iter().map(|h| locale_info((h.0 as usize & 0xFFFF) as u16).1).collect();
    all.dedup();
    Language { short, name, all }
}

/// Switch to the next keyboard layout (Win+Space, as Windows does).
#[tauri::command]
pub fn language_next() {
    crate::system::chord(&[VK_LWIN, VK_SPACE]);
}

// ---- Temperature --------------------------------------------------------------

#[derive(Serialize)]
pub struct Temperature {
    /// Hottest thermal zone, °C.
    celsius: Option<f64>,
    zones: Vec<(String, f64)>,
}

/// ACPI thermal zones from Windows' performance counters, which any user can
/// read. These are the board's sensors near the CPU, not the CPU's own cores.
#[tauri::command]
pub async fn temperature() -> Temperature {
    tauri::async_runtime::spawn_blocking(read_temperature).await.unwrap_or(Temperature { celsius: None, zones: vec![] })
}

fn read_temperature() -> Temperature {
    use windows::Win32::System::Performance::*;
    let mut zones = vec![];
    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
            return Temperature { celsius: None, zones };
        }
        let mut counter = PDH_HCOUNTER::default();
        if PdhAddEnglishCounterW(query, w!("\\Thermal Zone Information(*)\\Temperature"), 0, &mut counter) == 0
            && PdhCollectQueryData(query) == 0
        {
            let mut size = 0u32;
            let mut count = 0u32;
            let _ = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size > 0 {
                let mut buf = vec![0u8; size as usize];
                let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) == 0 {
                    for i in 0..count as usize {
                        let item = &*items.add(i);
                        let kelvin = item.FmtValue.Anonymous.doubleValue;
                        let c = kelvin - 273.15;
                        if c > 0.0 && c < 130.0 {
                            let name = item.szName.to_string().unwrap_or_default();
                            zones.push((name.trim_start_matches("\\_TZ.").to_string(), (c * 10.0).round() / 10.0));
                        }
                    }
                }
            }
        }
        let _ = PdhCloseQuery(query);
    }
    let celsius = zones.iter().map(|z| z.1).fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
    Temperature { celsius, zones }
}

// ---- Brightness ---------------------------------------------------------------

fn powershell(script: &str) -> Result<String, String> {
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("failed").to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The built-in display's brightness (0–100), or None when the screen's
/// brightness can't be set from Windows (most desktop monitors).
#[tauri::command]
pub async fn brightness_get() -> Option<u32> {
    tauri::async_runtime::spawn_blocking(|| {
        powershell("(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction Stop | Select-Object -First 1).CurrentBrightness")
            .ok()?
            .parse()
            .ok()
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub async fn brightness_set(level: u32) -> Result<(), String> {
    let level = level.min(100);
    tauri::async_runtime::spawn_blocking(move || {
        powershell(&format!(
            "Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods -ErrorAction Stop | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=0;Brightness={level}}} | Out-Null"
        ))
        .map(|_| ())
        .map_err(|_| "This screen's brightness can't be set from Windows.".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Mobile hotspot and airplane mode -----------------------------------------

fn com_init() {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

fn tethering() -> Result<windows::Networking::NetworkOperators::NetworkOperatorTetheringManager, String> {
    use windows::Networking::Connectivity::NetworkInformation;
    use windows::Networking::NetworkOperators::NetworkOperatorTetheringManager;
    com_init();
    let profile = NetworkInformation::GetInternetConnectionProfile().map_err(|_| "Not connected to the internet.".to_string())?;
    NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile).map_err(|_| "This connection can't be shared.".to_string())
}

#[derive(Serialize)]
pub struct Radios {
    /// None when this PC can't share its connection.
    hotspot: Option<bool>,
    /// True when every radio is off.
    airplane: Option<bool>,
}

#[tauri::command]
pub async fn radios_status() -> Radios {
    tauri::async_runtime::spawn_blocking(|| {
        use windows::Devices::Radios::{Radio, RadioState};
        use windows::Networking::NetworkOperators::TetheringOperationalState;
        let hotspot = tethering().ok().and_then(|m| m.TetheringOperationalState().ok()).map(|s| s == TetheringOperationalState::On);
        com_init();
        let airplane = Radio::GetRadiosAsync().and_then(|op| op.get()).ok().and_then(|list| {
            let states: Vec<RadioState> = list.into_iter().filter_map(|r| r.State().ok()).collect();
            (!states.is_empty()).then(|| states.iter().all(|s| *s != RadioState::On))
        });
        Radios { hotspot, airplane }
    })
    .await
    .unwrap_or(Radios { hotspot: None, airplane: None })
}

#[tauri::command]
pub async fn hotspot_set(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Networking::NetworkOperators::TetheringOperationStatus;
        let m = tethering()?;
        let op = if on { m.StartTetheringAsync() } else { m.StopTetheringAsync() };
        let result = op.and_then(|o| o.get()).map_err(|e| format!("Windows didn't allow it ({e}). Use Quick Settings instead."))?;
        let status = result.Status().map_err(|e| e.to_string())?;
        if status == TetheringOperationStatus::Success {
            Ok(())
        } else {
            let msg = result.AdditionalErrorMessage().map(|m| m.to_string()).unwrap_or_default();
            Err(if msg.is_empty() { "Windows couldn't change the hotspot. Use Quick Settings instead.".into() } else { msg })
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Turn every radio (Wi-Fi, Bluetooth, cellular) off, or back on.
#[tauri::command]
pub async fn airplane_set(on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioState};
        com_init();
        let access = Radio::RequestAccessAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
        if access != RadioAccessStatus::Allowed {
            return Err("Windows didn't allow switching radios. Use Quick Settings instead.".into());
        }
        let list = Radio::GetRadiosAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
        let state = if on { RadioState::Off } else { RadioState::On };
        for r in list {
            let _ = r.SetStateAsync(state).and_then(|op| op.get());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Notifications and keyboard -----------------------------------------------

#[tauri::command]
pub fn open_on_screen_keyboard() {
    // The touch keyboard where it exists, else the classic on-screen keyboard.
    let tabtip = Path::new(r"C:\Program Files\Common Files\microsoft shared\ink\TabTip.exe");
    if tabtip.exists() {
        shell_open(&tabtip.to_string_lossy(), None);
    } else {
        shell_open("osk.exe", None);
    }
}

// ---- Recently opened -------------------------------------------------------------

#[derive(Serialize)]
pub struct Shortcut {
    name: String,
    path: String,
    /// "folder", or the file's extension in lower case.
    kind: String,
    /// Seconds since it was last opened.
    ago: u64,
}

fn recent_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(r"Microsoft\Windows\Recent"))
}

#[tauri::command]
pub async fn recent_files() -> Vec<Shortcut> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(dir) = recent_dir() else { return vec![] };
        let now = std::time::SystemTime::now();
        let mut out: Vec<(std::time::SystemTime, Shortcut)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if p.extension()?.to_str()? != "lnk" {
                    return None;
                }
                let name = p.file_stem()?.to_string_lossy().to_string();
                let modified = e.metadata().ok()?.modified().ok()?;
                let ext = Path::new(&name).extension().map(|x| x.to_string_lossy().to_lowercase());
                let kind = match ext {
                    Some(x) if x.len() <= 5 && !x.contains(' ') => x,
                    _ => "folder".into(),
                };
                Some((
                    modified,
                    Shortcut {
                        name,
                        path: p.to_string_lossy().to_string(),
                        kind,
                        ago: now.duration_since(modified).map(|d| d.as_secs()).unwrap_or(0),
                    },
                ))
            })
            .collect();
        out.sort_by(|a, b| b.0.cmp(&a.0));
        out.into_iter().take(20).map(|x| x.1).collect()
    })
    .await
    .unwrap_or_default()
}

// ---- Start menu commands ------------------------------------------------------------

fn start_dirs() -> Vec<PathBuf> {
    let mut v = vec![];
    if let Some(p) = std::env::var_os("ProgramData") {
        v.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(a) = std::env::var_os("APPDATA") {
        v.push(PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    v
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<Shortcut>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() && depth < 3 {
            walk(&p, depth + 1, out);
        } else if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("lnk") || x.eq_ignore_ascii_case("url")) == Some(true) {
            let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let lower = name.to_lowercase();
            if lower.contains("uninstall") || lower.contains("readme") || lower.contains("help") && lower.contains("manual") {
                continue;
            }
            out.push(Shortcut { name, path: p.to_string_lossy().to_string(), kind: "app".into(), ago: 0 });
        }
    }
}

/// Every app shortcut in the Start menu, by name.
#[tauri::command]
pub async fn start_apps() -> Vec<Shortcut> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut out = vec![];
        for d in start_dirs() {
            walk(&d, 0, &mut out);
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        out
    })
    .await
    .unwrap_or_default()
}

/// Open a shortcut from the Start menu or the Recent folder (nothing else).
#[tauri::command]
pub fn open_shortcut(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    let canon = p.canonicalize().map_err(|_| "That item no longer exists.".to_string())?;
    let allowed = start_dirs().into_iter().chain(recent_dir()).filter_map(|d| d.canonicalize().ok()).any(|d| canon.starts_with(d));
    if !allowed || path.contains("..") {
        return Err("not a Start menu or recent item".into());
    }
    shell_open(&path, None);
    Ok(())
}

/// Built-in actions for the command launcher.
#[tauri::command]
pub fn run_action(action: String) -> Result<(), String> {
    match action.as_str() {
        "lock" => {
            let _ = Command::new("rundll32.exe").args(["user32.dll,LockWorkStation"]).creation_flags(NO_WINDOW).spawn();
        }
        "settings" => shell_open("ms-settings:", None),
        "explorer" => shell_open("explorer.exe", None),
        "taskmgr" => shell_open("taskmgr.exe", None),
        "terminal" => shell_open("wt.exe", None),
        "downloads" => shell_open("shell:Downloads", None),
        "documents" => shell_open("shell:Personal", None),
        "screenshots" => shell_open("shell:My Pictures\\Screenshots", None),
        "bin" => shell_open("shell:RecycleBinFolder", None),
        "controlpanel" => shell_open("control.exe", None),
        "calc" => shell_open("calc.exe", None),
        "notepad" => shell_open("notepad.exe", None),
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}

/// Search box: open a folder or file path in Explorer, or search the web.
#[tauri::command]
pub fn search_open(query: String, engine: Option<String>) -> Result<(), String> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(());
    }
    let expanded = if let Some(rest) = q.strip_prefix('~') {
        std::env::var("USERPROFILE").unwrap_or_default() + rest
    } else {
        q.to_string()
    };
    let looks_like_path = expanded.len() >= 2 && (expanded.as_bytes()[1] == b':' || expanded.starts_with("\\\\"));
    if looks_like_path {
        let p = Path::new(&expanded);
        if p.is_dir() {
            shell_open("explorer.exe", Some(&format!("\"{}\"", expanded)));
            return Ok(());
        }
        if p.exists() {
            shell_open("explorer.exe", Some(&format!("/select,\"{}\"", expanded)));
            return Ok(());
        }
        return Err("That folder or file doesn't exist.".into());
    }
    if q.starts_with("https://") && !q.contains(' ') {
        shell_open(q, None);
        return Ok(());
    }
    let enc: String = q
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let url = match engine.as_deref() {
        Some("bing") => format!("https://www.bing.com/search?q={enc}"),
        Some("duckduckgo") => format!("https://duckduckgo.com/?q={enc}"),
        _ => format!("https://www.google.com/search?q={enc}"),
    };
    shell_open(&url, None);
    Ok(())
}

// ---- Screenshots ------------------------------------------------------------------

/// "screen": the whole screen, saved to Pictures\Screenshots (Win+PrtScn).
/// "region" / "window": Windows' own snipping overlay.
#[tauri::command]
pub fn screenshot(mode: String) -> Result<(), String> {
    match mode.as_str() {
        "screen" => crate::system::chord(&[VK_LWIN, VK_SNAPSHOT]),
        "region" => shell_open("ms-screenclip:", None),
        "window" => shell_open("ms-screenclip:?clippingMode=Window", None),
        "folder" => shell_open("shell:My Pictures\\Screenshots", None),
        other => return Err(format!("unknown screenshot mode {other}")),
    }
    Ok(())
}


// ---------------------------------------------------------------------------
// Messages waiting in Windows' notification center (WhatsApp)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct AppMessage {
    pub title: String,
    pub text: String,
    /// Milliseconds since 1970.
    pub at: i64,
}

#[derive(serde::Serialize)]
pub struct AppMessages {
    /// Whether the app was found among Windows' notification senders at all.
    pub known: bool,
    pub count: usize,
    pub items: Vec<AppMessage>,
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'").replace("&amp;", "&")
}

/// The `<text>` lines of a toast's XML.
fn toast_texts(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<text") {
        rest = &rest[i + 5..];
        let Some(open_end) = rest.find('>') else { break };
        if rest[..open_end].ends_with('/') {
            rest = &rest[open_end + 1..];
            continue;
        }
        let body = &rest[open_end + 1..];
        let Some(close) = body.find("</text>") else { break };
        let t = xml_unescape(body[..close].trim());
        if !t.is_empty() {
            out.push(t);
        }
        rest = &body[close..];
    }
    out
}

fn notification_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(r"Microsoft\Windows\Notifications"))
}

/// Changes whenever Windows writes to its notification store.
pub(crate) fn notification_store_stamp() -> Option<(std::time::SystemTime, u64)> {
    let dir = notification_dir()?;
    ["wpndatabase.db-wal", "wpndatabase.db"]
        .iter()
        .filter_map(|f| std::fs::metadata(dir.join(f)).ok())
        .filter_map(|m| Some((m.modified().ok()?, m.len())))
        .max_by_key(|(t, _)| *t)
}

/// Run `f` on a private copy of Windows' notification store (Windows keeps
/// the real one locked). Each caller gets its own copy.
fn with_notification_db<R>(tag: &str, f: impl FnOnce(&rusqlite::Connection) -> Result<R, String>) -> Result<R, String> {
    let src = notification_dir().ok_or("no LOCALAPPDATA")?;
    let tmp = std::env::temp_dir().join(format!("luma_ai_bar_wpn_{tag}"));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    for name in ["wpndatabase.db", "wpndatabase.db-wal", "wpndatabase.db-shm"] {
        let (from, to) = (src.join(name), tmp.join(name));
        if from.exists() {
            std::fs::copy(&from, &to).map_err(|e| format!("couldn't read Windows' notifications: {e}"))?;
        } else {
            let _ = std::fs::remove_file(&to);
        }
    }
    let out = {
        let db = rusqlite::Connection::open(tmp.join("wpndatabase.db")).map_err(|e| e.to_string())?;
        f(&db)
    };
    let _ = std::fs::remove_dir_all(&tmp);
    out
}

/// Taskbar badges by lower-cased app ID: a count ("3", "99+"), or "•" for a
/// status glyph such as "newMessage" or "alert". Zero and "none" are left out.
pub(crate) fn read_badges() -> Result<std::collections::HashMap<String, String>, String> {
    with_notification_db("badges", |db| {
        let mut stmt = db
            .prepare(
                "SELECT h.PrimaryId, n.Payload FROM Notification n JOIN NotificationHandler h ON h.RecordId = n.HandlerId \
                 WHERE n.Type = 'badge' ORDER BY n.ArrivalTime",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<Vec<u8>>>(1)?.unwrap_or_default())))
            .map_err(|e| e.to_string())?;
        let mut out = std::collections::HashMap::new();
        for (id, payload) in rows.flatten() {
            let xml = String::from_utf8_lossy(&payload);
            let Some(start) = xml.find("value=\"").map(|i| i + 7) else { continue };
            let value = xml[start..].split('"').next().unwrap_or_default().trim().to_string();
            let shown = match value.parse::<u32>() {
                Ok(0) => None,
                Ok(n) if n > 99 => Some("99+".to_string()),
                Ok(n) => Some(n.to_string()),
                Err(_) if value.is_empty() || value == "none" => None,
                Err(_) => Some("•".to_string()),
            };
            let id = id.to_lowercase();
            match shown {
                Some(v) => {
                    out.insert(id, v);
                }
                None => {
                    out.remove(&id);
                }
            }
        }
        Ok(out)
    })
}

/// Notifications from one app that are still in the notification center, read
/// from Windows' own notification store. `app` is matched against the sender's
/// app ID, e.g. "whatsapp".
#[tauri::command]
pub async fn app_messages(app: String) -> Result<AppMessages, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_notification_db("messages", |db| {
            let like = format!("%{}%", app.to_lowercase());
            let known: bool = db
                .query_row("SELECT EXISTS(SELECT 1 FROM NotificationHandler WHERE lower(PrimaryId) LIKE ?1)", [&like], |r| r.get(0))
                .unwrap_or(false);
            let mut stmt = db
                .prepare(
                    "SELECT n.Payload, n.ArrivalTime FROM Notification n JOIN NotificationHandler h ON h.RecordId = n.HandlerId \
                     WHERE n.Type = 'toast' AND lower(h.PrimaryId) LIKE ?1 ORDER BY n.ArrivalTime DESC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([&like], |r| {
                    let payload: Vec<u8> = r.get::<_, Option<Vec<u8>>>(0)?.unwrap_or_default();
                    let arrival: i64 = r.get::<_, Option<i64>>(1)?.unwrap_or(0);
                    Ok((payload, arrival))
                })
                .map_err(|e| e.to_string())?;
            let mut items = Vec::new();
            for (payload, arrival) in rows.flatten() {
                let texts = toast_texts(&String::from_utf8_lossy(&payload));
                items.push(AppMessage {
                    title: texts.first().cloned().unwrap_or_default(),
                    text: texts.get(1..).map(|t| t.join(" ")).unwrap_or_default(),
                    // FILETIME: 100 ns ticks since 1601.
                    at: arrival / 10_000 - 11_644_473_600_000,
                });
            }
            Ok(AppMessages { known, count: items.len(), items })
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
