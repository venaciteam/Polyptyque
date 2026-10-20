//! Reconnaissance et lancement des fenêtres des sources.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use polyptyque_core::browsers::{command_line_uses_profile, file_stem, launch_args, window_class, GECKO_USER_JS};
use polyptyque_core::config::{BrowserFamily, Source, SourceKind};
use polyptyque_core::platform::{Platform, WindowInfo};

/// Retrouve la fenêtre d'une source parmi `windows` (ordre d'empilement).
///
/// - Navigateur : la fenêtre principale du processus dont la ligne de
///   commande désigne le profil dédié. Le titre n'intervient pas.
/// - Application : même exécutable, même classe et même titre ; à défaut
///   même classe ; à défaut la seule fenêtre de ce programme.
pub fn find_window(platform: &dyn Platform, source: &Source, windows: &[WindowInfo]) -> Option<WindowInfo> {
    match &source.kind {
        SourceKind::Browser { profile_dir, browser, .. } => {
            let class = window_class(browser.family);
            let mut checked: HashMap<u32, bool> = HashMap::new();
            windows
                .iter()
                .filter(|w| w.class == class)
                .find(|w| {
                    *checked.entry(w.pid).or_insert_with(|| {
                        platform
                            .process_command_line(w.pid)
                            .is_some_and(|line| command_line_uses_profile(&line, profile_dir))
                    })
                })
                .cloned()
        }
        SourceKind::Application { exe_path, window_class, window_title, .. } => {
            let mut same_program: Vec<&WindowInfo> = windows.iter().filter(|w| same_path(&w.exe, exe_path)).collect();
            if same_program.is_empty() {
                // Le programme a pu être mis à jour et changer de dossier
                // (Discord, par exemple) : on se rabat sur le nom du fichier.
                same_program = windows.iter().filter(|w| same_file_name(&w.exe, exe_path)).collect();
            }
            same_program
                .iter()
                .find(|w| &w.class == window_class && &w.title == window_title)
                .or_else(|| same_program.iter().find(|w| &w.class == window_class))
                .or_else(|| (same_program.len() == 1).then(|| &same_program[0]))
                .map(|w| (*w).clone())
        }
    }
}

/// Lance le programme d'une source : le navigateur sur son profil dédié, ou
/// l'application avec ses arguments.
pub fn launch(source: &Source) -> Result<()> {
    match &source.kind {
        SourceKind::Browser { profile_dir, start_url, browser } => {
            let first_launch = prepare_profile(profile_dir, browser.family)?;
            let url = if first_launch { start_url.as_deref() } else { None };
            spawn_detached(&browser.exe_path, &launch_args(browser.family, profile_dir, url))
        }
        SourceKind::Application { exe_path, launch_args, .. } => {
            let args: Vec<OsString> = launch_args.iter().map(OsString::from).collect();
            spawn_detached(exe_path, &args)
        }
    }
}

/// Crée le dossier du profil s'il n'existe pas et y dépose les préférences.
/// Renvoie vrai s'il s'agit du tout premier lancement.
pub fn prepare_profile(profile_dir: &Path, family: BrowserFamily) -> Result<bool> {
    let first_launch = !profile_dir.exists();
    fs::create_dir_all(profile_dir).with_context(|| format!("création du profil {}", profile_dir.display()))?;
    if family == BrowserFamily::Gecko {
        let user_js = profile_dir.join("user.js");
        if !user_js.exists() {
            fs::write(&user_js, GECKO_USER_JS).with_context(|| format!("écriture de {}", user_js.display()))?;
        }
    }
    Ok(first_launch)
}

