//! Superviseur d'encodage d'une source : lance FFmpeg, l'alimente par
//! deux tubes et veille sur lui jusqu'à la fermeture du fichier.
//!
//! Si FFmpeg s'arrête ou se fige pendant l'enregistrement, la capture
//! continue et un nouveau FFmpeg reprend dans un nouveau fichier
//! (`…_partie2.mkv`), image et son recalés au même instant. Le fichier
//! précédent reste lisible (c'est l'intérêt du MKV). Des échecs répétés
//! dès le démarrage arrêtent la source pour ne pas tourner en boucle.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use polyptyque_core::config::{CHANNELS, SAMPLE_RATE};
use polyptyque_core::encoder::{
    audio_recording_args, recording_args, AudioRecordingSpec, EncoderChoice, RecordingSpec,
};
use polyptyque_core::naming;
use polyptyque_core::platform::{Nv12Frame, Platform};

use crate::shared::Shared;
use crate::writer::{self, PartControl, WriterJob};

/// Un encodeur qui n'avance plus pendant ce délai est arrêté (puis relancé
/// pendant l'enregistrement). À l'arrêt, tant qu'il avance, on le laisse
/// vider sa file, aussi longue soit-elle : rien de ce qui a été capturé
/// n'est perdu.
const STALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Délai accordé à FFmpeg pour finaliser un fichier.
const FINALIZE_TIMEOUT: Duration = Duration::from_secs(60);
/// Une partie qui s'arrête avant ce délai compte comme un échec au
/// démarrage.
const QUICK_FAILURE: Duration = Duration::from_secs(20);
/// Échecs au démarrage tolérés d'affilée avant d'abandonner la source.
const MAX_QUICK_FAILURES: u32 = 3;
const SUPERVISION_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) struct OutputPlan {
    /// Image et son, ou son seul (micro).
    pub video: bool,
    pub ffmpeg: PathBuf,
    pub encoder: EncoderChoice,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_bitrate_kbps: u32,
    pub audio_bitrate_kbps: u32,
    /// Nom de la source, inscrit dans les fichiers.
    pub title: String,
    /// Fichier de la première partie.
    pub first_output: PathBuf,
    /// Journal de FFmpeg, commun à toutes les parties.
    pub log: PathBuf,
    /// Préfixe unique des tubes nommés de cette source.
    pub pipe_prefix: String,
}

pub(crate) struct OutputTask {
    pub platform: Arc<dyn Platform>,
    pub plan: OutputPlan,
    pub shared: Arc<Shared>,
    /// Images à encoder (aucune pour un fichier de son seul).
    pub video: Option<Receiver<Nv12Frame>>,
    pub audio: Receiver<Vec<u8>>,
}

enum PartEnd {
    /// Fichier fermé normalement (arrêt demandé).
    Closed,
    /// L'encodeur s'est arrêté ou figé pendant l'enregistrement.
    Crashed(String),
}

