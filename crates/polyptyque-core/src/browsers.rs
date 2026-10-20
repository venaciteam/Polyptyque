//! Navigateurs gérés : reconnaissance des familles, arguments de lancement
//! avec un profil dédié, reconnaissance d'un profil dans une ligne de
//! commande.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::config::BrowserFamily;

/// Famille d'un navigateur d'après le nom de son exécutable.
pub fn classify_executable(exe: &Path) -> Option<BrowserFamily> {
    if is_tor_browser(exe) {
        return None;
    }
    let stem = file_stem(exe)?;
    match stem.as_str() {
        "firefox" | "librewolf" | "waterfox" | "zen" | "floorp" | "mercury" => Some(BrowserFamily::Gecko),
        "chrome" | "msedge" | "brave" | "vivaldi" | "opera" | "chromium" | "thorium" => Some(BrowserFamily::Chromium),
        _ => None,
    }
}

/// Nom du fichier sans extension, en minuscules. Accepte les deux types de
/// séparateurs, pour lire des chemins Windows depuis n'importe quel système.
pub fn file_stem(path: &Path) -> Option<String> {
    program_name(path).map(|name| name.to_lowercase())
}

/// Nom du programme tel qu'il est écrit : « Discord » pour `…\Discord.exe`.
pub fn program_name(path: &Path) -> Option<String> {
    let text = path.to_string_lossy();
    let name = text.rsplit(['\\', '/']).next().filter(|n| !n.is_empty())?;
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    Some(stem.to_string())
}

/// Tor Browser embarque un `firefox.exe` : il ne doit pas être géré comme un
/// navigateur (pas de profil persistant, pas de connexions conservées).
pub fn is_tor_browser(exe: &Path) -> bool {
    let path = exe.to_string_lossy().to_lowercase();
    path.contains("tor browser") || path.contains("torbrowser")
}

/// Classe des fenêtres principales de chaque famille.
pub fn window_class(family: BrowserFamily) -> &'static str {
    match family {
        BrowserFamily::Gecko => "MozillaWindowClass",
        BrowserFamily::Chromium => "Chrome_WidgetWin_1",
    }
}

/// Arguments pour lancer un navigateur sur son profil dédié. `url` n'est
/// passée qu'au premier lancement : ensuite, le navigateur restaure lui-même
/// les onglets de la session précédente.
pub fn launch_args(family: BrowserFamily, profile_dir: &Path, url: Option<&str>) -> Vec<OsString> {
    let mut args: Vec<OsString> = match family {
        BrowserFamily::Gecko => vec!["-profile".into(), profile_dir.as_os_str().to_owned(), "-no-remote".into()],
        BrowserFamily::Chromium => {
            let mut dir = OsString::from("--user-data-dir=");
            dir.push(profile_dir.as_os_str());
            vec![dir, "--no-first-run".into(), "--no-default-browser-check".into(), "--restore-last-session".into()]
        }
    };
    if let Some(url) = url.filter(|u| !u.trim().is_empty()) {
        args.push(url.trim().into());
    }
    args
}

/// Préférences écrites dans les profils Firefox gérés : restaurer la
/// session précédente et éviter les questions au démarrage.
pub const GECKO_USER_JS: &str = r#"// Préférences écrites par Polyptyque pour ce profil dédié.
user_pref("browser.startup.page", 3);
user_pref("browser.sessionstore.resume_from_crash", true);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.aboutwelcome.enabled", false);
user_pref("datareporting.policy.dataSubmissionPolicyBypassNotification", true);
"#;

/// Vrai si la ligne de commande d'un processus désigne ce dossier de profil.
/// Insensible à la casse et au type de barre oblique, et sans confondre
/// `…\abc` avec `…\abcd`.
pub fn command_line_uses_profile(command_line: &str, profile_dir: &Path) -> bool {
    let normalize = |s: &str| s.replace('/', "\\").to_lowercase();
    let needle = normalize(&profile_dir.to_string_lossy());
    let needle = needle.trim_end_matches('\\');
    if needle.is_empty() {
        return false;
    }
    let haystack = normalize(command_line);
    let mut from = 0;
    while let Some(pos) = haystack[from..].find(needle) {
        let end = from + pos + needle.len();
        let rest = &haystack[end..];
        let boundary = rest.is_empty()
            || rest.starts_with('"')
            || rest.starts_with(' ')
            || rest.starts_with('\'')
            || rest == "\\"
            || rest.starts_with("\\\"")
            || rest.starts_with("\\ ");
        if boundary {
            return true;
        }
        from = end;
    }
    false
}

/// Extrait le chemin de l'exécutable d'une commande du registre, par
/// exemple `"C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1"`.
pub fn exe_from_command(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(PathBuf::from(&rest[..end]));
    }
    let lower = command.to_lowercase();
    let end = lower.find(".exe").map(|i| i + 4).unwrap_or(command.len());
    let path = command[..end].trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_browsers() {
        assert_eq!(
            classify_executable(Path::new(r"C:\Program Files\Mozilla Firefox\firefox.exe")),
            Some(BrowserFamily::Gecko)
        );
        assert_eq!(
            classify_executable(Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe")),
            Some(BrowserFamily::Chromium)
        );
        assert_eq!(classify_executable(Path::new(r"C:\x\Discord.exe")), None);
        assert_eq!(classify_executable(Path::new(r"C:\Users\x\Desktop\Tor Browser\Browser\firefox.exe")), None);
    }

    #[test]
    fn gecko_args() {
        let args = launch_args(BrowserFamily::Gecko, Path::new(r"C:\p\abc"), Some("https://a.b"));
        assert_eq!(
            args,
            vec![OsString::from("-profile"), r"C:\p\abc".into(), "-no-remote".into(), "https://a.b".into()]
        );
    }

    #[test]
    fn chromium_args_without_url() {
        let args = launch_args(BrowserFamily::Chromium, Path::new(r"C:\p\abc"), Some("  "));
        assert_eq!(args[0], OsString::from(r"--user-data-dir=C:\p\abc"));
        assert_eq!(args.len(), 4);
    }

    #[test]
    fn profile_matching() {
        let profile = Path::new(r"C:\Users\X\AppData\Local\Polyptyque\profils\abc");
        let gecko = r#""C:\Program Files\Mozilla Firefox\firefox.exe" -profile "C:\Users\x\AppData\Local\Polyptyque\profils\abc" -no-remote"#;
        assert!(command_line_uses_profile(gecko, profile));
        let chromium = r#""chrome.exe" --user-data-dir=C:\Users\x\AppData\Local\Polyptyque\profils\abc --no-first-run"#;
        assert!(command_line_uses_profile(chromium, profile));
        let other = r#""firefox.exe" -profile "C:\Users\x\AppData\Local\Polyptyque\profils\abcd""#;
        assert!(!command_line_uses_profile(other, profile));
        let end = r#"chrome.exe --user-data-dir=C:/Users/x/AppData/Local/Polyptyque/profils/abc"#;
        assert!(command_line_uses_profile(end, profile));
        assert!(!command_line_uses_profile("firefox.exe", profile));
    }

    #[test]
    fn parses_registry_commands() {
        assert_eq!(
            exe_from_command(r#""C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1""#),
            Some(PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"))
        );
        assert_eq!(
            exe_from_command(r"C:\Browsers\brave.exe --single-argument %1"),
            Some(PathBuf::from(r"C:\Browsers\brave.exe"))
        );
        assert_eq!(exe_from_command(""), None);
    }
}
