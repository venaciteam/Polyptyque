//! Contrôleur : relie l'interface (le « pont » Slint) au moteur.
//!
//! Tout se passe sur le fil de l'interface, sauf les opérations longues
//! (recherche de l'encodeur, ouverture des fenêtres, finalisation des
//! fichiers), confiées à des fils séparés qui rendent la main par
//! `slint::invoke_from_event_loop`.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use polyptyque_core::browsers::{classify_executable, is_tor_browser, program_name};
use polyptyque_core::config::{
    BrowserFamily, BrowserRef, Codec, Config, HardwarePreference, Scene, Source, SourceKind,
};
use polyptyque_core::encoder::{bytes_per_hour, EncoderChoice};
use polyptyque_core::hotkeys;
use polyptyque_core::naming;
use polyptyque_core::paths;
use polyptyque_core::platform::{BrowserInstall, Platform, WindowInfo};
use polyptyque_engine::resolve::{self, PrepareEvent};
use polyptyque_engine::{probe, Session, SessionPlan, SourceState, SourceStatus, SourceSummary};
use slint::{CloseRequestResponse, ComponentHandle, Image, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::format;
use crate::preview::{PreviewManager, PreviewState};
use crate::{AppWindow, Bridge, SceneItem, SourceCard, SourceItem, WindowItem};

const LAUNCH_WAIT: Duration = Duration::from_secs(30);
const DISK_STOP_THRESHOLD: u64 = 1_000_000_000;
const DISK_WARNING_THRESHOLD: u64 = 10_000_000_000;
const BACKLOG_WARNING_SECONDS: f32 = 2.0;
const TOAST_DURATION: Duration = Duration::from_secs(7);

const RESOLUTIONS: [(u32, u32); 4] = [(1280, 720), (1920, 1080), (2560, 1440), (3840, 2160)];
const FRAME_RATES: [u32; 5] = [24, 25, 30, 50, 60];
const AUDIO_BITRATES: [u32; 5] = [128, 160, 192, 256, 320];
const HARDWARE: [HardwarePreference; 3] = [HardwarePreference::Auto, HardwarePreference::Gpu, HardwarePreference::Cpu];

/// Actions qui demandent une confirmation.
#[derive(Clone)]
enum Confirm {
    DeleteScene,
    RemoveSource(String),
    QuitWhileRecording,
}

/// Résultat de la préparation d'un enregistrement.
struct Prepared {
    ffmpeg: PathBuf,
    encoder: EncoderChoice,
    missing: Vec<String>,
}

/// Raccourci clavier global déclenché.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum HotkeyAction {
    ToggleRecording,
    ShowWindow,
    TogglePreviews,
}

pub struct App {
    ui: slint::Weak<AppWindow>,
    platform: Arc<dyn Platform>,
    config: RefCell<Config>,
    config_path: PathBuf,
    profiles_root: PathBuf,
    session: RefCell<Option<Session>>,
    busy: Cell<bool>,
    previews: RefCell<PreviewManager>,
    cards: Rc<VecModel<SourceCard>>,
    browsers: RefCell<Vec<BrowserInstall>>,
    windows: RefCell<Vec<WindowInfo>>,
    confirm: RefCell<Option<Confirm>>,
    toast_timer: Timer,
    free_space: Cell<Option<u64>>,
    last_disk_check: Cell<Option<Instant>>,
    quitting: Cell<bool>,
    #[cfg(windows)]
    hotkeys: RefCell<Option<polyptyque_windows::hotkeys::HotkeyListener>>,
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

/// Exécute `f` avec l'application, sur le fil de l'interface.
fn with_app<R>(f: impl FnOnce(&Rc<App>) -> R) -> Option<R> {
    let app = APP.with(|a| a.borrow().clone())?;
    Some(f(&app))
}

/// Depuis n'importe quel fil : planifie `f` sur le fil de l'interface.
fn post(f: impl FnOnce(&Rc<App>) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        with_app(f);
    });
}

pub struct Paths {
    pub config: PathBuf,
    pub profiles: PathBuf,
}

pub fn run(platform: Arc<dyn Platform>, paths: Paths, initial: Option<Config>) -> Result<()> {
    let ui = AppWindow::new().context("création de la fenêtre")?;
    let (config, warning) = match initial {
        Some(config) => (config, None),
        None => Config::load_or_default(&paths.config),
    };
    let app = Rc::new(App {
        ui: ui.as_weak(),
        platform,
        config: RefCell::new(config),
        config_path: paths.config,
        profiles_root: paths.profiles,
        session: RefCell::new(None),
        busy: Cell::new(false),
        previews: RefCell::new(PreviewManager::default()),
        cards: Rc::new(VecModel::default()),
        browsers: RefCell::new(Vec::new()),
        windows: RefCell::new(Vec::new()),
        confirm: RefCell::new(None),
        toast_timer: Timer::default(),
        free_space: Cell::new(None),
        last_disk_check: Cell::new(None),
        quitting: Cell::new(false),
        #[cfg(windows)]
        hotkeys: RefCell::new(None),
    });
    APP.with(|a| *a.borrow_mut() = Some(app.clone()));

    install_callbacks(&ui);
    let bridge = ui.global::<Bridge>();
    bridge.set_version(env!("CARGO_PKG_VERSION").into());
    bridge.set_cards(ModelRc::from(app.cards.clone()));
    bridge.set_can_browse(cfg!(windows));
    app.refresh_all();
    app.restart_previews();
    app.register_hotkeys();
    if let Some(message) = warning {
        app.toast(&message, 2);
    }

    ui.window()
        .on_close_requested(|| with_app(|app| app.close_requested()).unwrap_or(CloseRequestResponse::HideWindow));

    let status_timer = Timer::default();
    status_timer.start(TimerMode::Repeated, Duration::from_millis(500), || {
        with_app(|app| app.tick_status());
    });
    let preview_timer = Timer::default();
    preview_timer.start(TimerMode::Repeated, Duration::from_millis(100), || {
        with_app(|app| app.tick_previews());
    });

    ui.run().context("boucle de l'interface")?;

    app.previews.borrow_mut().stop();
    // Fermeture par le système pendant un enregistrement : on finalise.
    if let Some(session) = app.session.borrow_mut().take() {
        session.stop();
    }
    APP.with(|a| a.borrow_mut().take());
    Ok(())
}

