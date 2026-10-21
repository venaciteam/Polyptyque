//! Emplacements standards des fichiers de Polyptyque.
//!
//! Sous Windows :
//! - configuration : `%APPDATA%\Polyptyque\config.toml`
//! - profils de navigateur et journaux : `%LOCALAPPDATA%\Polyptyque\…`
//! - enregistrements : `Vidéos\Polyptyque`

use std::path::PathBuf;

use directories::{BaseDirs, UserDirs};

const APP_DIR: &str = "Polyptyque";

pub fn config_file() -> PathBuf {
    BaseDirs::new().map(|b| b.config_dir().join(APP_DIR)).unwrap_or_else(|| PathBuf::from(APP_DIR)).join("config.toml")
}

pub fn local_data_dir() -> PathBuf {
    BaseDirs::new().map(|b| b.data_local_dir().join(APP_DIR)).unwrap_or_else(|| PathBuf::from(APP_DIR))
}

pub fn profiles_dir() -> PathBuf {
    local_data_dir().join("profils")
}

pub fn logs_dir() -> PathBuf {
    local_data_dir().join("journaux")
}

/// Journal de l'enregistrement en cours (voir `recovery`).
pub fn session_record_file() -> PathBuf {
    local_data_dir().join("session-en-cours.toml")
}

pub fn default_output_dir() -> PathBuf {
    UserDirs::new()
        .and_then(|u| u.video_dir().map(|p| p.join(APP_DIR)))
        .or_else(|| BaseDirs::new().map(|b| b.home_dir().join("Videos").join(APP_DIR)))
        .unwrap_or_else(|| PathBuf::from(APP_DIR))
}
