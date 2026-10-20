//! Polyptyque : enregistrement simultané de plusieurs fenêtres, chacune dans
//! son propre fichier avec son propre son. Interface graphique.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod controller;
mod convert;
#[cfg(not(windows))]
mod demo;
mod format;
mod preview;

use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use polyptyque_core::{naming, paths};
use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

slint::include_modules!();

fn main() {
    let log = init_logging();
    log::info!("Polyptyque {} (interface graphique)", env!("CARGO_PKG_VERSION"));
    if let Err(e) = launch() {
        log::error!("{e:#}");
        show_fatal(&format!("{e:#}\n\nJournal : {}", log.map(|p| p.display().to_string()).unwrap_or_default()));
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn launch() -> Result<()> {
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
    Some(path)
}