fn install_callbacks(ui: &AppWindow) {
    let b = ui.global::<Bridge>();
    b.on_start_stop(|| {
        with_app(|a| a.start_stop());
    });
    b.on_select_scene(|id| {
        with_app(|a| a.select_scene(&id));
    });
    b.on_request_new_scene(|| {
        with_app(|a| a.open_scene_sheet(true));
    });
    b.on_request_rename_scene(|| {
        with_app(|a| a.open_scene_sheet(false));
    });
    b.on_request_delete_scene(|| {
        with_app(|a| a.request_delete_scene());
    });
    b.on_confirm_scene_name(|| {
        with_app(|a| a.confirm_scene_name());
    });
    b.on_request_add_browser(|| {
        with_app(|a| a.open_browser_sheet());
    });
    b.on_confirm_add_browser(|| {
        with_app(|a| a.confirm_add_browser());
    });
    b.on_request_add_application(|| {
        with_app(|a| a.open_application_sheet());
    });
    b.on_refresh_windows(|| {
        with_app(|a| a.refresh_windows());
    });
    b.on_window_selected(|index| {
        with_app(|a| a.window_selected(index));
    });
    b.on_confirm_add_application(|| {
        with_app(|a| a.confirm_add_application());
    });
    b.on_request_remove_source(|| {
        with_app(|a| a.request_remove_source());
    });
    b.on_move_source(|delta| {
        with_app(|a| a.move_source(delta));
    });
    b.on_open_source(|id| {
        with_app(|a| a.open_source(&id));
    });
    b.on_open_settings(|| {
        with_app(|a| a.open_settings());
    });
    b.on_browse_output(|| {
        with_app(|a| a.browse_output());
    });
    b.on_save_settings(|| {
        with_app(|a| a.save_settings());
    });
    b.on_open_output_folder(|| {
        with_app(|a| a.open_output_folder());
    });
    b.on_toggle_previews(|| {
        with_app(|a| a.toggle_previews());
    });
    b.on_close_sheet(|| {
        with_app(|a| a.close_sheet());
    });
    b.on_confirm_accepted(|| {
        with_app(|a| a.confirm_accepted());
    });
    b.on_dismiss_toast(|| {
        with_app(|a| a.dismiss_toast());
    });
}

impl App {
    fn with_bridge(&self, f: impl FnOnce(&Bridge)) {
        if let Some(ui) = self.ui.upgrade() {
            f(&ui.global::<Bridge>());
        }
    }

    fn recording(&self) -> bool {
        self.session.borrow().is_some()
    }

    fn save(&self) {
        let mut config = self.config.borrow_mut();
        config.normalize();
        if let Err(e) = config.save(&self.config_path) {
            drop(config);
            self.toast(&format!("Impossible d'enregistrer la configuration : {e:#}"), 3);
        }
    }

    // ----- Affichage -------------------------------------------------------

    fn refresh_all(&self) {
        let config = self.config.borrow();
        let scene = config.active_scene();
        let scenes: Vec<SceneItem> = config
            .scenes
            .iter()
            .map(|s| SceneItem {
                id: s.id.as_str().into(),
                name: s.name.as_str().into(),
                count: s.sources.len() as i32,
                active: s.id == config.active_scene,
            })
            .collect();
        let sources: Vec<SourceItem> = scene
            .sources
            .iter()
            .map(|s| SourceItem {
                id: s.id.as_str().into(),
                name: s.name.as_str().into(),
                kind: s.kind_label().into(),
                browser: s.is_browser(),
            })
            .collect();
        let previews = config.settings.previews;
        let cards: Vec<SourceCard> = scene
            .sources
            .iter()
            .map(|s| SourceCard {
                id: s.id.as_str().into(),
                name: s.name.as_str().into(),
                kind: s.kind_label().into(),
                state: if previews { "Recherche…" } else { "En attente" }.into(),
                tone: 0,
                preview: Image::default(),
                has_preview: false,
                placeholder: if previews { "Recherche de la fenêtre…" } else { "Aperçu désactivé" }.into(),
                level: 0.0,
                sound: false,
                size: "".into(),
                dropped: 0,
                warning: "".into(),
                can_open: false,
            })
            .collect();
        let scene_name = scene.name.clone();
        let source_count = scene.sources.len();
        drop(config);

        self.cards.set_vec(cards);
        self.with_bridge(|b| {
            b.set_scenes(ModelRc::new(VecModel::from(scenes)));
            b.set_scene_name(scene_name.into());
            b.set_sources(ModelRc::new(VecModel::from(sources)));
            b.set_previews_enabled(previews);
            if b.get_selected_source() >= source_count as i32 {
                b.set_selected_source(source_count as i32 - 1);
            }
        });
        self.refresh_disk(true);
        if !self.recording() {
            self.refresh_encoder_text();
        }
    }

