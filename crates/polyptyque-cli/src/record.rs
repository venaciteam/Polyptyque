//! Enregistrement : préparation des fenêtres, suivi en direct, arrêt propre.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use console::{style, Term};
use dialoguer::Confirm;
use polyptyque_core::config::{Config, Source};
use polyptyque_core::encoder::bytes_per_hour;
use polyptyque_core::naming;
use polyptyque_core::paths;
use polyptyque_core::platform::Platform;
use polyptyque_engine::{probe, resolve, Session, SessionPlan, SourceState, SourceStatus};

use crate::app::{RECORDING, STOP_REQUESTED};
use crate::{keys, ui};

/// Attente maximale des fenêtres lancées par Polyptyque.
const LAUNCH_WAIT: Duration = Duration::from_secs(30);
/// En dessous, l'enregistrement s'arrête proprement pour protéger les fichiers.
const DISK_STOP_THRESHOLD: u64 = 1_000_000_000;
const DISK_WARNING_THRESHOLD: u64 = 10_000_000_000;
/// Au-delà, l'encodeur ne suit plus le rythme de la capture.
const BACKLOG_WARNING_SECONDS: f32 = 2.0;

pub fn run(platform: &Arc<dyn Platform>, config: &Config) -> Result<()> {
    let theme = ui::theme();
    let scene = config.active_scene().clone();
    let settings = config.settings.clone();
    if scene.sources.is_empty() {
        ui::warning("Ajoute d'abord au moins une source à cette scène.");
        ui::pause();
        return Ok(());
    }

    let Some(ffmpeg) = probe::locate_ffmpeg() else {
        bail!("FFmpeg introuvable : le fichier ffmpeg.exe doit se trouver à côté de polyptyque.exe");
    };
    println!("Recherche du meilleur encodeur disponible…");
    let video = &settings.video;
    let Some(encoder) = probe::select_encoder(platform.as_ref(), &ffmpeg, video.hardware, video.codec) else {
        bail!(
            "aucun encodeur {} utilisable avec le réglage « {} ». Essaie un autre matériel ou le format H.264.",
            video.codec.label(),
            video.hardware.label()
        );
    };
    ui::success(&format!("Encodeur : {}", encoder.label()));

    let missing = prepare_windows(platform.as_ref(), &scene.sources);
    if !missing.is_empty() {
        ui::warning(&format!(
            "Fenêtre(s) introuvable(s) : {}. Elles seront rattachées dès qu'elles apparaîtront (image noire en attendant).",
            missing.join(", ")
        ));
    }

    let per_hour = bytes_per_hour(video.bitrate_kbps, settings.audio.bitrate_kbps) * scene.sources.len() as u64;
    if let Some(free) = platform.free_disk_space(&settings.output_dir) {
        let autonomy = free as f64 / per_hour as f64 * 3600.0;
        println!(
            "Espace libre : {} · environ {} d'enregistrement pour {} source(s).",
            ui::bytes(free),
            ui::hours(autonomy),
            scene.sources.len()
        );
        if free < per_hour {
            ui::warning("Moins d'une heure d'enregistrement possible sur ce disque.");
        }
    }

    if !Confirm::with_theme(&theme).with_prompt("Démarrer l'enregistrement ?").default(true).interact()? {
        return Ok(());
    }

    let (date, time) = naming::now_parts();
    let log_dir = paths::logs_dir().join(format!("session_{date}_{time}"));
    STOP_REQUESTED.store(false, Ordering::SeqCst);
    RECORDING.store(true, Ordering::SeqCst);
    let plan = SessionPlan {
        scene_name: scene.name.clone(),
        sources: scene.sources.clone(),
        settings: settings.clone(),
        encoder,
        ffmpeg,
        log_dir: log_dir.clone(),
    };
    let session = match Session::start(platform.clone(), plan) {
        Ok(session) => session,
        Err(e) => {
            RECORDING.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    let header = format!(
        "{} · {}x{} · {} i/s · {} kb/s",
        encoder.label(),
        video.width,
        video.height,
        video.fps,
        ui::thousands(video.bitrate_kbps as u64)
    );
    let session = monitor(session, &header, &scene.sources);

    println!("\nArrêt en cours : les fichiers sont finalisés…");
    let summaries = session.stop();
    RECORDING.store(false, Ordering::SeqCst);

    println!();
    for summary in &summaries {
        let line = format!(
            "{} : {} ({}, {} image(s) perdue(s))",
            summary.name,
            summary.output.display(),
            ui::bytes(summary.file_size),
            ui::thousands(summary.frames_dropped)
        );
        match &summary.error {
            None => ui::success(&line),
            Some(error) => {
                ui::failure(&line);
                println!("    {}", style(error).red());
            }
        }
    }
    println!("{}", style(format!("Journaux : {}", log_dir.display())).dim());
    ui::pause();
    Ok(())
}

/// Retrouve ou lance la fenêtre de chaque source. Renvoie les noms des
/// sources encore introuvables.
fn prepare_windows(platform: &dyn Platform, sources: &[Source]) -> Vec<String> {
    let windows = platform.list_windows();
    let mut waiting: Vec<&Source> = Vec::new();
    let mut missing = Vec::new();
    for source in sources {
        if resolve::find_window(platform, source, &windows).is_some() {
            ui::success(&format!("{} : fenêtre trouvée.", source.name));
        } else if source.should_launch() {
            match resolve::launch(source) {
                Ok(()) => {
                    println!("… {} : lancement.", source.name);
                    waiting.push(source);
                }
                Err(e) => {
                    ui::failure(&format!("{} : {e:#}", source.name));
                    missing.push(source.name.clone());
                }
            }
        } else {
            missing.push(source.name.clone());
        }
    }
    let deadline = Instant::now() + LAUNCH_WAIT;
    while !waiting.is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(500));
        let windows = platform.list_windows();
        waiting.retain(|source| {
            let found = resolve::find_window(platform, source, &windows).is_some();
            if found {
                ui::success(&format!("{} : fenêtre ouverte.", source.name));
            }
            !found
        });
    }
    missing.extend(waiting.iter().map(|s| s.name.clone()));
    missing
}

