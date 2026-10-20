//! Configuration persistante : scènes, sources et réglages.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::naming::DEFAULT_TEMPLATE;
use crate::paths;

pub const CONFIG_VERSION: u32 = 1;

/// Fréquence d'échantillonnage utilisée pour tout l'audio.
pub const SAMPLE_RATE: u32 = 48_000;
/// Nombre de canaux audio (stéréo).
pub const CHANNELS: u16 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub active_scene: String,
    pub settings: Settings,
    pub scenes: Vec<Scene>,
}

impl Default for Config {
    fn default() -> Self {
        let scene = Scene::new("Scène principale");
        Self {
            version: CONFIG_VERSION,
            active_scene: scene.id.clone(),
            settings: Settings::default(),
            scenes: vec![scene],
        }
    }
}

impl Config {
    /// Charge la configuration. En cas de fichier absent, renvoie la
    /// configuration par défaut ; en cas de fichier illisible, en garde une
    /// copie et renvoie un avertissement.
    pub fn load_or_default(path: &Path) -> (Config, Option<String>) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => return (Config::default(), None),
            Err(e) => {
                return (
                    Config::default(),
                    Some(format!("Configuration illisible ({e}), réglages par défaut utilisés.")),
                )
            }
        };
        match toml::from_str::<Config>(&text) {
            Ok(mut config) => {
                config.normalize();
                (config, None)
            }
            Err(e) => {
                let backup = path.with_extension("toml.invalide");
                let _ = fs::copy(path, &backup);
                (
                    Config::default(),
                    Some(format!("Configuration invalide ({e}). Une copie a été gardée dans {}.", backup.display())),
                )
            }
        }
    }

    /// Enregistre la configuration de façon atomique (fichier temporaire puis
    /// renommage), pour ne jamais laisser un fichier à moitié écrit.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("création du dossier {}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self).context("sérialisation de la configuration")?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text).with_context(|| format!("écriture de {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("remplacement de {}", path.display()))?;
        Ok(())
    }

    /// Garantit les invariants : au moins une scène, une scène active valide,
    /// des réglages dans les bornes.
    pub fn normalize(&mut self) {
        if self.scenes.is_empty() {
            self.scenes.push(Scene::new("Scène principale"));
        }
        if !self.scenes.iter().any(|s| s.id == self.active_scene) {
            self.active_scene = self.scenes[0].id.clone();
        }
        self.settings.video.normalize();
        self.settings.audio.normalize();
        if self.settings.filename_template.trim().is_empty() {
            self.settings.filename_template = DEFAULT_TEMPLATE.to_string();
        }
    }

    pub fn active_scene(&self) -> &Scene {
        self.scenes.iter().find(|s| s.id == self.active_scene).unwrap_or(&self.scenes[0])
    }

    pub fn active_scene_mut(&mut self) -> &mut Scene {
        let index = self.scenes.iter().position(|s| s.id == self.active_scene).unwrap_or(0);
        &mut self.scenes[index]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub sources: Vec<Source>,
}