    fn refresh_encoder_text(&self) {
        let config = self.config.borrow();
        let video = &config.settings.video;
        let text = format!(
            "{}x{} · {} i/s · {} · {} kb/s",
            video.width,
            video.height,
            video.fps,
            video.codec.label(),
            video.bitrate_kbps
        );
        drop(config);
        self.with_bridge(|b| b.set_encoder_text(text.into()));
    }

    /// Met à jour l'espace disque affiché (au plus toutes les 10 s, sauf si
    /// `force`).
    fn refresh_disk(&self, force: bool) {
        let due = self.last_disk_check.get().is_none_or(|t| t.elapsed() >= Duration::from_secs(10));
        if !force && !due {
            return;
        }
        self.last_disk_check.set(Some(Instant::now()));
        let config = self.config.borrow();
        let sources = config.active_scene().sources.len().max(1) as u64;
        let per_hour = bytes_per_hour(config.settings.video.bitrate_kbps, config.settings.audio.bitrate_kbps) * sources;
        let free = self.platform.free_disk_space(&config.settings.output_dir);
        drop(config);
        self.free_space.set(free);
        let (text, warning) = match free {
            Some(free) => {
                let autonomy = free as f64 / per_hour.max(1) as f64 * 3600.0;
                (
                    format!("{} libres · ≈ {} d'enregistrement", format::bytes(free), format::hours(autonomy)),
                    free < DISK_WARNING_THRESHOLD || free < per_hour,
                )
            }
            None => (String::new(), false),
        };
        self.with_bridge(|b| {
            b.set_disk_text(text.into());
            b.set_disk_warning(warning);
        });
    }

    fn toast(&self, message: &str, tone: i32) {
        log::info!("message affiché : {message}");
        self.with_bridge(|b| {
            b.set_toast_text(message.into());
            b.set_toast_tone(tone);
        });
        self.toast_timer.start(TimerMode::SingleShot, TOAST_DURATION, || {
            with_app(|a| a.dismiss_toast());
        });
    }

    fn dismiss_toast(&self) {
        self.with_bridge(|b| b.set_toast_text("".into()));
    }

    fn set_busy(&self, text: &str) {
        self.busy.set(!text.is_empty());
        self.with_bridge(|b| b.set_busy_text(text.into()));
    }

    fn update_card(&self, index: usize, f: impl FnOnce(&mut SourceCard)) {
        if let Some(mut card) = self.cards.row_data(index) {
            f(&mut card);
            self.cards.set_row_data(index, card);
        }
    }

    // ----- Aperçus ---------------------------------------------------------

    fn restart_previews(&self) {
        let config = self.config.borrow();
        let mut previews = self.previews.borrow_mut();
        previews.stop();
        if config.settings.previews {
            *previews = PreviewManager::start(&self.platform, &config.active_scene().sources);
        }
    }

    fn tick_previews(&self) {
        let recording = self.recording();
        let previews = self.previews.borrow();
        for worker in previews.workers() {
            let (frame, state) = worker.take_update();
            if frame.is_none() && state.is_none() {
                continue;
            }
            let Some(index) = (0..self.cards.row_count())
                .find(|&i| self.cards.row_data(i).is_some_and(|c| c.id.as_str() == worker.source_id))
            else {
                continue;
            };
            self.update_card(index, |card| {
                if let Some(frame) = frame {
                    card.preview = Image::from_rgba8(frame);
                    card.has_preview = true;
                }
                if let Some(state) = state {
                    let (label, tone, placeholder, can_open) = match &state {
                        PreviewState::Searching => ("Recherche…", 0, "Recherche de la fenêtre…".to_string(), false),
                        PreviewState::Live => ("Fenêtre trouvée", 1, String::new(), false),
                        PreviewState::Minimized => ("Réduite", 2, "Fenêtre réduite : l'image est figée.".into(), false),
                        PreviewState::Missing => ("Introuvable", 2, "Fenêtre introuvable".into(), true),
                        PreviewState::Failed(e) => ("Aperçu impossible", 3, e.clone(), false),
                    };
                    if !matches!(state, PreviewState::Live | PreviewState::Minimized) {
                        card.has_preview = false;
                    }
                    card.placeholder = placeholder.into();
                    card.can_open = can_open && !recording;
                    // Pendant l'enregistrement, l'état affiché est celui du moteur.
                    if !recording {
                        card.state = label.into();
                        card.tone = tone;
                    }
                }
            });
        }
    }

    fn toggle_previews(&self) {
        let enabled = {
            let mut config = self.config.borrow_mut();
            config.settings.previews = !config.settings.previews;
            config.settings.previews
        };
        self.save();
        self.refresh_all();
        self.restart_previews();
        self.toast(if enabled { "Aperçus en direct activés." } else { "Aperçus en direct coupés." }, 1);
    }

