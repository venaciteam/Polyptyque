//! Informations sur les processus : chemin de l'exécutable et ligne de
//! commande (qui permet de reconnaître le profil d'un navigateur).

use std::ffi::{c_void, OsString};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::core::PWSTR;
use windows::Wdk::System::Threading::{NtQueryInformationProcess, ProcessCommandLineInformation};
use windows::Win32::Foundation::{CloseHandle, HANDLE, UNICODE_STRING};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Poignée de processus fermée automatiquement.
struct ProcessHandle(HANDLE);

impl ProcessHandle {
    fn open(pid: u32) -> Option<Self> {
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().map(ProcessHandle) }
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn image_path(pid: u32) -> Option<PathBuf> {
    let process = ProcessHandle::open(pid)?;
    let mut buffer = vec![0u16; 1024];
    let mut len = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len).ok()?;
    }
    Some(PathBuf::from(OsString::from_wide(&buffer[..len as usize])))
}

pub fn command_line(pid: u32) -> Option<String> {
    let process = ProcessHandle::open(pid)?;
    // Tampon aligné sur 8 octets : le résultat commence par une structure
    // UNICODE_STRING qui pointe vers le texte placé juste après.
    let mut buffer: Vec<u64> = vec![0; 1024];
    for _ in 0..2 {
        let mut needed = 0u32;
        let status = unsafe {
            NtQueryInformationProcess(
                process.0,
                ProcessCommandLineInformation,
                buffer.as_mut_ptr() as *mut c_void,
                (buffer.len() * 8) as u32,
                &mut needed,
            )
        };
        if status.0 >= 0 {
            let text = unsafe {
                let header = &*(buffer.as_ptr() as *const UNICODE_STRING);
                if header.Buffer.is_null() || header.Length == 0 {
                    return Some(String::new());
                }
                std::slice::from_raw_parts(header.Buffer.0, header.Length as usize / 2)
            };
            return Some(String::from_utf16_lossy(text));
        }
        if needed as usize > buffer.len() * 8 {
            buffer = vec![0; needed as usize / 8 + 1];
        } else {
            return None;
        }
    }
    None
}
