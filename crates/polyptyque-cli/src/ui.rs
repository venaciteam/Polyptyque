//! Petits utilitaires d'affichage en français.

use std::io::{self, BufRead, Write};
use std::time::Duration;

use console::style;
use dialoguer::theme::ColorfulTheme;

pub fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub fn pause() {
    print!("\n{}", style("Appuie sur Entrée pour continuer…").dim());
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().lock().read_line(&mut line);
}

pub fn success(message: &str) {
    println!("{} {message}", style("✓").green().bold());
}

pub fn warning(message: &str) {
    println!("{} {message}", style("!").yellow().bold());
}

pub fn failure(message: &str) {
    println!("{} {message}", style("✗").red().bold());
}

/// Taille lisible : « 850 Mo », « 1,2 Go ».
pub fn bytes(value: u64) -> String {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let v = value as f64;
    let text = if v >= GB {
        format!("{:.1} Go", v / GB)
    } else if v >= MB {
        format!("{:.0} Mo", v / MB)
    } else {
        format!("{:.0} Ko", v / KB)
    };
    text.replace('.', ",")
}

/// Durée « 02:14:37 ».
pub fn clock(duration: Duration) -> String {
    let s = duration.as_secs();
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Durée approximative : « 31 h », « 45 min ».
pub fn hours(seconds: f64) -> String {
    if seconds >= 3600.0 {
        format!("{:.0} h", seconds / 3600.0)
    } else {
        format!("{:.0} min", (seconds / 60.0).max(0.0))
    }
}

/// Nombre avec espaces : « 12 000 ».
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Indicateur de niveau sonore sur 8 cases (de -60 dB à 0 dB).
pub fn meter(peak: f32) -> String {
    let db = if peak > 0.0 { 20.0 * peak.log10() } else { -120.0 };
    let filled = (((db + 60.0) / 60.0) * 8.0).clamp(0.0, 8.0).round() as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(8 - filled))
}

/// Coupe un texte trop long en ajoutant « … ».
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
        cut.push('…');
        cut
    }
}
