//! Journal de la session d'enregistrement en cours.
//!
//! Écrit au départ d'un enregistrement et tenu à jour (nouvelles parties,
//! sources ajoutées), il est effacé à l'arrêt normal. S'il existe encore au
//! lancement suivant, Polyptyque s'est arrêté pendant un enregistrement
//! (plantage, coupure de courant, écran bleu) : il sert alors à faire le
//! bilan des fichiers et à proposer leur réparation et la reprise.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::storage;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionRecord {
    /// Date et heure de départ (« 2026-10-21 » et « 08h44 »).
    pub date: String,
    pub time: String,
    pub scene_id: String,
    pub scene_name: String,
    /// Fichiers de la session, dans l'ordre (parties et micro compris).
    pub files: Vec<PathBuf>,
}

impl SessionRecord {
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("journal de session")?;
        storage::write_atomic(path, text.as_bytes())
    }

    /// `Ok(None)` : aucun enregistrement n'était en cours. Une erreur
    /// signale un journal présent mais illisible : il y a bien eu un
    /// incident, sans le détail des fichiers.
    pub fn load(path: &Path) -> Result<Option<SessionRecord>> {
        match fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map(Some).context("journal de session illisible"),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("journal de session illisible"),
        }
    }

    pub fn clear(path: &Path) {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trip() {
        let dir = std::env::temp_dir().join(format!("polyptyque-recovery-{}", crate::config::new_id()));
        let path = dir.join("session-en-cours.toml");
        assert_eq!(SessionRecord::load(&path).unwrap(), None);
        let record = SessionRecord {
            date: "2026-10-21".into(),
            time: "08h44".into(),
            scene_id: "abc".into(),
            scene_name: "Scène principale".into(),
            files: vec![PathBuf::from("J:/a.mkv"), PathBuf::from("J:/a_partie2.mkv")],
        };
        record.save(&path).unwrap();
        assert_eq!(SessionRecord::load(&path).unwrap(), Some(record));
        SessionRecord::clear(&path);
        assert_eq!(SessionRecord::load(&path).unwrap(), None);
        fs::write(&path, "= = =").unwrap();
        assert!(SessionRecord::load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