/// Affichage en direct jusqu'à la demande d'arrêt (Q, Échap, Ctrl+C ou
/// disque presque plein). R rouvre les sources perdues.
fn monitor(mut session: Session, header: &str, sources: &[Source]) -> Session {
    let term = Term::stdout();
    let _ = term.clear_screen();
    let mut drawn = 0usize;
    let mut last_draw: Option<Instant> = None;
    let mut free = session.free_space();
    let mut last_disk_check = Instant::now();
    let mut disk_full = false;
    let mut states: Vec<SourceState> = Vec::new();
    let mut notice: Option<String> = None;

    loop {
        match keys::poll() {
            Some(keys::Key::Stop) => break,
            Some(keys::Key::Relaunch) => {
                notice = Some(relaunch_lost(sources, &states));
                last_draw = None;
            }
            None => {}
        }
        if STOP_REQUESTED.load(Ordering::SeqCst) {
            break;
        }
        if last_disk_check.elapsed() >= Duration::from_secs(10) {
            last_disk_check = Instant::now();
            free = session.free_space();
            if free.is_some_and(|f| f < DISK_STOP_THRESHOLD) {
                disk_full = true;
                break;
            }
        }
        if last_draw.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
            last_draw = Some(Instant::now());
            let statuses = session.status();
            states = statuses.iter().map(|s| s.state).collect();
            let lines = render(&session, &statuses, header, free, notice.as_deref());
            let _ = term.clear_last_lines(drawn);
            for line in &lines {
                let _ = term.write_line(line);
            }
            drawn = lines.len();
        }
        thread::sleep(Duration::from_millis(100));
    }
    if disk_full {
        ui::failure("Disque presque plein : arrêt automatique pour protéger les fichiers.");
    }
    session
}

/// Relance le programme des sources perdues ou introuvables qui peuvent
/// l'être (navigateurs, applications avec relance activée).
fn relaunch_lost(sources: &[Source], states: &[SourceState]) -> String {
    let mut launched = Vec::new();
    for (source, state) in sources.iter().zip(states) {
        if matches!(state, SourceState::Lost | SourceState::Searching) && source.should_launch() {
            match resolve::launch(source) {
                Ok(()) => launched.push(source.name.clone()),
                Err(e) => log::warn!("relance de {} impossible : {e:#}", source.name),
            }
        }
    }
    if launched.is_empty() {
        "Aucune source perdue à rouvrir.".into()
    } else {
        format!("Réouverture : {}", launched.join(", "))
    }
}

fn render(
    session: &Session,
    statuses: &[SourceStatus],
    header: &str,
    free: Option<u64>,
    notice: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{}  {}   {}",
            style("● ENREGISTREMENT").red().bold(),
            style(ui::clock(session.elapsed())).bold(),
            style(header).dim()
        ),
        match free {
            Some(free) => {
                let autonomy = free as f64 / session.bytes_per_hour().max(1) as f64 * 3600.0;
                let text = format!("Disque : {} libres · environ {} d'autonomie", ui::bytes(free), ui::hours(autonomy));
                if free < DISK_WARNING_THRESHOLD {
                    style(text).yellow().to_string()
                } else {
                    style(text).dim().to_string()
                }
            }
            None => style("Disque : espace libre inconnu").dim().to_string(),
        },
        String::new(),
        style(format!("  {:<22} {:<10} {:>9}   {:<16} {:>9}", "Source", "État", "Perdues", "Son", "Taille"))
            .underlined()
            .to_string(),
    ];
    for status in statuses {
        lines.push(source_line(status));
        if status.backlog_seconds >= BACKLOG_WARNING_SECONDS {
            lines.push(format!(
                "    {}",
                style(format!(
                    "L'encodeur prend du retard ({:.0} s d'images en attente) : baisse la résolution, les i/s ou le nombre de sources.",
                    status.backlog_seconds
                ))
                .yellow()
            ));
        }
        if let Some(error) = &status.error {
            lines.push(format!("    {}", style(ui::truncate(error, 110)).yellow()));
        }
    }
    lines.push(String::new());
    if let Some(notice) = notice {
        lines.push(style(notice).cyan().to_string());
    }
    lines.push(
        style("Q ou Échap : arrêter · R : rouvrir les sources perdues. Les fichiers restent lisibles même en cas de coupure.")
            .dim()
            .to_string(),
    );
    lines
}

fn source_line(status: &SourceStatus) -> String {
    let state = match status.state {
        SourceState::Connected => style(status.state.label()).green(),
        SourceState::Failed => style(status.state.label()).red().bold(),
        SourceState::Lost | SourceState::Minimized | SourceState::Searching => style(status.state.label()).yellow(),
        SourceState::Finished => style(status.state.label()).dim(),
    };
    let sound = if status.audio_recent { "actif" } else { "muet" };
    format!(
        "  {:<22} {:<10} {:>9}   {} {:<7} {:>9}",
        ui::truncate(&status.name, 22),
        state,
        ui::thousands(status.frames_dropped),
        ui::meter(status.audio_level),
        sound,
        ui::bytes(status.file_size)
    )
}
