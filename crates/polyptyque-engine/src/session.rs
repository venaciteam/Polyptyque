//! Session d'enregistrement : démarrage commun de toutes les sources d'une
//! scène, suivi, arrêt propre.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver};
use polyptyque_core::config::{new_id, Settings, Source, CHANNELS, SAMPLE_RATE};
use polyptyque_core::encoder::{bytes_per_hour, recording_args, EncoderChoice, RecordingSpec};
use polyptyque_core::naming::{self, NameParts};
use polyptyque_core::platform::{Nv12Frame, Platform};
use polyptyque_core::timeline::HNS_PER_SEC;

use crate::audio_task::AudioTask;
use crate::shared::{Shared, SourceState};
use crate::video_task::VideoTask;
use crate::writer;

/// Délai accordé aux encodeurs pour vider leurs files à l'arrêt.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(120);
/// Délai accordé à FFmpeg pour finaliser un fichier.
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(60);
/// Le son est « actif » s'il a été entendu dans ce délai.
const RECENT_SOUND_HNS: u64 = 2 * HNS_PER_SEC;

pub struct SessionPlan {
    pub scene_name: String,
    pub sources: Vec<Source>,
    pub settings: Settings,
    pub encoder: EncoderChoice,
    pub ffmpeg: PathBuf,
    /// Dossier des journaux de cette session.
    pub log_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SourceStatus {
    pub name: String,
    pub state: SourceState,
    pub frames_total: u64,
    pub frames_new: u64,
    pub frames_dropped: u64,
    /// Niveau crête depuis le dernier appel (0 à 1).
    pub audio_level: f32,
    pub audio_recent: bool,
    pub output: PathBuf,
    pub file_size: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SourceSummary {
    pub name: String,
    pub output: PathBuf,
    pub file_size: u64,
    pub frames_total: u64,
    pub frames_dropped: u64,
    pub error: Option<String>,
    pub log: PathBuf,
}

struct SourceRuntime {
    name: String,
    output: PathBuf,
    log: PathBuf,
    shared: Arc<Shared>,
    encoder: Option<Child>,
    capture_threads: Vec<JoinHandle<()>>,
    /// Fil d'écriture et chemin de son tube.
    writers: Vec<(JoinHandle<()>, String)>,
}

pub struct Session {
    platform: Arc<dyn Platform>,
    sources: Vec<SourceRuntime>,
    t0_hns: u64,
    output_dir: PathBuf,
    bytes_per_hour: u64,
    started: Instant,
}

impl Session {
    pub fn start(platform: Arc<dyn Platform>, plan: SessionPlan) -> Result<Session> {
        let settings = &plan.settings;
        fs::create_dir_all(&settings.output_dir)
            .with_context(|| format!("création du dossier {}", settings.output_dir.display()))?;
        fs::create_dir_all(&plan.log_dir).with_context(|| format!("création du dossier {}", plan.log_dir.display()))?;

        let (date, time) = naming::now_parts();
        let tag = format!("polyptyque-{}-{}", std::process::id(), new_id());
        let mut taken = HashSet::new();
        platform.begin_recording_session();

        // 1. Encodeurs prêts à recevoir : tubes, FFmpeg, fils d'écriture.
        let mut prepared = Vec::new();
        for (index, source) in plan.sources.iter().enumerate() {
            let stem = naming::render(
                &settings.filename_template,
                &NameParts { date: &date, time: &time, scene: &plan.scene_name, source: &source.name },
            );
            let output = naming::unique_path(&settings.output_dir, &stem, "mkv", &taken);
            taken.insert(output.clone());
            let log = plan.log_dir.join(format!("{:02}-{}.ffmpeg.log", index + 1, naming::sanitize(&source.name)));
            let shared = Arc::new(Shared::new());
            let (video_tx, video_rx) = unbounded::<Nv12Frame>();
            let (audio_tx, audio_rx) = unbounded::<Vec<u8>>();
            let mut runtime = SourceRuntime {
                name: source.name.clone(),
                output,
                log,
                shared: shared.clone(),
                encoder: None,
                capture_threads: Vec::new(),
                writers: Vec::new(),
            };
            match start_encoder(
                platform.as_ref(),
                &plan,
                source,
                &format!("{tag}-{index}"),
                &runtime,
                video_rx,
                audio_rx,
            ) {
                Ok((child, writers)) => {
                    runtime.encoder = Some(child);
                    runtime.writers = writers;
                }
                Err(e) => shared.fail(format!("démarrage de l'encodeur impossible : {e:#}")),
            }
            prepared.push((runtime, source.clone(), video_tx, audio_tx));
        }

        // 2. Départ commun de toutes les captures.
        let t0_hns = platform.now_hns();
        let video = &settings.video;
        let mut sources = Vec::new();
        for (mut runtime, source, video_tx, audio_tx) in prepared {
            if !runtime.shared.has_failed() {
                let video_task = VideoTask {
                    platform: platform.clone(),
                    source: source.clone(),
                    shared: runtime.shared.clone(),
                    queue: video_tx,
                    t0_hns,
                    width: video.width,
                    height: video.height,
                    fps: video.fps,
                };
                let audio_task = AudioTask {
                    platform: platform.clone(),
                    name: source.name.clone(),
                    shared: runtime.shared.clone(),
                    queue: audio_tx,
                    t0_hns,
                };
                for (suffix, spawned) in [
                    (
                        "vidéo",
                        thread::Builder::new().name(format!("{}-video", source.name)).spawn(move || video_task.run()),
                    ),
                    (
                        "son",
                        thread::Builder::new().name(format!("{}-audio", source.name)).spawn(move || audio_task.run()),
                    ),
                ] {
                    match spawned {
                        Ok(handle) => runtime.capture_threads.push(handle),
                        Err(e) => runtime.shared.fail(format!("fil {suffix} : {e}")),
                    }
                }
            }
            log::info!("[{}] enregistrement vers {}", runtime.name, runtime.output.display());
            sources.push(runtime);
        }

        Ok(Session {
            bytes_per_hour: bytes_per_hour(video.bitrate_kbps, settings.audio.bitrate_kbps) * sources.len() as u64,
            platform,
            sources,
            t0_hns,
            output_dir: settings.output_dir.clone(),
            started: Instant::now(),
        })
    }

    pub fn elapsed(&self) -> Duration {
        Duration::from_nanos(self.platform.now_hns().saturating_sub(self.t0_hns) * 100)
    }

    pub fn started_at(&self) -> Instant {
        self.started
    }

    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }

    /// Débit total estimé, en octets par heure.
    pub fn bytes_per_hour(&self) -> u64 {
        self.bytes_per_hour
    }

    pub fn free_space(&self) -> Option<u64> {
        self.platform.free_disk_space(&self.output_dir)
    }

    pub fn status(&mut self) -> Vec<SourceStatus> {
        let now = self.platform.now_hns();
        self.sources
            .iter_mut()
            .map(|source| {
                let shared = &source.shared;
                if let Some(child) = source.encoder.as_mut() {
                    if let Ok(Some(status)) = child.try_wait() {
                        if shared.stop_at().is_none() && !shared.has_failed() {
                            shared.fail(format!("l'encodeur s'est arrêté ({status}) : {}", log_tail(&source.log)));
                        }
                    }
                }
                let last_sound = shared.audio_last_sound_hns.load(Ordering::Relaxed);
                SourceStatus {
                    name: source.name.clone(),
                    state: shared.state(),
                    frames_total: shared.frames_total.load(Ordering::Relaxed),
                    frames_new: shared.frames_new.load(Ordering::Relaxed),
                    frames_dropped: shared.frames_dropped.load(Ordering::Relaxed),
                    audio_level: shared.take_peak(),
                    audio_recent: last_sound != 0 && now.saturating_sub(last_sound) < RECENT_SOUND_HNS,
                    output: source.output.clone(),
                    file_size: fs::metadata(&source.output).map(|m| m.len()).unwrap_or(0),
                    error: shared.error(),
                }
            })
            .collect()
    }

    /// Arrête toutes les sources au même instant et attend que chaque
    /// fichier soit correctement fermé.
    pub fn stop(mut self) -> Vec<SourceSummary> {
        let stop_at = self.platform.now_hns();
        for source in &self.sources {
            source.shared.request_stop(stop_at);
        }
        for source in &mut self.sources {
            for handle in source.capture_threads.drain(..) {
                let _ = handle.join();
            }
        }
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        for source in &mut self.sources {
            for (handle, pipe_path) in std::mem::take(&mut source.writers) {
                wait_writer(handle, &pipe_path, &mut source.encoder, deadline);
            }
            if let Some(child) = source.encoder.as_mut() {
                wait_encoder(child, FINALIZE_TIMEOUT);
            }
        }
        self.platform.end_recording_session();

        self.sources
            .iter()
            .map(|source| SourceSummary {
                name: source.name.clone(),
                output: source.output.clone(),
                file_size: fs::metadata(&source.output).map(|m| m.len()).unwrap_or(0),
                frames_total: source.shared.frames_total.load(Ordering::Relaxed),
                frames_dropped: source.shared.frames_dropped.load(Ordering::Relaxed),
                error: source.shared.error(),
                log: source.log.clone(),
            })
            .collect()
    }
}

type Writers = Vec<(JoinHandle<()>, String)>;

fn start_encoder(
    platform: &dyn Platform,
    plan: &SessionPlan,
    source: &Source,
    pipe_prefix: &str,
    runtime: &SourceRuntime,
    video_rx: Receiver<Nv12Frame>,
    audio_rx: Receiver<Vec<u8>>,
) -> Result<(Child, Writers)> {
    let video_pipe = platform.create_stream_pipe(&format!("{pipe_prefix}-video"))?;
    let audio_pipe = platform.create_stream_pipe(&format!("{pipe_prefix}-audio"))?;
    let video_path = video_pipe.ffmpeg_path();
    let audio_path = audio_pipe.ffmpeg_path();
    let settings = &plan.settings;
    let args = recording_args(&RecordingSpec {
        video_input: &video_path,
        audio_input: &audio_path,
        width: settings.video.width,
        height: settings.video.height,
        fps: settings.video.fps,
        sample_rate: SAMPLE_RATE,
        channels: CHANNELS,
        encoder: plan.encoder,
        video_bitrate_kbps: settings.video.bitrate_kbps,
        audio_bitrate_kbps: settings.audio.bitrate_kbps,
        title: &source.name,
        output: &runtime.output,
    });

    let mut log = File::create(&runtime.log).with_context(|| format!("création de {}", runtime.log.display()))?;
    writeln!(log, "{} {}", plan.ffmpeg.display(), args.join(" "))?;
    log.flush()?;
    let mut command = Command::new(&plan.ffmpeg);
    command.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(log));
    platform.prepare_encoder_process(&mut command);
    let mut child = command.spawn().with_context(|| format!("lancement de {}", plan.ffmpeg.display()))?;