impl Scene {
    pub fn new(name: &str) -> Self {
        Self { id: new_id(), name: name.to_string(), sources: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub capture_cursor: bool,
    /// Relancer le programme s'il n'est pas ouvert (toujours vrai pour un
    /// navigateur géré).
    #[serde(default)]
    pub relaunch: bool,
    pub kind: SourceKind,
}

impl Source {
    pub fn should_launch(&self) -> bool {
        match self.kind {
            SourceKind::Browser { .. } => true,
            SourceKind::Application { .. } => self.relaunch,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    /// Navigateur lancé par Polyptyque avec un profil qui lui est dédié.
    Browser {
        profile_dir: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_url: Option<String>,
        browser: BrowserRef,
    },
    /// Toute autre fenêtre (Discord, jeu, Tor Browser…).
    Application {
        exe_path: PathBuf,
        window_class: String,
        window_title: String,
        #[serde(default)]
        launch_args: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserRef {
    pub name: String,
    pub exe_path: PathBuf,
    pub family: BrowserFamily,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFamily {
    /// Firefox et ses dérivés.
    Gecko,
    /// Chrome, Edge, Brave, Vivaldi, Opera…
    Chromium,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub output_dir: PathBuf,
    pub filename_template: String,
    pub video: VideoSettings,
    pub audio: AudioSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: paths::default_output_dir(),
            filename_template: DEFAULT_TEMPLATE.to_string(),
            video: VideoSettings::default(),
            audio: AudioSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: Codec,
    pub hardware: HardwarePreference,
    pub bitrate_kbps: u32,
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            codec: Codec::H264,
            hardware: HardwarePreference::Auto,
            bitrate_kbps: 12_000,
        }
    }
}

impl VideoSettings {
    /// Ramène les valeurs dans des bornes sûres. Le format NV12 impose des
    /// dimensions paires.
    pub fn normalize(&mut self) {
        self.width = (self.width.clamp(64, 7680)) & !1;
        self.height = (self.height.clamp(64, 4320)) & !1;
        self.fps = self.fps.clamp(1, 240);
        self.bitrate_kbps = self.bitrate_kbps.clamp(250, 500_000);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
    Hevc,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HardwarePreference {
    /// Carte graphique si disponible, sinon processeur.
    Auto,
    /// Carte graphique uniquement.
    Gpu,
    /// Processeur uniquement.
    Cpu,
}

impl HardwarePreference {
    pub fn label(self) -> &'static str {
        match self {
            HardwarePreference::Auto => "Automatique (carte graphique si disponible)",
            HardwarePreference::Gpu => "Carte graphique",
            HardwarePreference::Cpu => "Processeur",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub bitrate_kbps: u32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { bitrate_kbps: 192 }
    }
}

impl AudioSettings {
    pub fn normalize(&mut self) {
        self.bitrate_kbps = self.bitrate_kbps.clamp(64, 512);
    }
}

fn default_true() -> bool {
    true
}

/// Identifiant court et unique (horodatage + compteur), suffisant pour
/// distinguer scènes, sources et dossiers de profil.
pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}{:02x}", n & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        let mut config = Config::default();
        config.active_scene_mut().sources.push(Source {
            id: new_id(),
            name: "Firefox 1".into(),
            capture_cursor: true,
            relaunch: true,
            kind: SourceKind::Browser {
                profile_dir: PathBuf::from(r"C:\Users\x\AppData\Local\Polyptyque\profils\abc"),
                start_url: Some("https://example.org".into()),
                browser: BrowserRef {
                    name: "Firefox".into(),
                    exe_path: PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
                    family: BrowserFamily::Gecko,
                },
            },
        });
        config.active_scene_mut().sources.push(Source {
            id: new_id(),
            name: "Discord".into(),
            capture_cursor: false,
            relaunch: false,
            kind: SourceKind::Application {
                exe_path: PathBuf::from(r"C:\Users\x\AppData\Local\Discord\app-1.0\Discord.exe"),
                window_class: "Chrome_WidgetWin_1".into(),
                window_title: "Discord".into(),
                launch_args: vec![],
            },
        });
        config
    }

    #[test]
    fn round_trip_toml() {
        let config = sample();
        let text = toml::to_string_pretty(&config).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.scenes.len(), 1);
        assert_eq!(back.active_scene().sources.len(), 2);
        assert_eq!(back.active_scene().sources[0].name, "Firefox 1");
        assert!(matches!(back.active_scene().sources[1].kind, SourceKind::Application { .. }));
    }

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("polyptyque-test-{}", new_id()));
        let path = dir.join("config.toml");
        let config = sample();
        config.save(&path).unwrap();
        let (loaded, warning) = Config::load_or_default(&path);
        assert!(warning.is_none());
        assert_eq!(loaded.active_scene().sources.len(), 2);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_file_is_kept_aside() {
        let dir = std::env::temp_dir().join(format!("polyptyque-test-{}", new_id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "ceci n'est pas du toml = = =").unwrap();
        let (loaded, warning) = Config::load_or_default(&path);
        assert!(warning.is_some());
        assert_eq!(loaded.scenes.len(), 1);
        assert!(path.with_extension("toml.invalide").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn normalize_fixes_invariants() {
        let mut config = Config { scenes: vec![], active_scene: "inexistante".into(), ..Default::default() };
        config.settings.video.width = 1921;
        config.settings.video.fps = 0;
        config.normalize();
        assert_eq!(config.scenes.len(), 1);
        assert_eq!(config.active_scene, config.scenes[0].id);
        assert_eq!(config.settings.video.width, 1920);
        assert_eq!(config.settings.video.fps, 1);
    }

    #[test]
    fn ids_are_unique() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
    }
}
