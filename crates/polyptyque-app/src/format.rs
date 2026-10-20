//! Mise en forme des valeurs affichées, en français.

use std::time::Duration;

/// Taille lisible : « 850 Mo », « 1,2 Go ».
pub fn bytes(value: u64) -> String {
    let v = value as f64;
    let text = if v >= 1e9 {
        format!("{:.1} Go", v / 1e9)
    } else if v >= 1e6 {
        format!("{:.0} Mo", v / 1e6)
    } else {
        format!("{:.0} Ko", v / 1e3)
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

/// Niveau crête (0 à 1) ramené sur une échelle perçue : -60 dB à 0 dB.
pub fn level(peak: f32) -> f32 {
    if peak <= 0.0 {
        return 0.0;
    }
    ((20.0 * peak.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Première lettre en majuscule.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(bytes(1_234_000_000), "1,2 Go");
        assert_eq!(bytes(850_000_000), "850 Mo");
        assert_eq!(clock(Duration::from_secs(8077)), "02:14:37");
        assert_eq!(hours(31.0 * 3600.0), "31 h");
        assert_eq!(level(1.0), 1.0);
        assert_eq!(level(0.0), 0.0);
        assert!((level(0.001) - 0.0).abs() < 1e-6);
        assert_eq!(capitalize("discord"), "Discord");
    }
}