    let shared = &runtime.shared;
    let writers = writer::spawn(format!("{}-video-pipe", source.name), video_pipe, video_rx, shared.clone(), "image")
        .and_then(|video| {
            writer::spawn(format!("{}-audio-pipe", source.name), audio_pipe, audio_rx, shared.clone(), "son")
                .map(|audio| vec![(video, video_path), (audio, audio_path)])
        });
    match writers {
        Ok(writers) => Ok((child, writers)),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e.into())
        }
    }
}

/// Attend la fin d'un fil d'écriture. Si FFmpeg s'est arrêté ou ne lit plus
/// avant l'échéance, le fil est débloqué pour ne jamais rester suspendu.
fn wait_writer(handle: JoinHandle<()>, pipe_path: &str, encoder: &mut Option<Child>, deadline: Instant) {
    let mut unblocked = false;
    while !handle.is_finished() {
        let encoder_gone = match encoder.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(Some(_))),
            None => true,
        };
        if !unblocked && (encoder_gone || Instant::now() >= deadline) {
            log::warn!("l'encodeur ne lit plus {pipe_path} : arrêt forcé");
            if let Some(child) = encoder.as_mut() {
                let _ = child.kill();
            }
            // Débloque un fil encore en attente de connexion.
            let _ = OpenOptions::new().read(true).open(pipe_path);
            unblocked = true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = handle.join();
}

fn wait_encoder(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                log::info!("encodeur terminé : {status}");
                return;
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            _ => {
                log::warn!("l'encodeur ne se termine pas : arrêt forcé");
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }
    }
}

/// Dernières lignes utiles du journal de FFmpeg, pour expliquer un échec.
fn log_tail(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" | ");
    let mut tail: String = tail.chars().take(400).collect();
    if tail.is_empty() {
        tail = "journal vide".into();
    }
    tail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_tail_keeps_last_lines() {
        let path = std::env::temp_dir().join(format!("polyptyque-log-{}.txt", new_id()));
        fs::write(&path, "commande\n\nligne 1\nligne 2\nligne 3\n").unwrap();
        assert_eq!(log_tail(&path), "ligne 1 | ligne 2 | ligne 3");
        fs::remove_file(path).unwrap();
    }
}
