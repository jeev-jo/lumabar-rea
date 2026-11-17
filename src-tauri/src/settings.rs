//! User settings, stored as JSON in `%LOCALAPPDATA%\luma_ai_bar\settings.json`.

use std::fs;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::dock::Edge;
use crate::taskbar;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskbarMode {
    /// Auto-hide the Windows taskbar while the dock runs.
    Hide,
    /// Leave the Windows taskbar alone.
    Leave,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub edge: Edge,
    pub taskbar_mode: TaskbarMode,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            edge: Edge::Bottom,
            taskbar_mode: TaskbarMode::Hide,
        }
    }
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);

fn path() -> std::path::PathBuf {
    taskbar::data_dir().join("settings.json")
}

pub fn get() -> Settings {
    let mut guard = SETTINGS.lock().unwrap();
    *guard.get_or_insert_with(|| {
        fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    })
}

pub fn update(f: impl FnOnce(&mut Settings)) -> Settings {
    let mut s = get();
    f(&mut s);
    *SETTINGS.lock().unwrap() = Some(s);
    let write = fs::create_dir_all(taskbar::data_dir())
        .and_then(|_| fs::write(path(), serde_json::to_string_pretty(&s).unwrap()));
    if let Err(e) = write {
        eprintln!("[settings] could not save: {e}");
    }
    s
}

#[tauri::command]
pub fn get_settings() -> Settings {
    get()
}

#[tauri::command]
pub fn set_taskbar_mode(mode: TaskbarMode) -> Settings {
    let s = update(|s| s.taskbar_mode = mode);
    match mode {
        TaskbarMode::Hide => taskbar::hide(),
        TaskbarMode::Leave => taskbar::restore(),
    }
    s
}
