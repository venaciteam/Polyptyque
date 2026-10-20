//! Gestion des scènes : chaque scène est un groupe de sources, pour un
//! usage donné (« Podcast », « Archive »…).

use anyhow::Result;
use dialoguer::{Confirm, Input, Select};
use polyptyque_core::config::{Config, Scene};

use crate::ui;

pub fn menu(config: &mut Config) -> Result<()> {
    let theme = ui::theme();
    loop {
        let mut items: Vec<String> = config
            .scenes
            .iter()
            .map(|scene| {
                let marker = if scene.id == config.active_scene { "●" } else { " " };
                format!("{marker} {} ({} source(s))", scene.name, scene.sources.len())
            })
            .collect();
        let scenes = items.len();
        items.extend([
            "Créer une scène".to_string(),
            "Renommer la scène active".to_string(),
            "Supprimer la scène active".to_string(),
            "Retour".to_string(),
        ]);
        let choice = Select::with_theme(&theme)
            .with_prompt("Choisis la scène active, ou une action")
            .items(&items)
            .default(items.len() - 1)
            .interact()?;
        match choice {
            i if i < scenes => {
                config.active_scene = config.scenes[i].id.clone();
                return Ok(());
            }
            i if i == scenes => {
                let name: String = Input::with_theme(&theme).with_prompt("Nom de la nouvelle scène").interact_text()?;
                let scene = Scene::new(name.trim());
                config.active_scene = scene.id.clone();
                config.scenes.push(scene);
                return Ok(());
            }
            i if i == scenes + 1 => {
                let current = config.active_scene().name.clone();
                let name: String =
                    Input::with_theme(&theme).with_prompt("Nouveau nom").default(current).interact_text()?;
                config.active_scene_mut().name = name.trim().to_string();
            }
            i if i == scenes + 2 => {
                if config.scenes.len() == 1 {
                    ui::warning("Il faut garder au moins une scène.");
                    ui::pause();
                    continue;
                }
                let name = config.active_scene().name.clone();
                if Confirm::with_theme(&theme)
                    .with_prompt(format!("Supprimer la scène « {name} » et ses sources ?"))
                    .default(false)
                    .interact()?
                {
                    let id = config.active_scene.clone();
                    config.scenes.retain(|s| s.id != id);
                    config.normalize();
                }
            }
            _ => return Ok(()),
        }
    }
}
