//! Plateforme de démonstration, hors Windows : fausses fenêtres animées et
//! faux son. Elle permet de développer et de vérifier l'interface (et même
//! d'enregistrer, avec FFmpeg) sans machine Windows.

use std::f32::consts::TAU;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use polyptyque_core::config::{BrowserFamily, BrowserRef, Config, Scene, Source};
use polyptyque_core::platform::*;

struct DemoWindow {
    handle: WindowHandle,
    title: &'static str,
    class: &'static str,
    exe: &'static str,
    /// Teinte de l'image (chrominance U, V).
    tint: (u8, u8),
}

const WINDOWS: [DemoWindow; 4] = [
    DemoWindow {
        handle: 1,
        title: "Concert en direct — Mozilla Firefox",
        class: "MozillaWindowClass",
        exe: "/demo/firefox.exe",
        tint: (96, 168),
    },
    DemoWindow {
        handle: 2,
        title: "Conférence - Google Chrome",
        class: "Chrome_WidgetWin_1",
        exe: "/demo/chrome.exe",
        tint: (168, 150),
    },
    DemoWindow {
        handle: 3,
        title: "#général | Discord",
        class: "Chrome_WidgetWin_1",
        exe: "/demo/Discord.exe",
        tint: (176, 112),
    },
    DemoWindow { handle: 4, title: "Notes", class: "Notepad", exe: "/demo/notepad.exe", tint: (128, 128) },
];

pub struct DemoPlatform {
    start: Instant,
    root: PathBuf,
}

impl DemoPlatform {
    pub fn new(root: &Path) -> Self {
        let _ = fs::create_dir_all(root.join("tubes"));
        Self { start: Instant::now(), root: root.to_path_buf() }
    }

    fn profile(&self, name: &str) -> PathBuf {
        self.root.join("profils").join(name)
    }

    /// Configuration de démonstration : une scène avec trois sources.
    pub fn seed_config(&self) -> Config {
        let mut config = Config::default();
        config.settings.output_dir = self.root.join("enregistrements");
        let firefox =
            BrowserRef { name: "Firefox".into(), exe_path: "/demo/firefox.exe".into(), family: BrowserFamily::Gecko };
        let chrome = BrowserRef {
            name: "Google Chrome".into(),
            exe_path: "/demo/chrome.exe".into(),
            family: BrowserFamily::Chromium,
        };
        let mut concert = Source::browser("Concert", firefox, None, &self.root.join("profils"));
        let mut conference = Source::browser("Conférence", chrome, None, &self.root.join("profils"));
        set_profile(&mut concert, self.profile("concert"));
        set_profile(&mut conference, self.profile("conference"));
        let discord = WindowInfo {
            handle: 3,
            pid: 103,
            title: WINDOWS[2].title.into(),
            class: WINDOWS[2].class.into(),
            exe: WINDOWS[2].exe.into(),
        };
        let scene = config.active_scene_mut();
        scene.name = "Podcast".into();
        scene.sources = vec![concert, conference, Source::application("Discord", &discord, false, true)];
        config.scenes.push(Scene::new("Archive"));
        config
    }
}

fn set_profile(source: &mut Source, dir: PathBuf) {
    if let polyptyque_core::config::SourceKind::Browser { profile_dir, .. } = &mut source.kind {
        *profile_dir = dir;
    }
}

impl Platform for DemoPlatform {
    fn list_windows(&self) -> Vec<WindowInfo> {
        WINDOWS
            .iter()
            .map(|w| WindowInfo {
                handle: w.handle,
                pid: 100 + w.handle as u32,
                title: w.title.into(),
                class: w.class.into(),
                exe: w.exe.into(),
            })
            .collect()
    }

    fn is_window_alive(&self, _: WindowHandle) -> bool {
        true
    }

    fn is_window_minimized(&self, _: WindowHandle) -> bool {
        false
    }

    fn process_command_line(&self, pid: u32) -> Option<String> {
        match pid {
            101 => Some(format!("firefox.exe -profile \"{}\" -no-remote", self.profile("concert").display())),
            102 => Some(format!("chrome.exe --user-data-dir={}", self.profile("conference").display())),
            _ => None,
        }
    }

