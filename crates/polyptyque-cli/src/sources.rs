//! Ajout, suppression et ouverture des sources.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input, Select};
use polyptyque_core::browsers::{classify_executable, file_stem, is_tor_browser};
use polyptyque_core::config::{new_id, BrowserFamily, BrowserRef, Config, Source, SourceKind};
use polyptyque_core::paths;
use polyptyque_core::platform::Platform;
use polyptyque_engine::resolve;

use crate::ui;

fn family_label(family: BrowserFamily) -> &'static str {
    match family {
        BrowserFamily::Gecko => "famille Firefox",
        BrowserFamily::Chromium => "famille Chromium",
    }
}

pub fn add_browser(platform: &dyn Platform, config: &mut Config) -> Result<()> {
    let theme = ui::theme();
    let installed = platform.detect_browsers();
    let mut items: Vec<String> = installed.iter().map(|b| format!("{} ({})", b.name, family_label(b.family))).collect();
    items.push("Autre navigateur (indiquer son fichier .exe)".into());
    items.push("Annuler".into());
    let choice = Select::with_theme(&theme)
        .with_prompt("Quel navigateur utiliser pour cette source ?")
        .items(&items)
        .default(0)
        .interact()?;

    let browser = if choice < installed.len() {
        let b = &installed[choice];
        BrowserRef { name: b.name.clone(), exe_path: b.exe_path.clone(), family: b.family }
    } else if choice == installed.len() {
        let path: String =
            Input::with_theme(&theme).with_prompt("Chemin complet du fichier .exe du navigateur").interact_text()?;
        let exe = PathBuf::from(path.trim().trim_matches('"'));
        if !exe.exists() {
            ui::failure("Ce fichier n'existe pas.");
            ui::pause();
            return Ok(());
        }
        if is_tor_browser(&exe) {
            ui::warning(
                "Tor Browser s'ajoute comme source application (il efface tout à la fermeture, par conception).",
            );
            ui::pause();
            return Ok(());
        }
        let family = match classify_executable(&exe) {
            Some(family) => family,
            None => {
                let families = ["Basé sur Firefox", "Basé sur Chromium (Chrome, Edge, Brave, Opera…)"];
                match Select::with_theme(&theme)
                    .with_prompt("Sur quoi ce navigateur est-il basé ?")
                    .items(families)
                    .default(1)
                    .interact()?
                {
                    0 => BrowserFamily::Gecko,
                    _ => BrowserFamily::Chromium,
                }
            }
        };
        let name = file_stem(&exe).map(|s| capitalize(&s)).unwrap_or_else(|| "Navigateur".into());
        BrowserRef { name, exe_path: exe, family }
    } else {
        return Ok(());
    };

    let count = config.active_scene().sources.len() + 1;
    let name: String = Input::with_theme(&theme)
        .with_prompt("Nom de la source (utilisé dans le nom du fichier)")
        .default(format!("{} {count}", short_name(&browser.name)))
        .interact_text()?;
    let url: String = Input::with_theme(&theme)
        .with_prompt("Adresse à ouvrir au premier lancement (facultatif)")
        .allow_empty(true)
        .interact_text()?;

    let id = new_id();
    let source = Source {
        name: name.trim().to_string(),
        capture_cursor: true,
        relaunch: true,
        kind: SourceKind::Browser {
            profile_dir: paths::profiles_dir().join(&id),
            start_url: Some(url.trim().to_string()).filter(|u| !u.is_empty()),
            browser,
        },
        id,
    };
    config.active_scene_mut().sources.push(source.clone());
    ui::success(&format!("Source « {} » ajoutée avec son propre profil de navigateur.", source.name));
    println!(
        "{}",
        style("Tout son joué dans ce profil (tous ses onglets) sera enregistré dans le fichier de cette source.").dim()
    );

    if Confirm::with_theme(&theme)
        .with_prompt("Ouvrir ce navigateur maintenant pour te connecter et préparer tes onglets ?")
        .default(true)
        .interact()?
    {
        resolve::launch(&source)?;
        ui::success("Navigateur lancé. Tes connexions et tes onglets seront conservés dans ce profil.");
        ui::pause();
    }
    Ok(())
}

