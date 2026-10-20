//! Détection des navigateurs installés, d'après la liste que Windows tient
//! dans le registre (`Clients\StartMenuInternet`).

use std::collections::HashSet;
use std::ffi::c_void;

use polyptyque_core::browsers::{classify_executable, exe_from_command, is_tor_browser};
use polyptyque_core::config::BrowserFamily;
use polyptyque_core::platform::BrowserInstall;
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
    RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
};

const BASE: &str = r"SOFTWARE\Clients\StartMenuInternet";
const BASE_32: &str = r"SOFTWARE\WOW6432Node\Clients\StartMenuInternet";

pub fn detect() -> Vec<BrowserInstall> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for (root, base) in [(HKEY_CURRENT_USER, BASE), (HKEY_LOCAL_MACHINE, BASE), (HKEY_LOCAL_MACHINE, BASE_32)] {
        for key in subkeys(root, base) {
            let path = format!(r"{base}\{key}");
            let Some(command) = default_value(root, &format!(r"{path}\shell\open\command")) else {
                continue;
            };
            let Some(exe) = exe_from_command(&command) else {
                continue;
            };
            if is_tor_browser(&exe) || !exe.exists() {
                continue;
            }
            // Opera s'enregistre avec un « launcher.exe » générique.
            let family = classify_executable(&exe)
                .or_else(|| key.to_lowercase().contains("opera").then_some(BrowserFamily::Chromium));
            let Some(family) = family else {
                continue;
            };
            if !seen.insert(exe.to_string_lossy().to_lowercase()) {
                continue;
            }
            let name = default_value(root, &path).filter(|n| !n.trim().is_empty()).unwrap_or(key);
            found.push(BrowserInstall { name, exe_path: exe, family });
        }
    }
    found.sort_by_key(|b| b.name.to_lowercase());
    found
}

fn subkeys(root: HKEY, path: &str) -> Vec<String> {
    let mut key = HKEY::default();
    let path = HSTRING::from(path);
    let mut names = Vec::new();
    unsafe {
        if RegOpenKeyExW(root, &path, None, KEY_READ, &mut key) != ERROR_SUCCESS {
            return names;
        }
        let mut index = 0;
        loop {
            let mut buffer = [0u16; 256];
            let mut len = buffer.len() as u32;
            let status = RegEnumKeyExW(key, index, Some(PWSTR(buffer.as_mut_ptr())), &mut len, None, None, None, None);
            if status != ERROR_SUCCESS {
                break;
            }
            names.push(String::from_utf16_lossy(&buffer[..len as usize]));
            index += 1;
        }
        let _ = RegCloseKey(key);
    }
    names
}

/// Valeur par défaut (texte) d'une clé.
fn default_value(root: HKEY, path: &str) -> Option<String> {
    let path = HSTRING::from(path);
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ;
    let mut size = 0u32;
    unsafe {
        if RegGetValueW(root, &path, PCWSTR::null(), flags, None, None, Some(&mut size)) != ERROR_SUCCESS {
            return None;
        }
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        let mut size = (buffer.len() * 2) as u32;
        if RegGetValueW(
            root,
            &path,
            PCWSTR::null(),
            flags,
            None,
            Some(buffer.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        ) != ERROR_SUCCESS
        {
            return None;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..len]))
    }
}
