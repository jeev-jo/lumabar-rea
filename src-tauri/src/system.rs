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
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Battery {
    pub percent: u32,
    pub charging: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Media {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Default)]
pub struct Status {
    pub volume: Option<Volume>,
    /// "wifi", "ethernet", "other" or "offline"
    pub network: String,
    pub battery: Option<Battery>,
    pub media: Option<Media>,
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

fn read_volume() -> Option<Volume> {
    let v = endpoint_volume()?;
    unsafe {
        Some(Volume {
            level: (v.GetMasterVolumeLevelScalar().ok()? * 100.0).round() as u32,
            muted: v.GetMute().ok()?.as_bool(),
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
    Some(Media {
        title,
        artist: props.Artist().map(|a| a.to_string()).unwrap_or_default(),
        playing,
    })
}

fn read_all() -> Status {
    Status {
        volume: read_volume(),
        network: read_network(),
        battery: read_battery(),
        media: read_media(),
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