impl OutputTask {
    pub fn run(self) {
        let OutputTask { platform, plan, shared, video, audio } = self;
        let mut part = 1u32;
        let mut quick_failures = 0u32;
        loop {
            let output = if part == 1 { plan.first_output.clone() } else { part_path(&plan.first_output, part) };
            let (video_start, audio_start) = if part == 1 {
                (0, 0)
            } else if plan.video {
                resume_positions(
                    shared.video_position.load(Ordering::SeqCst),
                    shared.audio_position.load(Ordering::SeqCst),
                    plan.fps,
                )
            } else {
                // Son seul : il reprend là où il en est.
                let frame_bytes = CHANNELS as u64 * 4;
                (0, shared.audio_position.load(Ordering::SeqCst).div_ceil(frame_bytes) * frame_bytes)
            };
            let resume_seconds = if plan.video {
                video_start / plan.fps.max(1) as u64
            } else {
                audio_start / (CHANNELS as u64 * 4) / SAMPLE_RATE as u64
            };
            shared.part.store(part, Ordering::Relaxed);
            shared.push_output(output.clone());
            if part > 1 {
                log::info!(
                    "[{}] partie {part} : {} (reprise à {} de l'enregistrement de cette source)",
                    plan.title,
                    output.display(),
                    clock(resume_seconds)
                );
            }

            let started = Instant::now();
            let streams = Streams { video: video.as_ref(), audio: &audio, video_start, audio_start, resume_seconds };
            let end = match Part::start(platform.as_ref(), &plan, &shared, &output, part, streams) {
                Ok(running) => running.supervise(&shared, &plan.log),
                Err(e) => PartEnd::Crashed(format!("démarrage de l'encodeur impossible : {e:#}")),
            };
            let reason = match end {
                PartEnd::Closed => return,
                PartEnd::Crashed(reason) => reason,
            };
            if !output.exists() || std::fs::metadata(&output).is_ok_and(|m| m.len() == 0) {
                // Rien d'écrit : on ne garde pas de fichier vide.
                let _ = std::fs::remove_file(&output);
                shared.remove_output(&output);
            }
            if stopping(&shared) {
                log::warn!("[{}] encodeur arrêté pendant la finalisation : {reason}", plan.title);
                return;
            }
            quick_failures = if started.elapsed() < QUICK_FAILURE { quick_failures + 1 } else { 1 };
            if quick_failures >= MAX_QUICK_FAILURES {
                shared.fail(format!("l'encodeur s'arrête dès son démarrage : {reason}"));
                return;
            }
            part += 1;
            log::warn!("[{}] {reason} ; relance dans un nouveau fichier (partie {part})", plan.title);
            shared.note_error(format!("Encodeur relancé : la suite est dans la partie {part}"));
            // Courte pause, plus longue si les échecs se répètent.
            let resume_at = Instant::now() + Duration::from_secs(quick_failures as u64);
            while Instant::now() < resume_at {
                if stopping(&shared) {
                    return;
                }
                thread::sleep(SUPERVISION_INTERVAL);
            }
        }
    }
}

fn stopping(shared: &Shared) -> bool {
    shared.stop_at().is_some() || shared.capture_abandoned()
}

/// Flux à brancher sur une nouvelle partie, et point de reprise.
struct Streams<'a> {
    video: Option<&'a Receiver<Nv12Frame>>,
    audio: &'a Receiver<Vec<u8>>,
    video_start: u64,
    audio_start: u64,
    /// Position de la reprise depuis le départ de la source, en secondes.
    resume_seconds: u64,
}

/// Une partie de fichier en cours : FFmpeg et ses fils d'écriture.
struct Part {
    child: Child,
    writers: Vec<(JoinHandle<()>, String)>,
    control: Arc<PartControl>,
}