    // ----- Enregistrement --------------------------------------------------

    fn start_stop(&self) {
        if self.busy.get() {
            return;
        }
        if self.recording() {
            self.stop_recording(None);
        } else {
            self.start_recording();
        }
    }

    fn start_recording(&self) {
        let config = self.config.borrow().clone();
        let scene = config.active_scene().clone();
        if scene.sources.is_empty() {
            self.toast("Ajoute d'abord au moins une source à cette scène.", 2);
            return;
        }
        self.set_busy("Recherche du meilleur encodeur…");
        let platform = self.platform.clone();
        thread::spawn(move || {
            let result = prepare(platform.as_ref(), &config, &scene);
            post(move |app| app.finish_start(result, config, scene));
        });
    }

    fn finish_start(&self, result: Result<Prepared>, config: Config, scene: Scene) {
        self.set_busy("");
        let prepared = match result {
            Ok(prepared) => prepared,
            Err(e) => {
                self.toast(&format!("Impossible de démarrer : {e:#}"), 3);
                return;
            }
        };
        let (date, time) = naming::now_parts();
        let plan = SessionPlan {
            scene_name: scene.name.clone(),
            sources: scene.sources.clone(),
            settings: config.settings.clone(),
            encoder: prepared.encoder,
            ffmpeg: prepared.ffmpeg,
            log_dir: paths::logs_dir().join(format!("session_{date}_{time}")),
        };
        match Session::start(self.platform.clone(), plan) {
            Ok(session) => {
                *self.session.borrow_mut() = Some(session);
                let video = &config.settings.video;
                let text = format!(
                    "{} · {}x{} · {} i/s · {} kb/s",
                    prepared.encoder.label(),
                    video.width,
                    video.height,
                    video.fps,
                    video.bitrate_kbps
                );
                self.with_bridge(|b| {
                    b.set_recording(true);
                    b.set_encoder_text(text.into());
                    b.set_elapsed("00:00:00".into());
                    b.set_sheet("".into());
                });
                for i in 0..self.cards.row_count() {
                    self.update_card(i, |card| card.can_open = false);
                }
                if prepared.missing.is_empty() {
                    self.toast("Enregistrement démarré.", 1);
                } else {
                    self.toast(
                        &format!(
                            "Enregistrement démarré. Fenêtre(s) introuvable(s) : {}. Image noire en attendant leur retour.",
                            prepared.missing.join(", ")
                        ),
                        2,
                    );
                }
            }
            Err(e) => self.toast(&format!("Impossible de démarrer : {e:#}"), 3),
        }
    }

    fn stop_recording(&self, reason: Option<String>) {
        let Some(session) = self.session.borrow_mut().take() else {
            return;
        };
        self.set_busy("Arrêt : finalisation des fichiers…");
        thread::spawn(move || {
            let summaries = session.stop();
            post(move |app| app.finish_stop(summaries, reason));
        });
    }

    fn finish_stop(&self, summaries: Vec<SourceSummary>, reason: Option<String>) {
        self.set_busy("");
        self.with_bridge(|b| b.set_recording(false));
        self.refresh_all();
        self.restart_previews();
        if self.quitting.get() {
            let _ = slint::quit_event_loop();
            return;
        }
        let errors: Vec<String> =
            summaries.iter().filter_map(|s| s.error.as_ref().map(|e| format!("{} : {e}", s.name))).collect();
        let total: u64 = summaries.iter().map(|s| s.file_size).sum();
        let dir = self.config.borrow().settings.output_dir.display().to_string();
        let mut message =
            format!("{} fichier(s) enregistré(s), {} au total, dans {dir}.", summaries.len(), format::bytes(total));
        if let Some(reason) = reason {
            message = format!("{reason} {message}");
        }
        if errors.is_empty() {
            self.toast(&message, 1);
        } else {
            self.toast(&format!("{message} Problèmes : {}", errors.join(" · ")), 3);
        }
    }

    fn tick_status(&self) {
        let statuses: Vec<SourceStatus> = {
            let mut session = self.session.borrow_mut();
            let Some(session) = session.as_mut() else {
                self.refresh_disk(false);
                return;
            };
            let elapsed = format::clock(session.elapsed());
            self.with_bridge(|b| b.set_elapsed(elapsed.into()));
            session.status()
        };
        for (index, status) in statuses.iter().enumerate() {
            self.update_card(index, |card| {
                let (state, tone) = match status.state {
                    SourceState::Connected => ("Enregistre", 1),
                    SourceState::Minimized => ("Réduite (image figée)", 2),
                    SourceState::Searching => ("En attente de la fenêtre", 2),
                    SourceState::Lost => ("Fenêtre perdue", 2),
                    SourceState::Failed => ("Erreur", 3),
                    SourceState::Finished => ("Terminée", 0),
                };
                card.state = state.into();
                card.tone = tone;
                card.level = format::level(status.audio_level);
                card.sound = status.audio_recent;
                card.size = format::bytes(status.file_size).into();
                card.dropped = status.frames_dropped as i32;
                card.warning = if let Some(error) = &status.error {
                    error.as_str().into()
                } else if status.backlog_seconds >= BACKLOG_WARNING_SECONDS {
                    format!("L'encodeur prend du retard ({:.0} s)", status.backlog_seconds).into()
                } else {
                    SharedString::new()
                };
            });
        }
        self.refresh_disk(false);
        if self.free_space.get().is_some_and(|free| free < DISK_STOP_THRESHOLD) {
            self.stop_recording(Some("Disque presque plein : arrêt automatique.".into()));
        }
    }

