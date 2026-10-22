//! Configuration persistante : scènes, sources et réglages.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::browsers::program_name;
use crate::naming::{self, DEFAULT_TEMPLATE};
use crate::paths;
use crate::platform::{BrowserInstall, WindowInfo};
use crate::storage;

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
    /// Charge la configuration. Fichier absent : configuration par défaut.
    /// Fichier abîmé ou vide (coupure de courant, disque) : une copie en est
    /// gardée, puis la sauvegarde la plus récente encore valide est
    /// restaurée ; à défaut, configuration par défaut. Le message éventuel
    /// est à montrer à l'utilisateur.
    pub fn load_or_default(path: &Path) -> (Config, Option<String>) {
        let problem = match fs::read_to_string(path) {
            Ok(text) => match parse(&text) {
                Ok(config) => return (config, None),
                Err(e) => {
                    let damaged = storage::sibling(path, ".invalide");
                    let _ = fs::copy(path, &damaged);
                    format!("Réglages abîmés ({e:#}). Une copie a été gardée dans {}.", damaged.display())
                }
            },
            Err(e) if e.kind() == ErrorKind::NotFound => return (Config::default(), None),
            Err(e) => format!("Réglages illisibles ({e})."),
        };
        for backup in backups(path) {
            let Ok(config) = fs::read_to_string(&backup).map_err(anyhow::Error::from).and_then(|t| parse(&t)) else {
                continue;
            };
            let name = backup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = config.save(path);
            return (config, Some(format!("{problem} La sauvegarde « {name} » a été restaurée.")));
        }
        (Config::default(), Some(format!("{problem} Aucune sauvegarde valide : réglages par défaut.")))
    }

    /// Enregistre la configuration sans jamais laisser un fichier à moitié
    /// écrit. La version précédente est gardée (`config.toml.precedent`),
    /// ainsi qu'une copie par jour dans `sauvegardes` (les cinq derniers
    /// jours).
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("sérialisation de la configuration")?;
        if let Ok(current) = fs::read_to_string(path) {
            // Une version abîmée ne remplace jamais une bonne sauvegarde.
            if parse(&current).is_ok() && current != text {
                let _ = storage::write_atomic(&storage::sibling(path, ".precedent"), current.as_bytes());
                backup_daily(path, &current);
            }
        }
        storage::write_atomic(path, text.as_bytes())
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

    /// Vrai si le navigateur d'une source n'est plus à l'emplacement
    /// enregistré (autre PC, navigateur réinstallé ailleurs).
    pub fn has_missing_browser(&self) -> bool {
        self.scenes.iter().flat_map(|s| &s.sources).any(|source| match &source.kind {
            SourceKind::Browser { browser, .. } => !browser.exe_path.exists(),
            SourceKind::Application { .. } => false,
        })
    }

    /// Après un changement de PC ou de dossier (version portable) : chaque
    /// profil de navigateur est cherché dans le dossier de profils actuel,
    /// et un navigateur introuvable est remplacé par le même navigateur
    /// installé ailleurs. Renvoie vrai si quelque chose a changé.
    pub fn relocate(&mut self, profiles_root: &Path, browsers: &[BrowserInstall]) -> bool {
        let mut changed = false;
        for source in self.scenes.iter_mut().flat_map(|s| s.sources.iter_mut()) {
            let SourceKind::Browser { profile_dir, browser, .. } = &mut source.kind else {
                continue;
            };
            // Profil créé par Polyptyque : son dossier porte l'identifiant de
            // la source.
            let here = profiles_root.join(&source.id);
            let ours = profile_dir.file_name().is_some_and(|n| n.to_string_lossy() == source.id);
            if ours && *profile_dir != here && (here.exists() || !profile_dir.exists()) {
                *profile_dir = here;
                changed = true;
            }
            if !browser.exe_path.exists() {
                let found = browsers
                    .iter()
                    .find(|b| b.name == browser.name && b.family == browser.family && b.exe_path != browser.exe_path);
                if let Some(found) = found {
                    browser.exe_path = found.exe_path.clone();
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn active_scene(&self) -> &Scene {
        self.scenes.iter().find(|s| s.id == self.active_scene).unwrap_or(&self.scenes[0])
    }

    pub fn active_scene_mut(&mut self) -> &mut Scene {
        let index = self.scenes.iter().position(|s| s.id == self.active_scene).unwrap_or(0);
        &mut self.scenes[index]
    }
}

/// Jours de sauvegarde quotidienne des réglages conservés.
const BACKUP_DAYS: usize = 5;

fn parse(text: &str) -> Result<Config> {
    // Un fichier vide serait lu comme une configuration par défaut : toutes
    // les scènes disparaîtraient sans un mot.
    if text.trim().is_empty() {
        anyhow::bail!("fichier vide");
    }
    let mut config: Config = toml::from_str(text)?;
    config.normalize();
    Ok(config)
}

fn backup_dir(path: &Path) -> PathBuf {
    path.parent().unwrap_or(Path::new(".")).join("sauvegardes")
}

/// Sauvegardes possibles, de la plus récente à la plus ancienne.
fn backups(path: &Path) -> Vec<PathBuf> {
    let mut daily: Vec<PathBuf> = fs::read_dir(backup_dir(path))
        .map(|entries| entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| is_daily_backup(p)).collect())
        .unwrap_or_default();
    daily.sort();
    daily.reverse();
    std::iter::once(storage::sibling(path, ".precedent")).chain(daily).collect()
}

fn is_daily_backup(path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.starts_with("config_") && name.ends_with(".toml")
}

/// Garde une copie par jour (la première version du jour), et efface les
/// plus anciennes.
fn backup_daily(path: &Path, text: &str) {
    let dir = backup_dir(path);
    let (date, _) = naming::now_parts();
    let today = dir.join(format!("config_{date}.toml"));
    if today.exists() {
        return;
    }
    if storage::write_atomic(&today, text.as_bytes()).is_err() {
        return;
    }
    let mut daily: Vec<PathBuf> = backups(path).into_iter().skip(1).collect();
    for old in daily.drain(BACKUP_DAYS.min(daily.len())..) {
        let _ = fs::remove_file(old);
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
    /// Source navigateur avec un profil dédié, rangé dans `profiles_root`.
    pub fn browser(name: &str, browser: BrowserRef, start_url: Option<&str>, profiles_root: &Path) -> Self {
        let id = new_id();
        Self {
            name: name.trim().to_string(),
            capture_cursor: true,
            relaunch: true,
            kind: SourceKind::Browser {
                profile_dir: profiles_root.join(&id),
                start_url: start_url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()),
                browser,
            },
            id,
        }
    }

    /// Source application d'après une fenêtre ouverte.
    pub fn application(name: &str, window: &WindowInfo, relaunch: bool, capture_cursor: bool) -> Self {
        Self {
            id: new_id(),
            name: name.trim().to_string(),
            capture_cursor,
            relaunch,
            kind: SourceKind::Application {
                exe_path: window.exe.clone(),
                window_class: window.class.clone(),
                window_title: window.title.clone(),
                launch_args: Vec::new(),
            },
        }
    }

    pub fn should_launch(&self) -> bool {
        match self.kind {
            SourceKind::Browser { .. } => true,
            SourceKind::Application { .. } => self.relaunch,
        }
    }

    /// Description courte : « Navigateur Firefox », « Application Discord ».
    pub fn kind_label(&self) -> String {
        match &self.kind {
            SourceKind::Browser { browser, .. } => format!("Navigateur {}", browser.name),
            SourceKind::Application { exe_path, .. } => {
                format!("Application {}", program_name(exe_path).unwrap_or_default())
            }
        }
    }

    pub fn is_browser(&self) -> bool {
        matches!(self.kind, SourceKind::Browser { .. })
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
    /// Aperçus en direct des sources dans l'interface.
    pub previews: bool,
    /// Conversion automatique en MP4 à la fin de chaque enregistrement
    /// (le MKV d'origine est conservé).
    pub convert_to_mp4: bool,
    /// Recherche d'une nouvelle version au démarrage (sur GitHub).
    pub check_updates: bool,
    /// Versions proposées. Absent tant que l'utilisateur n'a rien choisi :
    /// c'est alors celui de la version en cours (voir `UpdateChannel`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_channel: Option<UpdateChannel>,
    /// Au lancement de Polyptyque, ouvrir les fenêtres manquantes de la
    /// scène active (navigateurs, applications dont la relance est activée).
    pub open_windows_at_launch: bool,
    pub video: VideoSettings,
    pub audio: AudioSettings,
    pub microphone: MicrophoneSettings,
    pub hotkeys: HotkeySettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            output_dir: paths::default_output_dir(),
            filename_template: DEFAULT_TEMPLATE.to_string(),
            previews: true,
            convert_to_mp4: false,
            check_updates: true,
            update_channel: None,
            open_windows_at_launch: true,
            video: VideoSettings::default(),
            audio: AudioSettings::default(),
            microphone: MicrophoneSettings::default(),
            hotkeys: HotkeySettings::default(),
        }
    }
}

impl Settings {
    /// Canal de mises à jour en vigueur pour la version `version` de
    /// Polyptyque.
    pub fn update_channel(&self, version: &str) -> UpdateChannel {
        self.update_channel.unwrap_or_else(|| UpdateChannel::of_version(version))
    }
}

/// Versions proposées par la recherche de mises à jour.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    /// Versions stables seulement.
    Stable,
    /// Versions stables et bêta (préversions GitHub).
    Beta,
}