    fn detect_browsers(&self) -> Vec<BrowserInstall> {
        vec![
            BrowserInstall {
                name: "Firefox".into(),
                exe_path: "/demo/firefox.exe".into(),
                family: BrowserFamily::Gecko,
            },
            BrowserInstall {
                name: "Google Chrome".into(),
                exe_path: "/demo/chrome.exe".into(),
                family: BrowserFamily::Chromium,
            },
        ]
    }

    fn now_hns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64 / 100
    }

    fn open_video_capture(
        &self,
        handle: WindowHandle,
        width: u32,
        height: u32,
        _: bool,
    ) -> Result<Box<dyn VideoCapture>> {
        let tint = WINDOWS.iter().find(|w| w.handle == handle).map_or((128, 128), |w| w.tint);
        Ok(Box::new(DemoVideo { start: self.start, width, height, tint }))
    }

    fn open_audio_capture(&self, pid: u32, sample_rate: u32, channels: u16) -> Result<Box<dyn AudioCapture>> {
        Ok(Box::new(DemoAudio {
            start: self.start,
            sample_rate,
            channels,
            pitch: 220.0 * pid.saturating_sub(100).max(1) as f32,
            next_frame: None,
            phase: 0.0,
        }))
    }

    fn create_stream_pipe(&self, name: &str) -> Result<Box<dyn StreamPipe>> {
        let path = self.root.join("tubes").join(name);
        let status = Command::new("mkfifo").arg(&path).status()?;
        anyhow::ensure!(status.success(), "mkfifo a échoué");
        Ok(Box::new(Fifo { path }))
    }

    fn prepare_encoder_process(&self, _: &mut Command) {}
    fn begin_recording_session(&self) {}
    fn end_recording_session(&self) {}

    fn free_disk_space(&self, _: &Path) -> Option<u64> {
        Some(412_300_000_000)
    }
}

/// Image animée : dégradé teinté et barre lumineuse qui défile.
struct DemoVideo {
    start: Instant,
    width: u32,
    height: u32,
    tint: (u8, u8),
}

impl VideoCapture for DemoVideo {
    fn poll(&mut self) -> Result<Option<Nv12Frame>> {
        let (w, h) = (self.width as usize, self.height as usize);
        let t = self.start.elapsed().as_secs_f32();
        let bar = ((t * 0.25).fract() * w as f32) as usize;
        let mut frame = vec![0u8; w * h * 3 / 2];
        for y in 0..h {
            for x in 0..w {
                let base = 40.0 + 70.0 * (y as f32 / h as f32) + 25.0 * ((x as f32 / w as f32) * TAU + t).sin();
                let glow = if x.abs_diff(bar) < w / 24 { 90.0 } else { 0.0 };
                frame[y * w + x] = (base + glow).clamp(16.0, 235.0) as u8;
            }
        }
        for pair in frame[w * h..].chunks_exact_mut(2) {
            pair[0] = self.tint.0;
            pair[1] = self.tint.1;
        }
        Ok(Some(Arc::new(frame)))
    }
}

/// Son continu dont le volume ondule, daté par l'horloge commune.
struct DemoAudio {
    start: Instant,
    sample_rate: u32,
    channels: u16,
    pitch: f32,
    next_frame: Option<u64>,
    phase: f32,
}

impl AudioCapture for DemoAudio {
    fn read(&mut self, timeout_ms: u32, out: &mut Vec<AudioPacket>) -> Result<()> {
        std::thread::sleep(Duration::from_millis(timeout_ms.min(10) as u64));
        let now_frames = self.start.elapsed().as_nanos() as u64 * self.sample_rate as u64 / 1_000_000_000;
        let mut next = self.next_frame.unwrap_or(now_frames);
        while next + 480 <= now_frames {
            let volume = 0.35 + 0.3 * ((next as f32 / self.sample_rate as f32) * 0.7).sin();
            let mut data = Vec::with_capacity(480 * self.channels as usize * 4);
            for _ in 0..480 {
                let sample = (self.phase * TAU).sin() * volume;
                self.phase = (self.phase + self.pitch / self.sample_rate as f32).fract();
                for _ in 0..self.channels {
                    data.extend_from_slice(&sample.to_le_bytes());
                }
            }
            let timestamp = next * 10_000_000 / self.sample_rate as u64;
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
        Ok(Box::new(fs::OpenOptions::new().write(true).open(&self.path)?))
    }
}