    // ----- Scènes ----------------------------------------------------------

    fn select_scene(&self, id: &str) {
        if self.recording() || self.busy.get() {
            return;
        }
        self.config.borrow_mut().active_scene = id.to_string();
        self.save();
        self.with_bridge(|b| b.set_selected_source(-1));
        self.refresh_all();
        self.restart_previews();
    }

    fn open_scene_sheet(&self, new: bool) {
        let current = self.config.borrow().active_scene().name.clone();
        self.with_bridge(|b| {
            b.set_scene_name_input(if new { SharedString::new() } else { current.into() });
            b.set_sheet(if new { "scene-new" } else { "scene-rename" }.into());
        });
    }

    fn confirm_scene_name(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let bridge = ui.global::<Bridge>();
        let name = bridge.get_scene_name_input().trim().to_string();
        if name.is_empty() {
            return;
        }
        let new = bridge.get_sheet() == "scene-new";
        {
            let mut config = self.config.borrow_mut();
            if new {
                let scene = Scene::new(&name);
                config.active_scene = scene.id.clone();
                config.scenes.push(scene);
            } else {
                config.active_scene_mut().name = name;
            }
        }
        bridge.set_sheet("".into());
        self.save();
        self.refresh_all();
        if new {
            self.restart_previews();
        }
    }

    fn request_delete_scene(&self) {
        let name = self.config.borrow().active_scene().name.clone();
        self.ask(
            Confirm::DeleteScene,
            "Supprimer la scène",
            &format!("La scène « {name} » et ses sources seront supprimées. Les profils de navigateur restent sur le disque."),
            "Supprimer",
        );
    }

    // ----- Sources ---------------------------------------------------------

    fn open_browser_sheet(&self) {
        let browsers = self.platform.detect_browsers();
        let mut choices: Vec<SharedString> = browsers.iter().map(|b| b.name.as_str().into()).collect();
        choices.push("Autre navigateur…".into());
        let count = self.config.borrow().active_scene().sources.len() + 1;
        let default_name = browsers
            .first()
            .map(|b| format!("{} {count}", b.name.split_whitespace().last().unwrap_or(&b.name)))
            .unwrap_or_else(|| format!("Navigateur {count}"));
        *self.browsers.borrow_mut() = browsers;
        self.with_bridge(|b| {
            b.set_browser_choices(ModelRc::new(VecModel::from(choices)));
            b.set_browser_index(0);
            b.set_browser_path("".into());
            b.set_new_name(default_name.into());
            b.set_new_url("".into());
            b.set_open_now(true);
            b.set_sheet("browser".into());
        });
    }

    fn confirm_add_browser(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let bridge = ui.global::<Bridge>();
        let index = bridge.get_browser_index().max(0) as usize;
        let browser = {
            let browsers = self.browsers.borrow();
            match browsers.get(index) {
                Some(b) => BrowserRef { name: b.name.clone(), exe_path: b.exe_path.clone(), family: b.family },
                None => {
                    let path = PathBuf::from(bridge.get_browser_path().trim().trim_matches('"'));
                    if !path.exists() {
                        self.toast("Ce fichier de navigateur n'existe pas.", 3);
                        return;
                    }
                    if is_tor_browser(&path) {
                        self.toast("Tor Browser s'ajoute comme source application (il efface tout à la fermeture).", 2);
                        return;
                    }
                    let family = classify_executable(&path).unwrap_or(BrowserFamily::Chromium);
                    let name = program_name(&path).map(|n| format::capitalize(&n)).unwrap_or("Navigateur".into());
                    BrowserRef { name, exe_path: path, family }
                }
            }
        };
        let url = bridge.get_new_url().to_string();
        let source = Source::browser(
            &bridge.get_new_name(),
            browser,
            Some(url.as_str()).filter(|u| !u.trim().is_empty()),
            &self.profiles_root,
        );
        let open_now = bridge.get_open_now();
        let name = source.name.clone();
        self.config.borrow_mut().active_scene_mut().sources.push(source.clone());
        bridge.set_sheet("".into());
        self.save();
        self.refresh_all();
        self.restart_previews();
        if open_now {
            match resolve::launch(&source) {
                Ok(()) => self.toast(
                    &format!("« {name} » ajoutée. Connecte-toi et prépare tes onglets : tout sera conservé."),
                    1,
                ),
                Err(e) => self.toast(&format!("« {name} » ajoutée, mais le navigateur ne s'ouvre pas : {e:#}"), 3),
            }
        } else {
            self.toast(&format!("« {name} » ajoutée."), 1);
        }
    }

    fn open_application_sheet(&self) {
        self.refresh_windows();
        self.with_bridge(|b| {
            b.set_window_index(-1);
            b.set_new_name("".into());
            b.set_relaunch(false);
            b.set_capture_cursor(true);
            b.set_sheet("application".into());
        });
    }

