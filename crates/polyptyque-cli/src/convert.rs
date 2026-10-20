//! Conversion sans perte des enregistrements MKV en MP4 pour le montage.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{bail, Result};
use dialoguer::Input;
use polyptyque_core::config::Config;
use polyptyque_core::platform::Platform;
use polyptyque_engine::{probe, remux};

use crate::ui;

/// Menu : un fichier, ou tous les MKV d'un dossier qui n'ont pas encore
/// leur MP4.
pub fn menu(platform: &Arc<dyn Platform>, config: &Config) -> Result<()> {
    let text: String = Input::with_theme(&ui::theme())
        .with_prompt("Fichier MKV ou dossier à convertir")
        .default(config.settings.output_dir.display().to_string())
        .interact_text()?;
    let path = PathBuf::from(text.trim().trim_matches('"'));
    let files: Vec<PathBuf> = if path.is_dir() {
        fs::read_dir(&path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                remux::is_convertible(p) && !p.with_extension("mp4").exists() && !p.with_extension("m4a").exists()
            })
            .collect()
    } else {
        vec![path]
    };
    if files.is_empty() {
        ui::warning("Aucun fichier MKV à convertir ici.");
    } else {
        convert_all(platform.as_ref(), &files)?;
    }
    ui::pause();
    Ok(())
}

/// Convertit les fichiers un par un en affichant l'avancement.
pub fn convert_all(platform: &dyn Platform, files: &[PathBuf]) -> Result<()> {
    let Some(ffmpeg) = probe::locate_ffmpeg() else {
        bail!("FFmpeg introuvable : le fichier ffmpeg.exe doit se trouver à côté de polyptyque.exe");
    };
    let cancel = AtomicBool::new(false);
    for (index, file) in files.iter().enumerate() {
        let name = file_name(file);
        let label = format!("Conversion MP4 {}/{} : {name}", index + 1, files.len());
        print!("{label}…");
        let _ = std::io::stdout().flush();
        let result = remux::convert(
            platform,
            &ffmpeg,
            file,
            &mut |progress| {
                print!("\r{label} · {:.0} %   ", progress * 100.0);
                let _ = std::io::stdout().flush();
            },
            &cancel,
        );
        println!();
        match result {
            Ok(output) => ui::success(&format!("{} créé", file_name(&output))),
            Err(e) => ui::failure(&format!("{name} : {e:#}")),
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}