impl UpdateChannel {
    /// Canal par défaut : une bêta reçoit les bêtas suivantes, puis la
    /// version stable ; une version stable ne reçoit que des versions
    /// stables. Une bêta se reconnaît au tiret de son numéro (« 1.0.0-beta.1 »).
    pub fn of_version(version: &str) -> Self {
        let without_build = version.split('+').next().unwrap_or(version);
        if without_build.contains('-') {
            Self::Beta
        } else {
            Self::Stable
        }
    }
}

/// Raccourcis clavier globaux, au format « Ctrl+Maj+F9 » (vide : aucun).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HotkeySettings {
    pub toggle_recording: String,
    pub show_window: String,
    pub toggle_previews: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            toggle_recording: "Ctrl+Maj+F9".into(),
            show_window: "Ctrl+Maj+F10".into(),
            toggle_previews: "Ctrl+Maj+F11".into(),
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

/// Micro enregistré dans un fichier séparé. Désactivé par défaut : il
/// faut le choisir explicitement.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MicrophoneSettings {
    pub enabled: bool,
    /// Identifiant du périphérique ; vide pour le micro par défaut de Windows.
    pub device_id: String,
    /// Nom affiché du périphérique choisi (pour le reconnaître s'il est
    /// débranché).
    pub device_name: String,
}

