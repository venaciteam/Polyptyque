//! Écriture sûre des fichiers de Polyptyque (réglages, journal de session).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Écrit `data` dans `path` sans jamais laisser un fichier à moitié écrit :
/// fichier temporaire forcé sur le disque, puis renommage. Une coupure de
/// courant laisse soit l'ancien contenu, soit le nouveau.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("création du dossier {}", dir.display()))?;
    }
    let tmp = sibling(path, ".tmp");
    let mut file = File::create(&tmp).with_context(|| format!("écriture de {}", tmp.display()))?;
    file.write_all(data).with_context(|| format!("écriture de {}", tmp.display()))?;
    file.sync_all().with_context(|| format!("écriture sur le disque de {}", tmp.display()))?;
    drop(file);
    fs::rename(&tmp, path).with_context(|| format!("remplacement de {}", path.display()))?;
    Ok(())
}

/// `config.toml` + `.precedent` → `config.toml.precedent`.
pub fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join(format!("polyptyque-storage-{}", crate::config::new_id()));
        let path = dir.join("sous-dossier").join("fichier.toml");
        write_atomic(&path, b"premier").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert!(!sibling(&path, ".tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sibling_appends_suffix() {
        assert_eq!(sibling(Path::new("a/config.toml"), ".precedent"), Path::new("a/config.toml.precedent"));
    }
}
