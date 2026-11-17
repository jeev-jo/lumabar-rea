//! The system strip: volume, network, battery and now-playing media, plus
//! buttons that open the matching Windows flyouts.
//!
//! A background thread reads everything once a second and emits a
//! `system-status` event only when something changed.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::NetworkManagement::IpHelper::{GetBestInterface, GetIfEntry2, MIB_IF_ROW2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_CONTROL, VK_LWIN,
};

const IF_TYPE_ETHERNET_CSMACD: u32 = 6;
const IF_TYPE_IEEE80211: u32 = 71;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Volume {
    /// 0–100
    pub level: u32,
    pub muted: bool,
    /// The output's name, e.g. "Speakers" or "Headphones".
    pub device: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Battery {
    pub percent: u32,
    pub charging: bool,
    /// Windows' estimate of time left on battery, when it has one.
    pub minutes_left: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Media {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// Album art as a data URL, when the player provides it.
    pub art: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Default)]
pub struct Status {
    pub volume: Option<Volume>,
    /// "wifi", "ethernet", "other" or "offline"
    pub network: String,
    pub battery: Option<Battery>,
    pub media: Option<Media>,
    /// Wi-Fi / Bluetooth radio switched on; None when the PC has no such radio.
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

static LAST: Mutex<Option<Status>> = Mutex::new(None);

fn com_init() {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

// ---------------------------------------------------------------------------
// Readers
// ---------------------------------------------------------------------------

fn endpoint_volume() -> Option<IAudioEndpointVolume> {
    com_init();
    unsafe {
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

/// Friendly name of the default output device ("Speakers (Realtek Audio)").
fn output_name() -> Option<String> {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::System::Com::{CoTaskMemFree, STGM_READ};
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let text = PropVariantToStringAlloc(&value).ok()?;
        let name = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        name
    }
}

fn read_volume() -> Option<Volume> {
    let v = endpoint_volume()?;
    // "Speakers (Realtek(R) Audio)" -> "Speakers"
    let device = output_name()
        .map(|n| n.split(" (").next().unwrap_or(&n).trim().to_string())
        .unwrap_or_else(|| "Sound".into());
    unsafe {
        Some(Volume {
            level: (v.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32,
            muted: v.GetMute().ok()?.as_bool(),
            device,
        })
    }
}

/// Which kind of link carries traffic to the internet right now.
fn read_network() -> String {
    unsafe {
        let mut index = 0u32;
        // 1.1.1.1 in network byte order; no packet is sent, Windows only consults its routes.
        let dest = u32::from_ne_bytes([1, 1, 1, 1]);
        if GetBestInterface(dest, &mut index) != 0 {
            return "offline".into();
        }
        let mut row = MIB_IF_ROW2 {
            InterfaceIndex: index,
            ..Default::default()
        };
        if GetIfEntry2(&mut row).is_err() || row.OperStatus != IfOperStatusUp {
            return "offline".into();
        }
        match row.Type {
            IF_TYPE_IEEE80211 => "wifi",
            IF_TYPE_ETHERNET_CSMACD => "ethernet",
            _ => "other",
        }
        .into()
    }
}

fn read_battery() -> Option<Battery> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    // BatteryFlag 128 = no system battery; 255 = unknown.
    if s.BatteryFlag & 128 != 0 || s.BatteryFlag == 255 || s.BatteryLifePercent > 100 {
        return None;
    }
    Some(Battery {
        percent: s.BatteryLifePercent as u32,
        charging: s.ACLineStatus == 1,
        // 0xFFFFFFFF = unknown (e.g. while plugged in).
        minutes_left: (s.BatteryLifeTime != u32::MAX && s.ACLineStatus != 1).then(|| s.BatteryLifeTime / 60),
    })
}

fn media_session() -> Option<GlobalSystemMediaTransportControlsSession> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
        .ok()?
        .get()
        .ok()?;
    manager.GetCurrentSession().ok()
}

fn read_media() -> Option<Media> {
    let session = media_session()?;
    let props = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().ok()?.to_string();
    if title.is_empty() {
        return None;
    }
    let playing = session
        .GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map(|s| s == PlaybackStatus::Playing)
        .unwrap_or(false);
    let artist = props.Artist().map(|a| a.to_string()).unwrap_or_default();
    let art = album_art(&props, &format!("{title}\u{1}{artist}"));
    Some(Media { title, artist, playing, art })
}

/// The track's thumbnail as a data URL, read once per track.
fn album_art(
    props: &windows::Media::Control::GlobalSystemMediaTransportControlsSessionMediaProperties,
    track: &str,
) -> Option<String> {
    use base64::Engine;
    use windows::Storage::Streams::DataReader;
    static CACHE: Mutex<Option<(String, Option<String>)>> = Mutex::new(None);
    if let Some((k, v)) = CACHE.lock().unwrap().as_ref() {
        if k == track {
            return v.clone();
        }
    }
    let read = || -> Option<String> {
        let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()?.min(4 * 1024 * 1024) as u32;
        if size == 0 {
            return None;
        }
        let mime = stream.ContentType().map(|c| c.to_string()).unwrap_or_default();
        let reader = DataReader::CreateDataReader(&stream).ok()?;
        reader.LoadAsync(size).ok()?.get().ok()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        let mime = if mime.starts_with("image/") { mime } else { "image/png".into() };
        Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
    };
    let art = read();
    *CACHE.lock().unwrap() = Some((track.to_string(), art.clone()));
    art
}

/// Wi-Fi and Bluetooth radio states (None = no such radio on this PC).
fn read_radios() -> (Option<bool>, Option<bool>) {
    use windows::Devices::Radios::{Radio, RadioKind, RadioState};
    let Ok(list) = Radio::GetRadiosAsync().and_then(|op| op.get()) else {
        return (None, None);
    };
    let (mut wifi, mut bt) = (None, None);
    for radio in list {
        let on = radio.State().map(|s| s == RadioState::On).unwrap_or(false);
        match radio.Kind() {
            Ok(RadioKind::WiFi) => wifi = Some(wifi.unwrap_or(false) || on),
            Ok(RadioKind::Bluetooth) => bt = Some(bt.unwrap_or(false) || on),
            _ => {}
        }
    }
    (wifi, bt)
}

static RADIOS: Mutex<(u32, (Option<bool>, Option<bool>))> = Mutex::new((0, (None, None)));

/// Radios are slower to query, so they are read every 5th second.
fn radios_cached(force: bool) -> (Option<bool>, Option<bool>) {
    let mut r = RADIOS.lock().unwrap();
    if force || r.0 % 5 == 0 {
        r.1 = read_radios();
    }
    r.0 = r.0.wrapping_add(1);
    r.1
}

fn read_all() -> Status {
    let (wifi, bluetooth) = radios_cached(false);
    Status {
        volume: read_volume(),
        network: read_network(),
        battery: read_battery(),
        media: read_media(),
        wifi,
        bluetooth,
    }
}

/// Re-read now and push the result if it changed (after a button press, say).
fn refresh(app: &AppHandle) {
    let now = read_all();
    let mut last = LAST.lock().unwrap();
    if last.as_ref() != Some(&now) {
        let _ = app.emit("system-status", &now);
        *last = Some(now);
    }
}

pub fn init(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        com_init();
        loop {
            refresh(&app);
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

// ---------------------------------------------------------------------------
// Keyboard shortcuts for the Windows flyouts
// ---------------------------------------------------------------------------

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                ..Default::default()
            },
        },
    }
}

/// Press `keys` together, then release them in reverse order.
fn chord(keys: &[VIRTUAL_KEY]) {
    let mut inputs: Vec<INPUT> = keys.iter().map(|&k| key(k, false)).collect();
    inputs.extend(keys.iter().rev().map(|&k| key(k, true)));
    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn system_status() -> Status {
    let s = read_all();
    *LAST.lock().unwrap() = Some(s.clone());
    s
}

/// "start" (Start menu), "quick" (Quick Settings), "notifications" (calendar
/// and notifications) or "volume" (sound output picker).
#[tauri::command]
pub fn open_flyout(which: String) -> Result<(), String> {
    let a = VIRTUAL_KEY(b'A' as u16);
    let n = VIRTUAL_KEY(b'N' as u16);
    let v = VIRTUAL_KEY(b'V' as u16);
    match which.as_str() {
        "start" => chord(&[VK_LWIN]),
        "quick" => chord(&[VK_LWIN, a]),
        "notifications" => chord(&[VK_LWIN, n]),
        "volume" => chord(&[VK_LWIN, VK_CONTROL, v]),
        other => return Err(format!("unknown flyout {other}")),
    }
    Ok(())
}

/// Change the volume by `delta` percentage points (scroll wheel).
#[tauri::command]
pub async fn volume_step(app: AppHandle, delta: i32) -> Result<(), String> {
    let v = endpoint_volume().ok_or("no audio output")?;
    unsafe {
        let now = v.GetMasterVolumeLevelScalar().map_err(|e| e.to_string())?;
        let next = (now + delta as f32 / 100.0).clamp(0.0, 1.0);
        v.SetMasterVolumeLevelScalar(next, std::ptr::null())
            .map_err(|e| e.to_string())?;
        if next > 0.0 && v.GetMute().map(|m| m.as_bool()).unwrap_or(false) {
            let _ = v.SetMute(false, std::ptr::null());
        }
    }
    refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn volume_toggle_mute(app: AppHandle) -> Result<(), String> {
    let v = endpoint_volume().ok_or("no audio output")?;
    unsafe {
        let muted = v.GetMute().map_err(|e| e.to_string())?.as_bool();
        v.SetMute(!muted, std::ptr::null())
            .map_err(|e| e.to_string())?;
    }
    refresh(&app);
    Ok(())
}

/// "toggle", "next" or "previous".
#[tauri::command]
pub async fn media_control(app: AppHandle, action: String) -> Result<(), String> {
    com_init();
    let session = media_session().ok_or("nothing is playing")?;
    let op = match action.as_str() {
        "toggle" => session.TryTogglePlayPauseAsync(),
        "next" => session.TrySkipNextAsync(),
        "previous" => session.TrySkipPreviousAsync(),
        other => return Err(format!("unknown media action {other}")),
    };
    op.and_then(|o| o.get()).map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(150));
    refresh(&app);
    Ok(())
}

/// Set the volume to `level` percent (the Speakers widget's slider).
#[tauri::command]
pub async fn volume_set(app: AppHandle, level: u32) -> Result<(), String> {
    let v = endpoint_volume().ok_or("no audio output")?;
    unsafe {
        v.SetMasterVolumeLevelScalar(level.min(100) as f32 / 100.0, std::ptr::null())
            .map_err(|e| e.to_string())?;
        if level > 0 {
            let _ = v.SetMute(false, std::ptr::null());
        }
    }
    refresh(&app);
    Ok(())
}

/// Switch the Wi-Fi or Bluetooth radio on or off.
#[tauri::command]
pub async fn radio_set(app: AppHandle, kind: String, on: bool) -> Result<(), String> {
    use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
    com_init();
    let want = match kind.as_str() {
        "wifi" => RadioKind::WiFi,
        "bluetooth" => RadioKind::Bluetooth,
        other => return Err(format!("unknown radio {other}")),
    };
    let access = Radio::RequestAccessAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    if access != RadioAccessStatus::Allowed {
        return Err("Windows didn't allow switching this radio. Use Quick Settings instead.".into());
    }
    let list = Radio::GetRadiosAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
    let state = if on { RadioState::On } else { RadioState::Off };
    let mut found = false;
    for radio in list {
        if radio.Kind().ok() == Some(want) {
            found = true;
            radio.SetStateAsync(state).and_then(|op| op.get()).map_err(|e| e.to_string())?;
        }
    }
    if !found {
        return Err("This PC has no such radio.".into());
    }
    radios_cached(true);
    refresh(&app);
    Ok(())
}

/// Open a Windows Settings page (only the pages the widgets link to).
#[tauri::command]
pub fn open_ms_settings(page: String) -> Result<(), String> {
    const ALLOWED: &[&str] = &[
        "network-wifi",
        "bluetooth",
        "sound",
        "batterysaver",
        "personalization-background",
    ];
    if !ALLOWED.contains(&page.as_str()) {
        return Err(format!("not an allowed settings page: {page}"));
    }
    open_uri(&format!("ms-settings:{page}"));
    Ok(())
}

/// Open the quick list the Windows taskbar shows for Wi-Fi networks or
/// Bluetooth devices.
#[tauri::command]
pub fn open_quick_list(kind: String) -> Result<(), String> {
    let uri = match kind.as_str() {
        "wifi" => "ms-availablenetworks:",
        "bluetooth" => "ms-actioncenter:controlcenter/bluetooth",
        other => return Err(format!("unknown list {other}")),
    };
    open_uri(uri);
    Ok(())
}

fn open_uri(uri: &str) {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(uri), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// The desktop wallpaper as a thumbnail data URL.
#[tauri::command]
pub async fn wallpaper_image() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETDESKWALLPAPER, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut buf = [0u16; 1024];
    unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buf.len() as u32,
            Some(buf.as_mut_ptr() as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .ok()?;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..end]);
    if path.is_empty() {
        return None;
    }
    tauri::async_runtime::spawn_blocking(move || crate::apps::image_data_url(&path, 320, false))
        .await
        .ok()
        .flatten()
}