    fn refresh_windows(&self) {
        let windows = self.platform.list_windows();
        let items: Vec<WindowItem> = windows
            .iter()
            .map(|w| WindowItem {
                title: w.title.as_str().into(),
                program: program_name(&w.exe).unwrap_or_default().into(),
                browser: classify_executable(&w.exe).is_some(),
            })
            .collect();
        *self.windows.borrow_mut() = windows;
        self.with_bridge(|b| {
            b.set_windows(ModelRc::new(VecModel::from(items)));
            b.set_window_index(-1);
        });
    }

    fn window_selected(&self, index: i32) {
        let name = self
            .windows
            .borrow()
            .get(index.max(0) as usize)
            .map(|w| program_name(&w.exe).map(|n| format::capitalize(&n)).unwrap_or_else(|| w.title.clone()));
        if let Some(name) = name {
            self.with_bridge(|b| b.set_new_name(name.into()));
        }
    }

    fn confirm_add_application(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let bridge = ui.global::<Bridge>();
        let index = bridge.get_window_index();
        let Some(window) = self.windows.borrow().get(index.max(0) as usize).cloned().filter(|_| index >= 0) else {
            return;
        };
        let source =
            Source::application(&bridge.get_new_name(), &window, bridge.get_relaunch(), bridge.get_capture_cursor());
        let name = source.name.clone();
        self.config.borrow_mut().active_scene_mut().sources.push(source);
        bridge.set_sheet("".into());
        self.save();
        self.refresh_all();
        self.restart_previews();
        self.toast(&format!("« {name} » ajoutée."), 1);
    }

    fn selected_source(&self) -> Option<Source> {
        let index = self.ui.upgrade()?.global::<Bridge>().get_selected_source();
        self.config.borrow().active_scene().sources.get(index.max(0) as usize).cloned().filter(|_| index >= 0)
    }

    fn request_remove_source(&self) {
        let Some(source) = self.selected_source() else {
            return;
        };
        let detail = if source.is_browser() { " Son profil de navigateur reste sur le disque." } else { "" };
        self.ask(
            Confirm::RemoveSource(source.id.clone()),
            "Supprimer la source",
            &format!("La source « {} » sera retirée de la scène.{detail}", source.name),
            "Supprimer",
        );
    }

