//! Polyptyque : enregistrement simultané de plusieurs fenêtres, chacune dans
//! son propre fichier avec son propre son. Interface graphique.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod controller;
mod convert;
#[cfg(not(windows))]
mod demo;
mod format;
mod preview;
mod update;

use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use polyptyque_core::{naming, paths, storage};
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

slint::include_modules!();

/// Version installée par une mise à jour, si Polyptyque vient d'être
/// relancé par elle.
pub static UPDATED_TO: OnceLock<String> = OnceLock::new();

fn main() {
    // Installateur et mises à jour : doit passer en premier (il peut
    // terminer le programme, par exemple pendant une installation).
    #[cfg(windows)]
    velopack::VelopackApp::build()
        .on_restarted(|version| {
            let _ = UPDATED_TO.set(version.to_string());
        })
        .run();
    let log = init_logging();
    log::info!("Polyptyque {} (interface graphique)", env!("CARGO_PKG_VERSION"));
    if let Some(root) = paths::portable_root() {
        log::info!("version portable : données dans {}", root.join(paths::PORTABLE_DATA_DIR).display());
    }
    if let Err(e) = launch() {
        log::error!("{e:#}");
        show_fatal(&format!("{e:#}\n\nJournal : {}", log.map(|p| p.display().to_string()).unwrap_or_default()));
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn launch() -> Result<()> {
    use polyptyque_windows::single_instance::{self, Instance};
    // Tenu jusqu'à la fermeture de Polyptyque.
    let _instance = match single_instance::acquire() {
        Instance::First(lock) => lock,
        Instance::AlreadyOpen => {
            log::info!("Polyptyque est déjà ouvert : sa fenêtre est ramenée au premier plan");
            return Ok(());
        }
    };
    let platform = Arc::new(polyptyque_windows::WindowsPlatform::new());
    let paths = controller::Paths { config: paths::config_file(), profiles: paths::profiles_dir() };
    controller::run(platform, paths, None)
}

/// Hors Windows : plateforme de démonstration, dans un dossier temporaire.
#[cfg(not(windows))]
fn launch() -> Result<()> {
    let root = std::env::temp_dir().join("polyptyque-demo");
    let platform = demo::DemoPlatform::new(&root);
    let config = root.join("config.toml");
    let initial = (!config.exists()).then(|| platform.seed_config());
    let paths = controller::Paths { config, profiles: root.join("profils") };
    controller::run(Arc::new(platform), paths, initial)
}

fn show_fatal(message: &str) {
    #[cfg(windows)]
    {
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("Polyptyque")
            .set_description(message)
            .show();
    }
    #[cfg(not(windows))]
    eprintln!("Erreur : {message}");
}

fn init_logging() -> Option<PathBuf> {
    let dir = paths::logs_dir();
    fs::create_dir_all(&dir).ok()?;
    let (date, time) = naming::now_parts();
    let path = dir.join(format!("polyptyque_{date}_{time}.log"));
    let file = File::create(&path).ok()?;
    let mut builder = ConfigBuilder::new();
    let _ = builder.set_time_offset_to_local();
    let _ = WriteLogger::init(LevelFilter::Info, builder.build(), file);
    std::panic::set_hook(Box::new(|info| log::error!("erreur interne : {info}")));
    let removed = storage::prune_old_logs(&dir, storage::LOG_RETENTION);
    if removed > 0 {
        log::info!("{removed} journal(aux) de plus de 30 jours effacé(s)");
    }
    Some(path)
}
