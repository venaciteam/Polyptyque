//! Menu principal du prototype.

use std::fs::{self, File};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use console::{style, Term};
use dialoguer::Select;
use polyptyque_core::config::{Config, SourceKind};
use polyptyque_core::naming;
use polyptyque_core::paths;
use polyptyque_core::platform::Platform;
use polyptyque_windows::WindowsPlatform;
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

use crate::{convert, record, scenes, settings, sources, ui};

/// Un enregistrement est en cours : Ctrl+C l'arrête proprement au lieu de
/// fermer le programme.
pub static RECORDING: AtomicBool = AtomicBool::new(false);
pub static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn run() -> Result<()> {
    let log_file = init_logging()?;
    log::info!("Polyptyque {} (prototype console)", env!("CARGO_PKG_VERSION"));
    ctrlc::set_handler(|| {
        if RECORDING.load(Ordering::SeqCst) {
            STOP_REQUESTED.store(true, Ordering::SeqCst);
        } else {
            std::process::exit(0);
        }
    })
    .context("installation du gestionnaire Ctrl+C")?;

    let platform: Arc<dyn Platform> = Arc::new(WindowsPlatform::new());
    let config_path = paths::config_file();
    let (mut config, warning) = Config::load_or_default(&config_path);
    let mut notice = warning;
    let theme = ui::theme();

    loop {
        let term = Term::stdout();
        let _ = term.clear_screen();
        print_header(&config);
        if let Some(message) = notice.take() {
            ui::warning(&message);
            println!();
        }
        let actions = [
            "Démarrer l'enregistrement",
            "Ajouter une source navigateur (Firefox, Chrome, Edge…)",
            "Ajouter une source application (Discord, jeu, Tor Browser…)",
            "Ouvrir les navigateurs de la scène (pour te connecter)",
            "Supprimer une source",
            "Scènes",
            "Réglages",
            "Ouvrir le dossier des enregistrements",
            "Convertir des enregistrements en MP4 (pour le montage)",
            "Quitter",
        ];
        let choice =
            Select::with_theme(&theme).with_prompt("Que veux-tu faire ?").items(actions).default(0).interact()?;
        let result = match choice {
            0 => record::run(&platform, &config),
            1 => sources::add_browser(platform.as_ref(), &mut config),
            2 => sources::add_application(platform.as_ref(), &mut config),
            3 => sources::open_browsers(platform.as_ref(), &config),
            4 => sources::remove(&mut config),
            5 => scenes::menu(&mut config),
            6 => settings::menu(platform.as_ref(), &mut config),
            7 => open_output_dir(&config),
            8 => convert::menu(&platform, &config),
            _ => break,
        };
        if let Err(e) = result {
            log::error!("{e:#}");
            ui::failure(&format!("{e:#}"));
            ui::pause();
        }
        config.normalize();
        if let Err(e) = config.save(&config_path) {
            notice = Some(format!("Impossible d'enregistrer la configuration : {e:#}"));
        }
    }
    log::info!("fermeture ; journal : {}", log_file.display());
    Ok(())
}

fn print_header(config: &Config) {
    let video = &config.settings.video;
    println!("{}", style(" Polyptyque · prototype (étape 0) ").bold().reverse());
    println!();
    println!(
        "Scène : {}    {}",
        style(&config.active_scene().name).bold().cyan(),
        style(format!(
            "{}x{} · {} i/s · {} kb/s · {}",
            video.width,
            video.height,
            video.fps,
            ui::thousands(video.bitrate_kbps as u64),
            video.codec.label()
        ))
        .dim()
    );
    let scene = config.active_scene();
    if scene.sources.is_empty() {
        println!("{}", style("Aucune source pour l'instant.").dim());
    } else {
        for (i, source) in scene.sources.iter().enumerate() {
            let kind = match &source.kind {
                SourceKind::Browser { browser, .. } => format!("navigateur {}", browser.name),
                SourceKind::Application { exe_path, .. } => format!(
                    "application {}",
                    exe_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
                ),
            };
            println!("  {}. {}  {}", i + 1, style(&source.name).bold(), style(format!("({kind})")).dim());
        }
    }
    println!("{}", style(format!("Dossier : {}", config.settings.output_dir.display())).dim());
    println!();
}

fn open_output_dir(config: &Config) -> Result<()> {
    let dir = &config.settings.output_dir;
    fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
    Command::new("explorer").arg(dir).spawn().context("ouverture de l'Explorateur")?;
    Ok(())
}

fn init_logging() -> Result<std::path::PathBuf> {
    let dir = paths::logs_dir();
    fs::create_dir_all(&dir).with_context(|| format!("création de {}", dir.display()))?;
    let (date, time) = naming::now_parts();
    let path = dir.join(format!("polyptyque_{date}_{time}.log"));
    let file = File::create(&path).with_context(|| format!("création de {}", path.display()))?;
    let mut builder = ConfigBuilder::new();
    let _ = builder.set_time_offset_to_local();
    let _ = WriteLogger::init(LevelFilter::Info, builder.build(), file);
    std::panic::set_hook(Box::new(|info| {
        log::error!("erreur interne : {info}");
        eprintln!("Erreur interne : {info}");
    }));
    Ok(path)
}
