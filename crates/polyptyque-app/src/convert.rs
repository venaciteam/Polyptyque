//! File de conversion MKV → MP4, traitée un fichier à la fois sur un fil
//! dédié : l'interface reste libre, et un enregistrement peut démarrer
//! pendant une conversion.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use polyptyque_core::platform::Platform;
use polyptyque_engine::{probe, remux};

/// Marge d'espace disque gardée libre en plus de la taille du fichier.
const DISK_MARGIN: u64 = 1_000_000_000;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

pub enum ConversionEvent {
    Started {
        input: PathBuf,
    },
    Progress(f32),
    Finished {
        input: PathBuf,
        result: Result<PathBuf, String>,
    },
    /// File vide, y compris après une annulation.
    Idle,
}

pub struct Converter {
    jobs: Option<Sender<PathBuf>>,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Converter {
    pub fn start(platform: Arc<dyn Platform>, on_event: impl Fn(ConversionEvent) + Send + 'static) -> Self {
        let (jobs, queue) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let worker = thread::Builder::new()
            .name("conversion-mp4".into())
            .spawn(move || run(platform, queue, flag, on_event))
            .map_err(|e| log::error!("conversion MP4 indisponible : {e}"))
            .ok();
        Self { jobs: Some(jobs), cancel, worker }
    }

    pub fn enqueue(&self, file: PathBuf) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(file);
        }
    }

    /// Interrompt le fichier en cours (le fichier partiel est effacé) et
    /// abandonne ceux qui attendent.
    pub fn cancel_all(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Drop for Converter {
    fn drop(&mut self) {
        self.cancel_all();
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run(
    platform: Arc<dyn Platform>,
    queue: Receiver<PathBuf>,
    cancel: Arc<AtomicBool>,
    on_event: impl Fn(ConversionEvent),
) {
    while let Ok(first) = queue.recv() {
        let mut next = Some(first);
        while let Some(input) = next {
            if !cancel.load(Ordering::SeqCst) {
                on_event(ConversionEvent::Started { input: input.clone() });
                let result = convert_one(platform.as_ref(), &input, &cancel, &on_event).map_err(|e| format!("{e:#}"));
                on_event(ConversionEvent::Finished { input, result });
            }
            next = queue.try_recv().ok();
        }
        // Une annulation ne vaut que pour les fichiers déjà demandés.
        cancel.store(false, Ordering::SeqCst);
        on_event(ConversionEvent::Idle);
    }
}

fn convert_one(
    platform: &dyn Platform,
    input: &Path,
    cancel: &AtomicBool,
    on_event: &dyn Fn(ConversionEvent),
) -> Result<PathBuf> {
    let ffmpeg = probe::locate_ffmpeg()
        .ok_or_else(|| anyhow!("FFmpeg introuvable : ffmpeg.exe doit se trouver à côté de polyptyque.exe"))?;
    let size = fs::metadata(input).map_err(|e| anyhow!("fichier inaccessible : {e}"))?.len();
    let dir = input.parent().unwrap_or(Path::new("."));
    if let Some(free) = platform.free_disk_space(dir) {
        if free < size.saturating_add(DISK_MARGIN) {
            bail!("espace disque insuffisant pour la copie MP4");
        }
    }
    let mut last = Instant::now() - PROGRESS_INTERVAL;
    remux::convert(
        platform,
        &ffmpeg,
        input,
        &mut |progress| {
            if last.elapsed() >= PROGRESS_INTERVAL || progress >= 1.0 {
                last = Instant::now();
                on_event(ConversionEvent::Progress(progress));
            }
        },
        cancel,
    )
}
