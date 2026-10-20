//! Énumération des fenêtres principales.

use std::ffi::c_void;

use polyptyque_core::platform::{WindowHandle, WindowInfo};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
    IsWindow, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

use crate::process;

/// Fenêtres du bureau et de la barre des tâches, jamais utiles à capturer.
const IGNORED_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"];

pub fn hwnd(handle: WindowHandle) -> HWND {
    HWND(handle as *mut c_void)
}

/// Fenêtres principales visibles, dans l'ordre d'empilement (la plus
/// récemment utilisée en premier).
pub fn list(own_pid: u32) -> Vec<WindowInfo> {
    let mut handles: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut handles as *mut Vec<HWND> as isize));
    }
    handles.into_iter().filter_map(describe).filter(|w| w.pid != own_pid).collect()
}

unsafe extern "system" fn collect(window: HWND, lparam: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    handles.push(window);
    TRUE
}

fn describe(window: HWND) -> Option<WindowInfo> {
    unsafe {
        if !IsWindowVisible(window).as_bool() || is_cloaked(window) {
            return None;
        }
        let ex_style = GetWindowLongW(window, GWL_EXSTYLE) as u32;
        if ex_style & WS_EX_TOOLWINDOW.0 != 0 {
            return None;
        }
        if let Ok(owner) = GetWindow(window, GW_OWNER) {
            if !owner.is_invalid() {
                return None;
            }
        }
        let title = text(window);
        if title.trim().is_empty() {
            return None;
        }
        let class = class_name(window);
        if IGNORED_CLASSES.contains(&class.as_str()) {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let exe = process::image_path(pid).unwrap_or_default();
        Some(WindowInfo { handle: window.0 as WindowHandle, pid, title, class, exe })
    }
}

/// Fenêtres « masquées » par Windows (applications suspendues, bureaux
/// virtuels…) : visibles en apparence mais jamais affichées.
fn is_cloaked(window: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let result = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    result.is_ok() && cloaked != 0
}

fn text(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    let len = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

fn class_name(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

pub fn is_alive(handle: WindowHandle) -> bool {
    unsafe { IsWindow(Some(hwnd(handle))).as_bool() }
}

pub fn is_minimized(handle: WindowHandle) -> bool {
    unsafe { IsIconic(hwnd(handle)).as_bool() }
}
