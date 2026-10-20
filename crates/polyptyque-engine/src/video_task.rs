//! Fil vidéo d'une source : recherche et capture de la fenêtre, émission des
//! images à cadence constante sur l'horloge commune.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::Sender;
use polyptyque_core::config::Source;
use polyptyque_core::platform::{Nv12Frame, Platform, VideoCapture, WindowHandle};
use polyptyque_core::timeline::{frame_time_hns, frames_due, HNS_PER_SEC};
use polyptyque_core::video::black_nv12;

use crate::resolve;
use crate::shared::{Shared, SourceState};

/// Intervalle de recherche d'une fenêtre absente.
const SEARCH_INTERVAL_HNS: u64 = HNS_PER_SEC;
const MINIMIZED_CHECK_HNS: u64 = HNS_PER_SEC / 2;

pub(crate) struct VideoTask {
    pub platform: Arc<dyn Platform>,
    pub source: Source,
    pub shared: Arc<Shared>,
    pub queue: Sender<Nv12Frame>,
    pub t0_hns: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl VideoTask {
    pub fn run(self) {
        let VideoTask { platform, source, shared, queue, t0_hns, width, height, fps } = self;
        let black: Nv12Frame = Arc::new(black_nv12(width, height));
        let mut latest = black.clone();
        let mut last_sent = black.clone();
        let mut capture: Option<(Box<dyn VideoCapture>, WindowHandle)> = None;
        let mut next_index = 0u64;
        let mut next_search = 0u64;
        let mut next_minimized_check = 0u64;
        // Au-delà d'une seconde d'avance, l'encodeur ne suit plus : on
        // répète l'image précédente plutôt que d'accumuler de la mémoire.
        let max_backlog = fps as usize;

        loop {
            let now = platform.now_hns();
            let stop_at = shared.stop_at();

            if capture.is_none() && stop_at.is_none() && now >= next_search {
                next_search = now + SEARCH_INTERVAL_HNS;
                let windows = platform.list_windows();
                if let Some(window) = resolve::find_window(platform.as_ref(), &source, &windows) {
                    match platform.open_video_capture(window.handle, width, height, source.capture_cursor) {
                        Ok(c) => {
                            log::info!(
                                "[{}] fenêtre rattachée : « {} » ({})",
                                source.name,
                                window.title,
                                window.exe.display()
                            );
                            capture = Some((c, window.handle));
                            shared.clear_note();
                            shared.set_target(Some(window));
                            shared.set_state(SourceState::Connected);
                            next_minimized_check = 0;
                        }
                        Err(e) => {
                            log::warn!("[{}] capture de la fenêtre impossible : {e:#}", source.name);
                            shared.note_error(format!("capture de la fenêtre impossible : {e:#}"));
                        }
                    }
                }
            }

            let mut lost = false;
            if let Some((c, handle)) = capture.as_mut() {
                if !platform.is_window_alive(*handle) {
                    lost = true;
                } else {
                    match c.poll() {
                        Ok(Some(frame)) => {
                            latest = frame;
                            shared.frames_new.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(None) => {}
                        Err(e) => {
                            log::warn!("[{}] capture interrompue : {e:#}", source.name);
                            lost = true;
                        }
                    }
                    if !lost && now >= next_minimized_check {
                        next_minimized_check = now + MINIMIZED_CHECK_HNS;
                        let state = if platform.is_window_minimized(*handle) {
                            SourceState::Minimized
                        } else {
                            SourceState::Connected
                        };
                        shared.set_state(state);
                    }
                }
            }
            if lost {
                log::info!("[{}] fenêtre perdue, image noire jusqu'à son retour", source.name);
                capture = None;
                latest = black.clone();
                shared.set_target(None);
                shared.set_state(SourceState::Lost);
                next_search = now + SEARCH_INTERVAL_HNS;
            }

            // Émission des images dues depuis le départ commun.
            let horizon = stop_at.map_or(now, |stop| stop.min(now));
            let due = if horizon >= t0_hns { frames_due(horizon - t0_hns, fps) } else { 0 };
            while next_index < due {
                let frame = if queue.len() >= max_backlog {
                    shared.frames_dropped.fetch_add(1, Ordering::Relaxed);
                    last_sent.clone()
                } else {
                    latest.clone()
                };
                if queue.send(frame.clone()).is_err() {
                    // L'écriture a cessé : l'erreur est déjà signalée.
                    return;
                }
                last_sent = frame;
                next_index += 1;
            }
            shared.frames_total.store(next_index, Ordering::Relaxed);

            if stop_at.is_some_and(|stop| now >= stop) || shared.has_failed() {
                break;
            }

            let next_time = t0_hns + frame_time_hns(next_index, fps);
            let now = platform.now_hns();
            if next_time > now {
                thread::sleep(Duration::from_nanos((next_time - now) * 100));
            }
        }
        shared.set_state(SourceState::Finished);
    }
}
