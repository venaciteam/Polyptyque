//! Horloge, mise en veille, minuteries et espace disque.

use std::path::Path;
use std::sync::OnceLock;

use windows::core::HSTRING;
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};

/// Horloge haute précision (QPC) en centaines de nanosecondes : c'est la
/// même base de temps que les horodatages audio de WASAPI.
pub fn now_hns() -> u64 {
    static FREQUENCY: OnceLock<u64> = OnceLock::new();
    let frequency = *FREQUENCY.get_or_init(|| {
        let mut f = 0i64;
        unsafe {
            let _ = QueryPerformanceFrequency(&mut f);
        }
        f.max(1) as u64
    });
    let mut counter = 0i64;
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
    }
    (counter as u128 * 10_000_000 / frequency as u128) as u64
}

/// Empêche la mise en veille (système et écran) et passe les minuteries à
/// 1 ms pour un rythme d'images régulier.
pub fn begin_session() {
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        timeBeginPeriod(1);
    }
}

pub fn end_session() {
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS);
        timeEndPeriod(1);
    }
}

pub fn free_space(path: &Path) -> Option<u64> {
    // Le dossier de sortie peut ne pas encore exister.
    let mut dir = path;
    while !dir.exists() {
        dir = dir.parent()?;
    }
    let dir = HSTRING::from(dir.as_os_str());
    let mut available = 0u64;
    unsafe { GetDiskFreeSpaceExW(&dir, Some(&mut available), None, None).ok()? };
    Some(available)
}