pub fn add_application(platform: &dyn Platform, config: &mut Config) -> Result<()> {
    let theme = ui::theme();
    loop {
        let windows = platform.list_windows();
        let mut items: Vec<String> = windows
            .iter()
            .map(|w| {
                let program = w.exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                format!("{}  —  {}", ui::truncate(&w.title, 60), program)
            })
            .collect();
        items.push("Actualiser la liste".into());
        items.push("Annuler".into());
        let choice = Select::with_theme(&theme)
            .with_prompt("Quelle fenêtre enregistrer ?")
            .items(&items)
            .default(0)
            .interact()?;
        if choice == windows.len() {
            continue;
        }
        if choice > windows.len() {
            return Ok(());
        }
        let window = &windows[choice];

        if classify_executable(&window.exe).is_some() {
            ui::warning(
                "C'est un navigateur : son son sera mélangé avec celui de toutes ses autres fenêtres. \
                 Pour isoler le son, ajoute plutôt une source navigateur.",
            );
            if !Confirm::with_theme(&theme).with_prompt("Ajouter quand même ?").default(false).interact()? {
                return Ok(());
            }
        }

        let default_name =
            file_stem(&window.exe).map(|s| capitalize(&s)).unwrap_or_else(|| ui::truncate(&window.title, 30));
        let name: String = Input::with_theme(&theme)
            .with_prompt("Nom de la source (utilisé dans le nom du fichier)")
            .default(default_name)
            .interact_text()?;
        let relaunch = Confirm::with_theme(&theme)
            .with_prompt(
                "Relancer ce programme automatiquement s'il n'est pas ouvert au démarrage d'un enregistrement ?",
            )
            .default(false)
            .interact()?;
        let capture_cursor = Confirm::with_theme(&theme)
            .with_prompt("Afficher le curseur de la souris dans l'enregistrement ?")
            .default(true)
            .interact()?;

        let source = Source {
            id: new_id(),
            name: name.trim().to_string(),
            capture_cursor,
            relaunch,
            kind: SourceKind::Application {
                exe_path: window.exe.clone(),
                window_class: window.class.clone(),
                window_title: window.title.clone(),
                launch_args: Vec::new(),
            },
        };
        ui::success(&format!("Source « {} » ajoutée.", source.name));
        config.active_scene_mut().sources.push(source);
        ui::pause();
        return Ok(());
    }
}

pub fn open_browsers(platform: &dyn Platform, config: &Config) -> Result<()> {
    let windows = platform.list_windows();
    let mut any = false;
    for source in &config.active_scene().sources {
        if !matches!(source.kind, SourceKind::Browser { .. }) {
            continue;
        }
        any = true;
        if resolve::find_window(platform, source, &windows).is_some() {
            ui::success(&format!("{} : déjà ouvert.", source.name));
        } else {
            match resolve::launch(source) {
                Ok(()) => ui::success(&format!("{} : lancé.", source.name)),
                Err(e) => ui::failure(&format!("{} : {e:#}", source.name)),
            }
        }
    }
    if !any {
        ui::warning("Cette scène ne contient aucune source navigateur.");
    }
    ui::pause();
    Ok(())
}

pub fn remove(config: &mut Config) -> Result<()> {
    let theme = ui::theme();
    let scene = config.active_scene();
    if scene.sources.is_empty() {
        ui::warning("Aucune source à supprimer.");
        ui::pause();
        return Ok(());
    }
    let mut items: Vec<String> = scene.sources.iter().map(|s| s.name.clone()).collect();
    items.push("Annuler".into());
    let choice = Select::with_theme(&theme)
        .with_prompt("Quelle source supprimer ?")
        .items(&items)
        .default(items.len() - 1)
        .interact()?;
    if choice >= scene.sources.len() {
        return Ok(());
    }
    let source = scene.sources[choice].clone();
    if !Confirm::with_theme(&theme).with_prompt(format!("Supprimer « {} » ?", source.name)).default(false).interact()?
    {
        return Ok(());
    }
    if let SourceKind::Browser { profile_dir, .. } = &source.kind {
        if profile_dir.exists()
            && Confirm::with_theme(&theme)
                .with_prompt("Supprimer aussi son profil de navigateur (connexions, cookies, onglets) ?")
                .default(false)
                .interact()?
        {
            if let Err(e) = fs::remove_dir_all(profile_dir) {
                ui::failure(&format!("Profil non supprimé (navigateur encore ouvert ?) : {e}"));
                ui::pause();
            }
        }
    }
    config.active_scene_mut().sources.remove(choice);
    Ok(())
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// « Mozilla Firefox » → « Firefox », « Google Chrome » → « Chrome ».
fn short_name(name: &str) -> String {
    name.split_whitespace().last().unwrap_or(name).to_string()
}
