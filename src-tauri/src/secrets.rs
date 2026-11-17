//! API keys pasted into widgets, encrypted with Windows DPAPI for the current
//! user and stored in `%LOCALAPPDATA%\luma_ai_bar\secrets`. The plain key only
//! ever exists in Rust memory; the UI can set or clear a key but never read it.

use std::fs;
use std::path::PathBuf;

use windows::core::w;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

use crate::taskbar;

fn path(id: &str) -> PathBuf {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    taskbar::data_dir().join("secrets").join(format!("{safe}.bin"))
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

/// Copy DPAPI's output and free its buffer.
unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(Some(HLOCAL(out.pbData as *mut _))) };
    v
}

pub fn store(id: &str, secret: &str) -> Result<(), String> {
    let input = blob(secret.as_bytes());
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            w!("luma-ai-bar widget key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .map_err(|e| format!("could not encrypt the key: {e}"))?;
    }
    let encrypted = unsafe { take(out) };
    let p = path(id);
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(p, encrypted).map_err(|e| e.to_string())
}

pub fn load(id: &str) -> Option<String> {
    let encrypted = fs::read(path(id)).ok()?;
    let input = blob(&encrypted);
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out).ok()?;
    }
    String::from_utf8(unsafe { take(out) }).ok()
}

pub fn exists(id: &str) -> bool {
    path(id).exists()
}

pub fn remove(id: &str) {
    let _ = fs::remove_file(path(id));
}
