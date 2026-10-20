//! Session complète simulée : fausses fenêtres et faux son, vrai FFmpeg.
//!
//! Vérifie de bout en bout ce qui ne dépend pas de Windows : tubes, cadence
//! constante, placement du son, fenêtres absentes ou tardives, arrêt propre
//! et fichiers MKV lisibles aux bonnes durées. Ignoré si FFmpeg n'est pas
//! installé ; ne fonctionne que sous Unix (tubes nommés par `mkfifo`).

#![cfg(unix)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use polyptyque_core::config::{new_id, Codec, HardwarePreference, Settings, Source, SourceKind, VideoSettings};
use polyptyque_core::encoder::{EncoderChoice, EncoderKind};
use polyptyque_core::platform::*;
use polyptyque_engine::{remux, Session, SessionPlan, SourceState};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FPS: u32 = 30;

struct SimPlatform {
    start: Instant,
    pipes_dir: PathBuf,
}

impl SimPlatform {
    fn elapsed_hns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64 / 100
    }
}

fn window(handle: isize, title: &str) -> WindowInfo {
    WindowInfo {
        handle,
        pid: handle as u32,
        title: title.into(),
        class: "Classe".into(),
        exe: PathBuf::from(format!("/sim/{title}")),
    }
}

impl Platform for SimPlatform {
    fn list_windows(&self) -> Vec<WindowInfo> {
        let mut windows = vec![window(1, "immediate")];
        // La deuxième fenêtre n'apparaît qu'après une seconde.
        if self.start.elapsed() > Duration::from_secs(1) {
            windows.push(window(2, "tardive"));
        }
        windows
    }
    fn is_window_alive(&self, _: WindowHandle) -> bool {
        true
    }
    fn is_window_minimized(&self, _: WindowHandle) -> bool {
        false
    }
    fn process_command_line(&self, _: u32) -> Option<String> {
        None
    }
    fn detect_browsers(&self) -> Vec<BrowserInstall> {
        vec![]
    }
    fn now_hns(&self) -> u64 {
        // Décalage arbitraire : l'horloge ne commence pas à zéro.
        5_000_000_000 + self.elapsed_hns()
    }
    fn open_video_capture(&self, _: WindowHandle, w: u32, h: u32, _: bool) -> Result<Box<dyn VideoCapture>> {
        Ok(Box::new(SimVideo { width: w, height: h, counter: 0 }))
    }
    fn open_audio_capture(&self, _: u32, sample_rate: u32, channels: u16) -> Result<Box<dyn AudioCapture>> {
        Ok(Box::new(SimAudio { start: self.start, sample_rate, channels, next_frame: None, phase: 0.0 }))
    }
    fn create_stream_pipe(&self, name: &str) -> Result<Box<dyn StreamPipe>> {
        let path = self.pipes_dir.join(name);
        let status = Command::new("mkfifo").arg(&path).status()?;
        anyhow::ensure!(status.success(), "mkfifo a échoué");
        Ok(Box::new(Fifo { path }))
    }
    fn prepare_encoder_process(&self, _: &mut Command) {}
    fn begin_recording_session(&self) {}
    fn end_recording_session(&self) {}
    fn free_disk_space(&self, _: &Path) -> Option<u64> {
        Some(u64::MAX)
    }
}

struct SimVideo {
    width: u32,
    height: u32,
    counter: u8,
}

impl VideoCapture for SimVideo {
    fn poll(&mut self) -> Result<Option<Nv12Frame>> {
        self.counter = self.counter.wrapping_add(1);
        let luma = (self.width * self.height) as usize;
        let mut frame = vec![self.counter; luma + luma / 2];
        frame[luma..].fill(128);
        Ok(Some(Arc::new(frame)))
    }
}

/// Son continu (sinus 440 Hz) daté par l'horloge commune, livré par
/// paquets de 10 ms comme le ferait WASAPI.
struct SimAudio {
    start: Instant,
    sample_rate: u32,
    channels: u16,
    next_frame: Option<u64>,
    phase: f32,
}

