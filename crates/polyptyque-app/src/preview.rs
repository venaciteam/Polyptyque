//! Aperçus en direct des sources.
//!
//! Chaque source a sa propre capture en petite définition, indépendante de
//! l'enregistrement : la carte graphique réduit l'image avant la lecture,
//! ce qui rend les aperçus très peu coûteux. Ils fonctionnent aussi bien
//! hors enregistrement que pendant.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use polyptyque_core::config::Source;
use polyptyque_core::platform::{Platform, VideoCapture, WindowHandle};
use polyptyque_core::video::nv12_to_rgba;
use polyptyque_engine::resolve;
use slint::{Rgba8Pixel, SharedPixelBuffer};

pub const PREVIEW_WIDTH: u32 = 384;
pub const PREVIEW_HEIGHT: u32 = 216;
/// Environ 10 images par seconde : fluide à l'œil, négligeable en calcul.
const FRAME_INTERVAL: Duration = Duration::from_millis(100);
const SEARCH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    Searching,
    Live,
    Minimized,
    /// Fenêtre introuvable : elle peut être ouverte depuis l'aperçu.
    Missing,
    Failed(String),
}

struct Slot {
    frame: Option<SharedPixelBuffer<Rgba8Pixel>>,
    state: PreviewState,
    state_changed: bool,
}

pub struct PreviewWorker {
    pub source_id: String,
    slot: Arc<Mutex<Slot>>,
    stop: Arc<AtomicBool>,
}

impl PreviewWorker {
    /// Nouvelle image et nouvel état depuis le dernier appel.
    pub fn take_update(&self) -> (Option<SharedPixelBuffer<Rgba8Pixel>>, Option<PreviewState>) {
        let mut slot = lock(&self.slot);
        let state = std::mem::take(&mut slot.state_changed).then(|| slot.state.clone());
        (slot.frame.take(), state)
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        // Le fil s'arrête de lui-même au prochain tour, sans bloquer l'interface.
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub struct PreviewManager {
    workers: Vec<PreviewWorker>,
}

impl PreviewManager {
    pub fn start(platform: &Arc<dyn Platform>, sources: &[Source]) -> Self {
        let workers = sources
            .iter()
            .filter_map(|source| {
                let slot =
                    Arc::new(Mutex::new(Slot { frame: None, state: PreviewState::Searching, state_changed: true }));
                let stop = Arc::new(AtomicBool::new(false));
                let (platform, source_clone, slot_clone, stop_clone) =
                    (platform.clone(), source.clone(), slot.clone(), stop.clone());
                thread::Builder::new()
                    .name(format!("{}-apercu", source.name))
                    .spawn(move || run(platform, source_clone, slot_clone, stop_clone))
                    .map_err(|e| log::warn!("aperçu de {} indisponible : {e}", source.name))
                    .ok()
                    .map(|_| PreviewWorker { source_id: source.id.clone(), slot, stop })
            })
            .collect();
        Self { workers }
    }

    pub fn workers(&self) -> &[PreviewWorker] {
        &self.workers
    }

    pub fn stop(&mut self) {
        self.workers.clear();
    }
}

fn run(platform: Arc<dyn Platform>, source: Source, slot: Arc<Mutex<Slot>>, stop: Arc<AtomicBool>) {
    let set_state = |state: PreviewState| {
        let mut slot = lock(&slot);
        if slot.state != state {
            slot.state = state;
            slot.state_changed = true;
        }
    };
    let mut capture: Option<(Box<dyn VideoCapture>, WindowHandle)> = None;
    let mut next_search = Instant::now();
    let mut next_minimized_check = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if capture.is_none() && now >= next_search {
            next_search = now + SEARCH_INTERVAL;
            let windows = platform.list_windows();
            match resolve::find_window(platform.as_ref(), &source, &windows) {
                Some(window) => {
                    match platform.open_video_capture(
                        window.handle,
                        PREVIEW_WIDTH,
                        PREVIEW_HEIGHT,
                        source.capture_cursor,
                    ) {
                        Ok(c) => {
                            capture = Some((c, window.handle));
                            set_state(PreviewState::Live);
                        }
                        Err(e) => set_state(PreviewState::Failed(format!("{e:#}"))),
                    }
                }
                None => set_state(PreviewState::Missing),
            }
        }

        let mut lost = false;
        if let Some((c, handle)) = capture.as_mut() {
            if !platform.is_window_alive(*handle) {
                lost = true;
            } else {
                if now >= next_minimized_check {
                    next_minimized_check = now + Duration::from_millis(500);
                    let minimized = platform.is_window_minimized(*handle);
                    set_state(if minimized { PreviewState::Minimized } else { PreviewState::Live });
                }
                match c.poll() {
                    Ok(Some(frame)) => {
                        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(PREVIEW_WIDTH, PREVIEW_HEIGHT);
                        nv12_to_rgba(&frame, PREVIEW_WIDTH as usize, PREVIEW_HEIGHT as usize, buffer.make_mut_bytes());
                        lock(&slot).frame = Some(buffer);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        log::debug!("aperçu de {} interrompu : {e:#}", source.name);
                        lost = true;
                    }
                }
            }
        }
        if lost {
            capture = None;
            set_state(PreviewState::Missing);
            next_search = now + SEARCH_INTERVAL;
        }
        thread::sleep(FRAME_INTERVAL);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