impl Part {
    fn start(
        platform: &dyn Platform,
        plan: &OutputPlan,
        shared: &Arc<Shared>,
        output: &Path,
        part: u32,
        streams: Streams,
    ) -> Result<Part> {
        let prefix = format!("{}-p{part}", plan.pipe_prefix);
        let audio_pipe = platform.create_stream_pipe(&format!("{prefix}-audio"))?;
        let audio_path = audio_pipe.ffmpeg_path();
        let video = match streams.video {
            Some(queue) => {
                let pipe = platform.create_stream_pipe(&format!("{prefix}-video"))?;
                let path = pipe.ffmpeg_path();
                Some((pipe, path, queue))
            }
            None => None,
        };
        let title = if part == 1 {
            plan.title.clone()
        } else {
            format!("{} · partie {part} (reprise à {})", plan.title, clock(streams.resume_seconds))
        };
        let args = match &video {
            Some((_, video_path, _)) => recording_args(&RecordingSpec {
                video_input: video_path,
                audio_input: &audio_path,
                width: plan.width,
                height: plan.height,
                fps: plan.fps,
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
                encoder: plan.encoder,
                video_bitrate_kbps: plan.video_bitrate_kbps,
                audio_bitrate_kbps: plan.audio_bitrate_kbps,
                title: &title,
                output,
            }),
            None => audio_recording_args(&AudioRecordingSpec {
                input: &audio_path,
                sample_rate: SAMPLE_RATE,
                channels: CHANNELS,
                bitrate_kbps: plan.audio_bitrate_kbps,
                title: &title,
                output,
            }),
        };

        let mut log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&plan.log)
            .with_context(|| format!("ouverture de {}", plan.log.display()))?;
        writeln!(log, "--- partie {part} ---\n{} {}", plan.ffmpeg.display(), args.join(" "))?;
        log.flush()?;
        let mut command = Command::new(&plan.ffmpeg);
        command.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(log));
        platform.prepare_encoder_process(&mut command);
        let child = command.spawn().with_context(|| format!("lancement de {}", plan.ffmpeg.display()))?;

        let mut running = Part { child, writers: Vec::new(), control: Arc::new(PartControl::default()) };
        let spawn_all = || -> std::io::Result<()> {
            if let Some((pipe, path, queue)) = video {
                let job = WriterJob {
                    thread_name: format!("{}-video-pipe", plan.title),
                    pipe,
                    queue: queue.clone(),
                    position: shared.video_position.clone(),
                    start_at: streams.video_start,
                    label: "image",
                };
                running.writers.push((writer::spawn(job, shared.clone(), running.control.clone())?, path));
            }
            let job = WriterJob {
                thread_name: format!("{}-audio-pipe", plan.title),
                pipe: audio_pipe,
                queue: streams.audio.clone(),
                position: shared.audio_position.clone(),
                start_at: streams.audio_start,
                label: "son",
            };
            running.writers.push((writer::spawn(job, shared.clone(), running.control.clone())?, audio_path));
            Ok(())
        };
        if let Err(e) = spawn_all() {
            let _ = running.child.kill();
            let _ = running.child.wait();
            running.abandon_writers();
            return Err(e.into());
        }
        Ok(running)
    }

    /// Veille sur la partie jusqu'à la fermeture normale du fichier ou
    /// l'arrêt de l'encodeur.
    fn supervise(mut self, shared: &Shared, log: &Path) -> PartEnd {
        let mut last_bytes = shared.bytes_sent.load(Ordering::Relaxed);
        let mut last_progress = Instant::now();
        let mut stalled = false;
        loop {
            let writers_done = self.writers.iter().all(|(handle, _)| handle.is_finished());
            if writers_done {
                self.join_writers();
                if self.control.broken.load(Ordering::SeqCst) && !stopping(shared) {
                    // Les tubes ne passent plus : l'encodeur est hors service.
                    let _ = self.child.kill();
                    let status = self.child.wait().map(|s| s.to_string()).unwrap_or_default();
                    return PartEnd::Crashed(format!("l'encodeur s'est arrêté ({status}) : {}", log_tail(log)));
                }
                // Flux fermés : FFmpeg termine le fichier.
                return match wait_exit(&mut self.child, FINALIZE_TIMEOUT) {
                    Some(status) if !status.success() && !stopping(shared) => {
                        PartEnd::Crashed(format!("l'encodeur s'est arrêté ({status}) : {}", log_tail(log)))
                    }
                    _ => PartEnd::Closed,
                };
            }
            if let Ok(Some(status)) = self.child.try_wait() {
                // FFmpeg parti (planté, ou arrêté parce que figé) : les fils
                // d'écriture sont débloqués et la partie est abandonnée.
                self.abandon_writers();
                let reason = if stalled {
                    "l'encodeur ne répondait plus".to_string()
                } else {
                    format!("l'encodeur s'est arrêté ({status}) : {}", log_tail(log))
                };
                return if stopping(shared) && !stalled { PartEnd::Closed } else { PartEnd::Crashed(reason) };
            }
            if self.control.broken.load(Ordering::SeqCst) && !stalled {
                // Un tube ne passe plus alors que FFmpeg tourne encore.
                log::warn!("un tube de l'encodeur est rompu : arrêt de l'encodeur");
                stalled = true;
                let _ = self.child.kill();
            }
            let bytes = shared.bytes_sent.load(Ordering::Relaxed);
            if bytes != last_bytes {
                last_bytes = bytes;
                last_progress = Instant::now();
            } else if !stalled && last_progress.elapsed() >= STALL_TIMEOUT {
                log::warn!("l'encodeur n'avance plus depuis {} s : arrêt forcé", STALL_TIMEOUT.as_secs());
                stalled = true;
                let _ = self.child.kill();
            }
            thread::sleep(SUPERVISION_INTERVAL);
        }
    }

    /// À appeler une fois FFmpeg parti : les écritures échouent ou voient
    /// l'annulation ; un fil encore en attente de connexion (FFmpeg n'a
    /// jamais ouvert son tube) est débloqué en ouvrant le tube à sa place.
    fn abandon_writers(&mut self) {
        self.control.cancel.store(true, Ordering::SeqCst);
        let grace = Instant::now() + Duration::from_millis(800);
        while Instant::now() < grace && !self.writers.iter().all(|(handle, _)| handle.is_finished()) {
            thread::sleep(Duration::from_millis(20));
        }
        for (handle, pipe_path) in &self.writers {
            if !handle.is_finished() {
                let path = pipe_path.clone();
                // Dans un fil à part : cette ouverture ne doit jamais bloquer
                // le superviseur.
                let _ = thread::Builder::new().name("deblocage-tube".into()).spawn(move || {
                    let _ = OpenOptions::new().read(true).open(path);
                });
            }
        }
        self.join_writers();
    }

    fn join_writers(&mut self) {
        for (handle, _) in self.writers.drain(..) {
            let _ = handle.join();
        }
    }
}