impl MicrophoneSettings {
    pub fn device(&self) -> Option<&str> {
        Some(self.device_id.as_str()).filter(|id| !id.is_empty())
    }

    pub fn label(&self) -> &str {
        if self.device_id.is_empty() || self.device_name.is_empty() {
            "micro par défaut"
        } else {
            &self.device_name
        }
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
        assert!(dir.join("config.toml.invalide").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_file_is_restored_from_backup() {
        let dir = std::env::temp_dir().join(format!("polyptyque-test-{}", new_id()));
        let path = dir.join("config.toml");
        let mut config = sample();
        config.save(&path).unwrap();
        config.active_scene_mut().name = "Modifiée".into();
        config.save(&path).unwrap();
        assert!(dir.join("config.toml.precedent").exists());
        assert_eq!(fs::read_dir(dir.join("sauvegardes")).unwrap().count(), 1);
        // Coupure de courant : fichier vide.
        fs::write(&path, "").unwrap();
        let (loaded, warning) = Config::load_or_default(&path);
        assert!(warning.unwrap().contains("restaurée"));
        assert_eq!(loaded.active_scene().sources.len(), 2);
        // Le fichier restauré est réécrit.
        assert!(!fs::read_to_string(&path).unwrap().trim().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn daily_backups_are_pruned() {
        let dir = std::env::temp_dir().join(format!("polyptyque-test-{}", new_id()));
        let backups = dir.join("sauvegardes");
        fs::create_dir_all(&backups).unwrap();
        for day in 1..=8 {
            fs::write(backups.join(format!("config_2020-01-0{day}.toml")), "x").unwrap();
        }
        let path = dir.join("config.toml");
        let mut config = sample();
        config.save(&path).unwrap();
        config.active_scene_mut().name = "Modifiée".into();
        config.save(&path).unwrap();
        let mut names: Vec<String> =
            fs::read_dir(&backups).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names.len(), BACKUP_DAYS);
        assert_eq!(names[0], "config_2020-01-05.toml");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn profiles_and_browsers_follow_the_portable_folder() {
        let old_root = Path::new("/ancien-pc/Polyptyque/données/profils");
        let dir = std::env::temp_dir().join(format!("polyptyque-test-{}", new_id()));
        let firefox =
            BrowserRef { name: "Firefox".into(), exe_path: dir.join("absent.exe"), family: BrowserFamily::Gecko };
        let mut config = Config::default();
        let source = Source::browser("Concert", firefox, None, old_root);
        let id = source.id.clone();
        config.active_scene_mut().sources.push(source);
        assert!(config.has_missing_browser());
        let installed = vec![BrowserInstall {
            name: "Firefox".into(),
            exe_path: PathBuf::from("C:/Program Files/Mozilla Firefox/firefox.exe"),
            family: BrowserFamily::Gecko,
        }];
        let profiles = dir.join("profils");
        assert!(config.relocate(&profiles, &installed));
        let SourceKind::Browser { profile_dir, browser, .. } = &config.active_scene().sources[0].kind else {
            panic!("source navigateur attendue");
        };
        assert_eq!(profile_dir, &profiles.join(&id));
        assert_eq!(browser.exe_path, installed[0].exe_path);
        // Une seconde fois : rien à changer.
        assert!(!config.relocate(&profiles, &installed));
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
    fn source_constructors() {
        let browser = BrowserRef {
            name: "Firefox".into(),
            exe_path: PathBuf::from(r"C:\ff\firefox.exe"),
            family: BrowserFamily::Gecko,
        };
        let source = Source::browser(" Firefox A ", browser, Some("  "), Path::new("/profils"));
        assert_eq!(source.name, "Firefox A");
        assert_eq!(source.kind_label(), "Navigateur Firefox");
        match &source.kind {
            SourceKind::Browser { profile_dir, start_url, .. } => {
                assert_eq!(profile_dir, &Path::new("/profils").join(&source.id));
                assert!(start_url.is_none());
            }
            _ => panic!("source navigateur attendue"),
        }
        let window = WindowInfo {
            handle: 1,
            pid: 2,
            title: "Discord".into(),
            class: "Chrome_WidgetWin_1".into(),
            exe: PathBuf::from(r"C:\d\Discord.exe"),
        };
        let app = Source::application("Discord", &window, false, true);
        assert_eq!(app.kind_label(), "Application Discord");
        assert!(!app.should_launch());
    }

    #[test]
    fn older_config_gets_new_defaults() {
        let text = "version = 1\nactive_scene = \"x\"\n[settings]\noutput_dir = \"/tmp\"\n";
        let config: Config = toml::from_str(text).unwrap();
        assert!(config.settings.previews);
        assert_eq!(config.settings.hotkeys.toggle_recording, "Ctrl+Maj+F9");
        assert_eq!(config.settings.update_channel, None);
    }

    #[test]
    fn update_channel_follows_the_version_until_chosen() {
        assert_eq!(UpdateChannel::of_version("1.0.0-beta.1"), UpdateChannel::Beta);
        assert_eq!(UpdateChannel::of_version("1.0.0"), UpdateChannel::Stable);
        assert_eq!(UpdateChannel::of_version("1.0.0+abc-def"), UpdateChannel::Stable);
        let mut settings = Settings::default();
        assert_eq!(settings.update_channel("1.0.0-beta.2"), UpdateChannel::Beta);
        assert_eq!(settings.update_channel("1.0.0"), UpdateChannel::Stable);
        // Rien n'est écrit tant que l'utilisateur n'a pas choisi.
        assert!(!toml::to_string(&settings).unwrap().contains("update_channel"));
        settings.update_channel = Some(UpdateChannel::Stable);
        assert_eq!(settings.update_channel("1.0.0-beta.2"), UpdateChannel::Stable);
        let back: Settings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
        assert_eq!(back.update_channel, Some(UpdateChannel::Stable));
    }

    #[test]
    fn ids_are_unique() {
        let a = new_id();
        let b = new_id();
        assert_ne!(a, b);
    }
}