    fn move_source(&self, delta: i32) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let bridge = ui.global::<Bridge>();
        let index = bridge.get_selected_source();
        let target = index + delta;
        {
            let mut config = self.config.borrow_mut();
            let sources = &mut config.active_scene_mut().sources;
            if index < 0 || target < 0 || target as usize >= sources.len() {
                return;
            }
            sources.swap(index as usize, target as usize);
        }
        bridge.set_selected_source(target);
        self.save();
        self.refresh_all();
        self.restart_previews();
    }

    fn open_source(&self, id: &str) {
        let source = self.config.borrow().active_scene().sources.iter().find(|s| s.id == id).cloned();
        let Some(source) = source else {
            return;
        };
        if let SourceKind::Application { exe_path, .. } = &source.kind {
            if !exe_path.exists() {
                self.toast(&format!("Programme introuvable ({}) : ouvre la fenêtre toi-même.", exe_path.display()), 2);
                return;
            }
        }
        match resolve::launch(&source) {
            Ok(()) => self.toast(&format!("Ouverture de « {} »…", source.name), 1),
            Err(e) => self.toast(&format!("Impossible d'ouvrir « {} » : {e:#}", source.name), 3),
        }
    }

    // ----- Réglages --------------------------------------------------------

    fn open_settings(&self) {
        let config = self.config.borrow();
        let s = &config.settings;
        let mut resolutions: Vec<SharedString> = RESOLUTIONS.iter().map(|(w, h)| format!("{w} × {h}").into()).collect();
        let resolution_index = match RESOLUTIONS.iter().position(|&r| r == (s.video.width, s.video.height)) {
            Some(i) => i,
            None => {
                resolutions.push(format!("{} × {} (personnalisée)", s.video.width, s.video.height).into());
                resolutions.len() - 1
            }
        };
        let mut fps: Vec<SharedString> = FRAME_RATES.iter().map(|f| format!("{f} i/s").into()).collect();
        let fps_index = match FRAME_RATES.iter().position(|&f| f == s.video.fps) {
            Some(i) => i,
            None => {
                fps.push(format!("{} i/s (personnalisé)", s.video.fps).into());
                fps.len() - 1
            }
        };
        let hotkey_choices: Vec<SharedString> =
            std::iter::once("Aucun".to_string()).chain(hotkeys::choices()).map(Into::into).collect();
        let hotkey_index = |value: &str| {
            hotkeys::choices().iter().position(|c| c.eq_ignore_ascii_case(value)).map_or(0, |i| i as i32 + 1)
        };
        let values = (
            resolutions,
            resolution_index as i32,
            fps,
            fps_index as i32,
            s.video.bitrate_kbps.to_string(),
            match s.video.codec {
                Codec::H264 => 0,
                Codec::Hevc => 1,
            },
            HARDWARE.iter().position(|&h| h == s.video.hardware).unwrap_or(0) as i32,
            AUDIO_BITRATES.iter().position(|&a| a == s.audio.bitrate_kbps).unwrap_or(2) as i32,
            s.output_dir.display().to_string(),
            s.filename_template.clone(),
            s.previews,
            hotkey_index(&s.hotkeys.toggle_recording),
            hotkey_index(&s.hotkeys.show_window),
            hotkey_index(&s.hotkeys.toggle_previews),
        );
        drop(config);
        self.with_bridge(|b| {
            b.set_resolution_choices(ModelRc::new(VecModel::from(values.0)));
            b.set_resolution_index(values.1);
            b.set_fps_choices(ModelRc::new(VecModel::from(values.2)));
            b.set_fps_index(values.3);
            b.set_bitrate_text(values.4.into());
            b.set_codec_choices(ModelRc::new(VecModel::from(vec![
                SharedString::from("H.264 : compatibilité maximale (recommandé)"),
                SharedString::from("HEVC : fichiers plus légers"),
            ])));
            b.set_codec_index(values.5);
            b.set_hardware_choices(ModelRc::new(VecModel::from(
                HARDWARE.iter().map(|h| SharedString::from(h.label())).collect::<Vec<_>>(),
            )));
            b.set_hardware_index(values.6);
            b.set_audio_choices(ModelRc::new(VecModel::from(
                AUDIO_BITRATES.iter().map(|a| SharedString::from(format!("{a} kb/s"))).collect::<Vec<_>>(),
            )));
            b.set_audio_index(values.7);
            b.set_output_dir(values.8.into());
            b.set_template(values.9.into());
            b.set_settings_previews(values.10);
            b.set_hotkey_choices(ModelRc::new(VecModel::from(hotkey_choices)));
            b.set_hotkey_record_index(values.11);
            b.set_hotkey_show_index(values.12);
            b.set_hotkey_previews_index(values.13);
            b.set_settings_error("".into());
            b.set_sheet("settings".into());
        });
    }

    fn browse_output(&self) {
        #[cfg(windows)]
        {
            let current = self.ui.upgrade().map(|ui| ui.global::<Bridge>().get_output_dir().to_string());
            let mut dialog = rfd::FileDialog::new().set_title("Dossier des enregistrements");
            if let Some(dir) = current.filter(|d| std::path::Path::new(d).exists()) {
                dialog = dialog.set_directory(dir);
            }
            if let Some(folder) = dialog.pick_folder() {
                self.with_bridge(|b| b.set_output_dir(folder.display().to_string().into()));
            }
        }
    }

    fn save_settings(&self) {
        let Some(ui) = self.ui.upgrade() else {
            return;
        };
        let b = ui.global::<Bridge>();
        let bitrate: u32 = match b.get_bitrate_text().trim().replace([' ', '\u{a0}'], "").parse() {
            Ok(value) if (250..=500_000).contains(&value) => value,
            _ => {
                b.set_settings_error("Débit vidéo invalide : indique un nombre entre 250 et 500 000 kb/s.".into());
                return;
            }
        };
        let output_dir = b.get_output_dir().trim().trim_matches('"').to_string();
        if output_dir.is_empty() {
            b.set_settings_error("Indique un dossier pour les enregistrements.".into());
            return;
        }
        let hotkey = |index: i32| {
            if index <= 0 {
                String::new()
            } else {
                hotkeys::choices().get(index as usize - 1).cloned().unwrap_or_default()
            }
        };
        let previews_before;
        let previews_after;
        {
            let mut config = self.config.borrow_mut();
            let s = &mut config.settings;
            if let Some(&(w, h)) = RESOLUTIONS.get(b.get_resolution_index().max(0) as usize) {
                s.video.width = w;
                s.video.height = h;
            }
            if let Some(&fps) = FRAME_RATES.get(b.get_fps_index().max(0) as usize) {
                s.video.fps = fps;
            }
            s.video.bitrate_kbps = bitrate;
            s.video.codec = if b.get_codec_index() == 1 { Codec::Hevc } else { Codec::H264 };
            s.video.hardware =
                HARDWARE.get(b.get_hardware_index().max(0) as usize).copied().unwrap_or(HardwarePreference::Auto);
            s.audio.bitrate_kbps = AUDIO_BITRATES.get(b.get_audio_index().max(0) as usize).copied().unwrap_or(192);
            s.output_dir = PathBuf::from(output_dir);
            s.filename_template = b.get_template().trim().to_string();
            previews_before = s.previews;
            s.previews = b.get_settings_previews();
            previews_after = s.previews;
            s.hotkeys.toggle_recording = hotkey(b.get_hotkey_record_index());
            s.hotkeys.show_window = hotkey(b.get_hotkey_show_index());
            s.hotkeys.toggle_previews = hotkey(b.get_hotkey_previews_index());
        }
        b.set_sheet("".into());
        self.save();
        self.refresh_all();
        if previews_before != previews_after {
            self.restart_previews();
        }
        self.register_hotkeys();
        self.toast("Réglages enregistrés.", 1);
    }

    fn open_output_folder(&self) {
        let dir = self.config.borrow().settings.output_dir.clone();
        if let Err(e) = fs::create_dir_all(&dir) {
            self.toast(&format!("Impossible de créer {} : {e}", dir.display()), 3);
            return;
        }
        let opener = if cfg!(windows) { "explorer" } else { "xdg-open" };
        if let Err(e) = Command::new(opener).arg(&dir).spawn() {
            self.toast(&format!("Impossible d'ouvrir le dossier : {e}"), 3);
        }
    }

    // ----- Fenêtres modales et confirmations --------------------------------

    fn close_sheet(&self) {
        self.confirm.borrow_mut().take();
        self.with_bridge(|b| {
            b.set_sheet("".into());
            b.set_settings_error("".into());
        });
    }

    fn ask(&self, action: Confirm, title: &str, text: &str, button: &str) {
        *self.confirm.borrow_mut() = Some(action);
        self.with_bridge(|b| {
            b.set_confirm_title(title.into());
            b.set_confirm_text(text.into());
            b.set_confirm_button(button.into());
            b.set_sheet("confirm".into());
        });
    }

    fn confirm_accepted(&self) {
        let action = self.confirm.borrow_mut().take();
        self.with_bridge(|b| b.set_sheet("".into()));
        match action {
            Some(Confirm::DeleteScene) => {
                {
                    let mut config = self.config.borrow_mut();
                    if config.scenes.len() <= 1 {
                        return;
                    }
                    let id = config.active_scene.clone();
                    config.scenes.retain(|s| s.id != id);
                    config.normalize();
                }
                self.save();
                self.refresh_all();
                self.restart_previews();
            }
            Some(Confirm::RemoveSource(id)) => {
                self.config.borrow_mut().active_scene_mut().sources.retain(|s| s.id != id);
                self.save();
                self.refresh_all();
                self.restart_previews();
            }
            Some(Confirm::QuitWhileRecording) => {
                self.quitting.set(true);
                self.stop_recording(None);
            }
            None => {}
        }
    }

    fn close_requested(&self) -> CloseRequestResponse {
        if self.busy.get() {
            return CloseRequestResponse::KeepWindowShown;
        }
        if self.recording() {
            self.ask(
                Confirm::QuitWhileRecording,
                "Enregistrement en cours",
                "Arrêter l'enregistrement et quitter Polyptyque ? Les fichiers seront finalisés proprement.",
                "Arrêter et quitter",
            );
            return CloseRequestResponse::KeepWindowShown;
        }
        self.previews.borrow_mut().stop();
        CloseRequestResponse::HideWindow
    }

    // ----- Raccourcis clavier globaux --------------------------------------

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn on_hotkey(&self, action: HotkeyAction) {
        log::info!("raccourci : {action:?}");
        match action {
            HotkeyAction::ToggleRecording => self.start_stop(),
            HotkeyAction::ShowWindow => {
                if let Some(ui) = self.ui.upgrade() {
                    let window = ui.window();
                    window.set_minimized(!window.is_minimized());
                }
            }
            HotkeyAction::TogglePreviews => self.toggle_previews(),
        }
    }

    #[cfg(windows)]
    fn register_hotkeys(&self) {
        use polyptyque_windows::hotkeys::HotkeyListener;
        let settings = self.config.borrow().settings.hotkeys.clone();
        let bindings = [
            (settings.toggle_recording.as_str(), HotkeyAction::ToggleRecording),
            (settings.show_window.as_str(), HotkeyAction::ShowWindow),
            (settings.toggle_previews.as_str(), HotkeyAction::TogglePreviews),
        ];
        let parsed: Vec<_> =
            bindings.iter().filter_map(|(text, action)| Some((hotkeys::parse(text)?, *action))).collect();
        // Remplacer l'ancien écouteur libère d'abord ses raccourcis.
        self.hotkeys.borrow_mut().take();
        let (keys, actions): (Vec<_>, Vec<_>) = parsed.into_iter().unzip();
        match HotkeyListener::start(keys, move |index| {
            if let Some(&action) = actions.get(index) {
                post(move |app| app.on_hotkey(action));
            }
        }) {
            Ok((listener, failed)) => {
                *self.hotkeys.borrow_mut() = Some(listener);
                if !failed.is_empty() {
                    self.toast(
                        &format!("Raccourci(s) déjà utilisé(s) par un autre programme : {}", failed.join(", ")),
                        2,
                    );
                }
            }
            Err(e) => log::warn!("raccourcis indisponibles : {e:#}"),
        }
    }

    #[cfg(not(windows))]
    fn register_hotkeys(&self) {}
}

/// Recherche de l'encodeur et ouverture des fenêtres (hors du fil de
/// l'interface : peut durer plusieurs secondes).
fn prepare(platform: &dyn Platform, config: &Config, scene: &Scene) -> Result<Prepared> {
    let ffmpeg = probe::locate_ffmpeg()
        .ok_or_else(|| anyhow!("FFmpeg introuvable : ffmpeg.exe doit se trouver à côté de polyptyque.exe"))?;
    let video = &config.settings.video;
    let encoder = probe::select_encoder(platform, &ffmpeg, video.hardware, video.codec).ok_or_else(|| {
        anyhow!(
            "aucun encodeur {} utilisable avec le réglage « {} ». Essaie un autre matériel ou le format H.264.",
            video.codec.label(),
            video.hardware.label()
        )
    })?;
    post(|app| app.set_busy("Ouverture des fenêtres…"));
    let missing = resolve::prepare_windows(platform, &scene.sources, LAUNCH_WAIT, |event| {
        if let PrepareEvent::Launching(source) = event {
            let text = format!("Ouverture de « {} »…", source.name);
            post(move |app| app.set_busy(&text));
        }
    });
    Ok(Prepared { ffmpeg, encoder, missing })
}