impl AudioCapture for SimAudio {
    fn read(&mut self, timeout_ms: u32, out: &mut Vec<AudioPacket>) -> Result<()> {
        thread::sleep(Duration::from_millis(timeout_ms.min(10) as u64));
        let now_frames = self.start.elapsed().as_nanos() as u64 * self.sample_rate as u64 / 1_000_000_000;
        let mut next = self.next_frame.unwrap_or(now_frames);
        while next + 480 <= now_frames {
            let mut data = Vec::with_capacity(480 * self.channels as usize * 4);
            for _ in 0..480 {
                let sample = (self.phase * std::f32::consts::TAU).sin() * 0.5;
                self.phase = (self.phase + 440.0 / self.sample_rate as f32).fract();
                for _ in 0..self.channels {
                    data.extend_from_slice(&sample.to_le_bytes());
                }
            }
            let timestamp = 5_000_000_000 + next * 10_000_000 / self.sample_rate as u64;
            out.push(AudioPacket { data, timestamp_hns: Some(timestamp), silent: false });
            next += 480;
        }
        self.next_frame = Some(next);
        Ok(())
    }
}

struct Fifo {
    path: PathBuf,
}

impl StreamPipe for Fifo {
    fn ffmpeg_path(&self) -> String {
        self.path.display().to_string()
    }
    fn connect(self: Box<Self>) -> Result<Box<dyn Write + Send>> {
        Ok(Box::new(OpenOptions::new().write(true).open(&self.path)?))
    }
}

fn application(name: &str, title: &str) -> Source {
    Source {
        id: new_id(),
        name: name.into(),
        capture_cursor: false,
        relaunch: false,
        kind: SourceKind::Application {
            exe_path: PathBuf::from(format!("/sim/{title}")),
            window_class: "Classe".into(),
            window_title: title.into(),
            launch_args: vec![],
        },
    }
}

fn ffmpeg_available() -> bool {
    ["ffmpeg", "ffprobe"]
        .iter()
        .all(|tool| Command::new(tool).arg("-version").output().map(|o| o.status.success()).unwrap_or(false))
}

