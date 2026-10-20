//! Horloge, mise en veille, minuteries et espace disque.

use std::path::Path;
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;

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

/// Fil qui maintient l'ordinateur éveillé. L'état « éveillé » de Windows est
/// attaché au fil qui le demande : un fil dédié permet de démarrer et
/// d'arrêter un enregistrement depuis n'importe quel autre fil.
static KEEP_AWAKE: Mutex<Option<(mpsc::Sender<()>, thread::JoinHandle<()>)>> = Mutex::new(None);

/// Empêche la mise en veille (système et écran) et passe les minuteries à
/// 1 ms pour un rythme d'images régulier.
pub fn begin_session() {
    unsafe {
        timeBeginPeriod(1);
    }
    let mut guard = KEEP_AWAKE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_some() {
        return;
    }
    let (stop, wait) = mpsc::channel::<()>();
    let spawned = thread::Builder::new().name("anti-veille".into()).spawn(move || {
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        }
        // Attend la fin de l'enregistrement (ou la fermeture du canal).
        let _ = wait.recv();
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    });
    match spawned {
        Ok(handle) => *guard = Some((stop, handle)),
        Err(e) => log::warn!("anti-veille indisponible : {e}"),
    }
}

pub fn end_session() {
    unsafe {
        timeEndPeriod(1);
    }
    let taken = KEEP_AWAKE.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some((stop, handle)) = taken {
        let _ = stop.send(());
        let _ = handle.join();
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
