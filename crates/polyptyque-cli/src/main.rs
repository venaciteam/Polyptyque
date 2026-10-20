//! Polyptyque, prototype en console (étape 0).
//!
//! Sert à valider le moteur d'enregistrement avant l'interface graphique :
//! gestion des scènes et des sources, réglages, enregistrement avec suivi en
//! direct. Toute la logique est dans `polyptyque-engine` ; cette console ne
//! fait que la piloter, comme le fera l'interface graphique.

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod convert;
#[cfg(windows)]
mod keys;
#[cfg(windows)]
mod record;
#[cfg(windows)]
mod scenes;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod sources;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
fn main() {
    if let Err(e) = app::run() {
        log::error!("{e:#}");
        eprintln!("\nErreur : {e:#}");
        ui::pause();
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Polyptyque ne fonctionne que sous Windows pour l'instant.");
    std::process::exit(1);
}