/// Durées (secondes) de la vidéo et du son d'un fichier, en comptant les
/// images et les échantillons réellement décodés.
fn measure(path: &Path) -> (f64, f64) {
    let probe = |stream: &str, entry: &str| -> String {
        let output = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", stream, "-count_frames", "-show_entries"])
            .arg(format!("stream={entry}"))
            .args(["-of", "default=noprint_wrappers=1:nokey=1"])
            .arg(path)
            .output()
            .expect("ffprobe");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    let video_frames: f64 = probe("v:0", "nb_read_frames").parse().expect("images");
    // Échantillons audio : décodage complet en PCM brut.
    let pcm = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-f", "s16le", "-ac", "1", "-ar", "48000", "-"])
        .output()
        .expect("décodage audio");
    let audio_samples = pcm.stdout.len() as f64 / 2.0;
    (video_frames / FPS as f64, audio_samples / 48_000.0)
}

/// Étiquettes colorimétriques du flux vidéo.
fn color_tags(path: &Path) -> String {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
        .arg("stream=color_range,color_space,color_transfer,color_primaries")
        .args(["-of", "compact=nk=0:p=0"])
        .arg(path)
        .output()
        .expect("ffprobe");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn simulated_session_produces_synchronised_files() {
    if !ffmpeg_available() {
        eprintln!("FFmpeg absent : test ignoré");
        return;
    }
    let root = std::env::temp_dir().join(format!("polyptyque-sim-{}", new_id()));
    let pipes_dir = root.join("tubes");
    fs::create_dir_all(&pipes_dir).unwrap();
    let platform = Arc::new(SimPlatform { start: Instant::now(), pipes_dir });

    let settings = Settings {
        output_dir: root.join("sorties"),
        filename_template: "{scène}_{source}".into(),
        video: VideoSettings {
            width: WIDTH,
            height: HEIGHT,
            fps: FPS,
            codec: Codec::H264,
            hardware: HardwarePreference::Cpu,
            bitrate_kbps: 500,
        },
        ..Default::default()
    };
    let plan = SessionPlan {
        scene_name: "Simulation".into(),
        sources: vec![
            application("Immédiate", "immediate"),
            application("Tardive", "tardive"),
            application("Absente", "absente"),
        ],
        settings,
        encoder: EncoderChoice { kind: EncoderKind::Software, codec: Codec::H264 },
        ffmpeg: PathBuf::from("ffmpeg"),
        log_dir: root.join("journaux"),
    };

    let mut session = Session::start(platform, plan).expect("démarrage");
    let recording = Duration::from_secs(4);
    let started = Instant::now();
    while started.elapsed() < recording {
        thread::sleep(Duration::from_millis(250));
        for status in session.status() {
            assert!(status.error.is_none() || status.state != SourceState::Failed, "{:?}", status);
        }
    }
    let statuses = session.status();
    assert_eq!(statuses[0].state, SourceState::Connected);
    assert_eq!(statuses[1].state, SourceState::Connected);
    assert_eq!(statuses[2].state, SourceState::Searching);
    let elapsed = session.elapsed().as_secs_f64();
    let summaries = session.stop();

    for summary in &summaries {
        assert!(summary.error.is_none(), "{} : {:?}", summary.name, summary.error);
        assert!(summary.output.exists(), "fichier manquant : {}", summary.output.display());
        assert_eq!(
            color_tags(&summary.output),
            "color_range=tv|color_space=bt709|color_transfer=bt709|color_primaries=bt709",
            "{}",
            summary.name
        );
        let (video, audio) = measure(&summary.output);
        eprintln!("{} : vidéo {video:.3} s, son {audio:.3} s, session {elapsed:.3} s", summary.name);
        // Toutes les sources ont la durée de la session, image et son calés.
        assert!((video - elapsed).abs() < 0.25, "{} : vidéo {video} s pour {elapsed} s", summary.name);
        assert!((audio - video).abs() < 0.1, "{} : son {audio} s, vidéo {video} s", summary.name);
    }
    assert!(summaries[0].output.ends_with("Simulation_Immédiate.mkv"));

    // Conversion sans perte en MP4 : mêmes durées, mêmes couleurs, aucune
    // erreur de décodage, fichier d'origine conservé.
    let mkv = &summaries[0].output;
    let mut progress = Vec::new();
    let mp4 = remux::convert(
        &SimPlatform { start: Instant::now(), pipes_dir: root.clone() },
        Path::new("ffmpeg"),
        mkv,
        &mut |p| progress.push(p),
        &AtomicBool::new(false),
    )
    .expect("conversion");
    assert!(mp4.ends_with("Simulation_Immédiate.mp4"));
    assert!(mkv.exists());
    assert_eq!(progress.last(), Some(&1.0));
    let (mkv_video, mkv_audio) = measure(mkv);
    let (mp4_video, mp4_audio) = measure(&mp4);
    eprintln!("MP4 : vidéo {mp4_video:.3} s, son {mp4_audio:.3} s");
    assert!((mp4_video - mkv_video).abs() < 0.001, "vidéo {mp4_video} s au lieu de {mkv_video} s");
    assert!((mp4_audio - mkv_audio).abs() < 0.03, "son {mp4_audio} s au lieu de {mkv_audio} s");
    assert!(color_tags(&mp4).contains("color_space=bt709"), "{}", color_tags(&mp4));
    let decode =
        Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&mp4).args(["-f", "null", "-"]).output().unwrap();
    assert!(decode.stderr.is_empty(), "{}", String::from_utf8_lossy(&decode.stderr));
    // Une deuxième conversion n'écrase pas la première.
    let again = remux::convert(
        &SimPlatform { start: Instant::now(), pipes_dir: root.clone() },
        Path::new("ffmpeg"),
        mkv,
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .expect("deuxième conversion");
    assert!(again.ends_with("Simulation_Immédiate (2).mp4"));
    // Une conversion annulée ne laisse rien derrière elle.
    let cancelled = remux::convert(
        &SimPlatform { start: Instant::now(), pipes_dir: root.clone() },
        Path::new("ffmpeg"),
        mkv,
        &mut |_| {},
        &AtomicBool::new(true),
    );
    assert!(cancelled.is_err());
    assert!(!mkv.with_file_name("Simulation_Immédiate (3).mp4").exists());
    fs::remove_dir_all(root).unwrap();
}