/// Fichier d'une partie suivante : `nom_partie2.mkv`.
fn part_path(first: &Path, part: u32) -> PathBuf {
    let dir = first.parent().unwrap_or(Path::new("."));
    let stem = first.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = first.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_else(|| "mkv".into());
    naming::unique_path(dir, &format!("{stem}_partie{part}"), &ext, &Default::default())
}

/// Point de reprise commun à l'image et au son après une relance : le plus
/// avancé des deux flux, arrondi à une image. Renvoie (image, octet de son).
fn resume_positions(video_frames: u64, audio_bytes: u64, fps: u32) -> (u64, u64) {
    let fps = fps.max(1) as u64;
    let rate = SAMPLE_RATE as u64;
    let frame_bytes = CHANNELS as u64 * 4;
    let audio_samples = audio_bytes.div_ceil(frame_bytes);
    let start_frame = video_frames.max((audio_samples * fps).div_ceil(rate));
    let start_sample = (start_frame * rate).div_ceil(fps).max(audio_samples);
    (start_frame, start_sample * frame_bytes)
}

fn clock(seconds: u64) -> String {
    format!("{:02}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
}

fn wait_exit(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                log::info!("encodeur terminé : {status}");
                return Some(status);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(SUPERVISION_INTERVAL),
            _ => {
                log::warn!("l'encodeur ne se termine pas : arrêt forcé");
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// Dernières lignes utiles du journal de FFmpeg, pour expliquer un échec.
pub(crate) fn log_tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with("--- partie")).collect();
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
    fn resume_aligns_sound_on_the_next_frame() {
        // 48 000 Hz à 60 i/s : 800 échantillons (6 400 octets) par image.
        assert_eq!(resume_positions(0, 0, 60), (0, 0));
        // Image en avance : le son saute jusqu'à elle.
        assert_eq!(resume_positions(120, 6_400 * 100, 60), (120, 6_400 * 120));
        // Son en avance, au milieu d'une image : on part de l'image suivante.
        assert_eq!(resume_positions(100, 6_400 * 110 + 8, 60), (111, 6_400 * 111));
        // 30 i/s : 1 600 échantillons par image.
        assert_eq!(resume_positions(10, 0, 30), (10, 12_800 * 10));
    }

    #[test]
    fn part_names() {
        let dir = std::env::temp_dir();
        let first = dir.join(format!("polyptyque-{}.mkv", polyptyque_core::config::new_id()));
        let stem = first.file_stem().unwrap().to_string_lossy().into_owned();
        assert_eq!(part_path(&first, 2), dir.join(format!("{stem}_partie2.mkv")));
    }

    #[test]
    fn clock_format() {
        assert_eq!(clock(3723), "01:02:03");
    }

    #[test]
    fn log_tail_keeps_last_lines() {
        let path = std::env::temp_dir().join(format!("polyptyque-log-{}.txt", polyptyque_core::config::new_id()));
        std::fs::write(&path, "--- partie 1 ---\ncommande\n\nligne 1\nligne 2\nligne 3\n").unwrap();
        assert_eq!(log_tail(&path), "ligne 1 | ligne 2 | ligne 3");
        std::fs::remove_file(path).unwrap();
    }
}
