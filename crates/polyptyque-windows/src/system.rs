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

/// Demande à Windows de rester éveillé (système et écran). L'état
/// « éveillé » de Windows est attaché au fil qui le demande : un fil dédié
/// permet de le prendre et de le rendre depuis n'importe quel autre fil.
/// Windows reste éveillé tant qu'un fil au moins le demande : chaque besoin
/// (enregistrement, programmation) a donc le sien, et la fin de l'un ne
/// relâche pas l'autre.
struct KeepAwake {
    name: &'static str,
    thread: Mutex<Option<(mpsc::Sender<()>, thread::JoinHandle<()>)>>,
}

impl KeepAwake {
    const fn new(name: &'static str) -> Self {
        Self { name, thread: Mutex::new(None) }
    }

    fn hold(&self) {
        let mut guard = self.thread.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_some() {
            return;
        }
        let (stop, wait) = mpsc::channel::<()>();
        let spawned = thread::Builder::new().name(self.name.into()).spawn(move || {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
            }
            // Attend la fin du besoin (ou la fermeture du canal).
            let _ = wait.recv();
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        });
        match spawned {
            Ok(handle) => *guard = Some((stop, handle)),
            Err(e) => log::warn!("{} indisponible : {e}", self.name),
        }
    }

    fn release(&self) {
        let taken = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some((stop, handle)) = taken {
            let _ = stop.send(());
            let _ = handle.join();
        }
    }
}

static RECORDING: KeepAwake = KeepAwake::new("anti-veille");
static SCHEDULES: KeepAwake = KeepAwake::new("anti-veille des programmations");

/// Empêche la mise en veille (système et écran) et passe les minuteries à
/// 1 ms pour un rythme d'images régulier.
pub fn begin_session() {
    unsafe {
        timeBeginPeriod(1);
    }
    RECORDING.hold();
}

pub fn end_session() {
    unsafe {
        timeEndPeriod(1);
    }
    RECORDING.release();
}

/// Tant qu'une programmation est active : mise en veille automatique (et
/// extinction de l'écran) empêchée, comme pendant un enregistrement.
pub fn keep_awake_for_schedules(on: bool) {
    if on {
        SCHEDULES.hold();
    } else {
        SCHEDULES.release();
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