fn spawn_detached(exe: &Path, args: &[OsString]) -> Result<()> {
    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = exe.parent().filter(|d| d.exists()) {
        command.current_dir(dir);
    }
    command.spawn().with_context(|| format!("lancement de {}", exe.display()))?;
    Ok(())
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

fn same_file_name(a: &Path, b: &Path) -> bool {
    match (file_stem(a), file_stem(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyptyque_core::config::BrowserRef;
    use polyptyque_core::platform::*;
    use std::path::PathBuf;

    struct FakePlatform;

    impl Platform for FakePlatform {
        fn list_windows(&self) -> Vec<WindowInfo> {
            vec![]
        }
        fn is_window_alive(&self, _: WindowHandle) -> bool {
            true
        }
        fn is_window_minimized(&self, _: WindowHandle) -> bool {
            false
        }
        fn process_command_line(&self, pid: u32) -> Option<String> {
            match pid {
                10 => Some(r#"firefox.exe -profile "C:\p\autre""#.into()),
                11 => Some(r#"firefox.exe -profile "C:\p\abc" -no-remote"#.into()),
                _ => None,
            }
        }
        fn detect_browsers(&self) -> Vec<BrowserInstall> {
            vec![]
        }
        fn now_hns(&self) -> u64 {
            0
        }
        fn open_video_capture(&self, _: WindowHandle, _: u32, _: u32, _: bool) -> Result<Box<dyn VideoCapture>> {
            unimplemented!()
        }
        fn open_audio_capture(&self, _: u32, _: u32, _: u16) -> Result<Box<dyn AudioCapture>> {
            unimplemented!()
        }
        fn create_stream_pipe(&self, _: &str) -> Result<Box<dyn StreamPipe>> {
            unimplemented!()
        }
        fn prepare_encoder_process(&self, _: &mut Command) {}
        fn begin_recording_session(&self) {}
        fn end_recording_session(&self) {}
        fn free_disk_space(&self, _: &Path) -> Option<u64> {
            None
        }
    }

    fn window(handle: isize, pid: u32, title: &str, class: &str, exe: &str) -> WindowInfo {
        WindowInfo { handle, pid, title: title.into(), class: class.into(), exe: PathBuf::from(exe) }
    }

    fn source(kind: SourceKind) -> Source {
        Source { id: "x".into(), name: "x".into(), capture_cursor: true, relaunch: false, kind }
    }

    #[test]
    fn browser_is_found_by_profile_not_title() {
        let windows = vec![
            window(1, 10, "Page — Mozilla Firefox", "MozillaWindowClass", r"C:\ff\firefox.exe"),
            window(2, 11, "Page — Mozilla Firefox", "MozillaWindowClass", r"C:\ff\firefox.exe"),
        ];
        let src = source(SourceKind::Browser {
            profile_dir: PathBuf::from(r"C:\p\abc"),
            start_url: None,
            browser: BrowserRef {
                name: "Firefox".into(),
                exe_path: PathBuf::from(r"C:\ff\firefox.exe"),
                family: BrowserFamily::Gecko,
            },
        });
        assert_eq!(find_window(&FakePlatform, &src, &windows).map(|w| w.handle), Some(2));
    }

    #[test]
    fn application_matching_tiers() {
        let exe = r"C:\Discord\app-1.0.2\Discord.exe";
        let windows = vec![
            window(1, 5, "Autre", "Chrome_WidgetWin_1", r"C:\Discord\app-1.0.2\Discord.exe"),
            window(2, 5, "#général - Discord", "Chrome_WidgetWin_1", r"C:\Discord\app-1.0.2\Discord.exe"),
        ];
        let exact = source(SourceKind::Application {
            exe_path: exe.into(),
            window_class: "Chrome_WidgetWin_1".into(),
            window_title: "#général - Discord".into(),
            launch_args: vec![],
        });
        assert_eq!(find_window(&FakePlatform, &exact, &windows).map(|w| w.handle), Some(2));

        // Titre changé : même classe, première fenêtre dans l'ordre d'empilement.
        let renamed = source(SourceKind::Application {
            exe_path: exe.into(),
            window_class: "Chrome_WidgetWin_1".into(),
            window_title: "#autre-salon - Discord".into(),
            launch_args: vec![],
        });
        assert_eq!(find_window(&FakePlatform, &renamed, &windows).map(|w| w.handle), Some(1));

        // Programme mis à jour dans un autre dossier.
        let updated = source(SourceKind::Application {
            exe_path: r"C:\Discord\app-1.0.1\Discord.exe".into(),
            window_class: "Chrome_WidgetWin_1".into(),
            window_title: "#général - Discord".into(),
            launch_args: vec![],
        });
        assert_eq!(find_window(&FakePlatform, &updated, &windows).map(|w| w.handle), Some(2));
    }

    #[test]
    fn unrelated_application_is_not_matched() {
        let windows = vec![window(1, 5, "Jeu", "UnityWndClass", r"C:\Jeux\jeu.exe")];
        let src = source(SourceKind::Application {
            exe_path: r"C:\Autre\programme.exe".into(),
            window_class: "Classe".into(),
            window_title: "Titre".into(),
            launch_args: vec![],
        });
        assert!(find_window(&FakePlatform, &src, &windows).is_none());
    }
}
