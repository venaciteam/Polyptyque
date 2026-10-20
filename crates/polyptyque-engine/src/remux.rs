//! Conversion sans perte des enregistrements Matroska en MP4 (ou en M4A
//! pour un fichier de son seul), pour les logiciels de montage qui gèrent
//! mal le MKV.
//!
//! Les données sont recopiées telles quelles (« remuxage ») : aucune perte
//! de qualité, et la durée ne dépend que de la vitesse du disque. Le
//! fichier d'origine est toujours conservé, et le résultat est vérifié
//! (pistes, durée) avant d'être déclaré réussi.

use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use anyhow::{bail, Context, Result};
use polyptyque_core::naming;
use polyptyque_core::platform::Platform;

/// Ce que FFmpeg annonce d'un fichier.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaInfo {
    /// Durée en secondes, si le fichier l'indique (un MKV interrompu
    /// brutalement peut ne pas l'avoir).
    pub duration: Option<f64>,
    pub video_streams: usize,
    pub audio_streams: usize,
    pub video_codec: Option<String>,
}

/// Fichiers que l'on sait convertir.
pub fn is_convertible(path: &Path) -> bool {
    matches!(extension(path).as_deref(), Some("mkv" | "mka"))
}

/// Destination d'une conversion : même dossier, même nom, extension `.mp4`
/// (ou `.m4a` pour du son seul). Un fichier existant n'est jamais écrasé.
pub fn converted_path(input: &Path) -> PathBuf {
    let ext = if extension(input).as_deref() == Some("mka") { "m4a" } else { "mp4" };
    let dir = input.parent().unwrap_or(Path::new("."));
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "conversion".into());
    naming::unique_path(dir, &stem, ext, &HashSet::new())
}

/// Lit les pistes et la durée d'un fichier.
pub fn inspect(platform: &dyn Platform, ffmpeg: &Path, path: &Path) -> Result<MediaInfo> {
    let mut command = Command::new(ffmpeg);
    command.args(["-hide_banner", "-nostdin", "-i"]).arg(path);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    platform.prepare_encoder_process(&mut command);
    // Sans fichier de sortie, FFmpeg décrit l'entrée puis s'arrête en erreur :
    // c'est attendu.
    let output = command.output().with_context(|| format!("lancement de {}", ffmpeg.display()))?;
    let text = String::from_utf8_lossy(&output.stderr);
    let info = parse_media_info(&text);
    if info.video_streams == 0 && info.audio_streams == 0 {
        bail!("fichier illisible : {}", last_line(&text));
    }
    Ok(info)
}

/// Convertit `input` et renvoie le chemin du fichier créé. `on_progress`
/// reçoit l'avancement (0 à 1) quand la durée est connue. Si `cancel`
/// passe à vrai, la conversion s'arrête et le fichier partiel est effacé.
pub fn convert(
    platform: &dyn Platform,
    ffmpeg: &Path,
    input: &Path,
    on_progress: &mut dyn FnMut(f32),
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    if !is_convertible(input) {
        bail!("seuls les fichiers .mkv et .mka peuvent être convertis");
    }
    let source = inspect(platform, ffmpeg, input)?;
    let output = converted_path(input);
    let args = remux_args(input, &output, source.video_codec.as_deref() == Some("hevc"));
    log::info!("conversion : {} -> {}", input.display(), output.display());

    let mut command = Command::new(ffmpeg);
    command.args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    platform.prepare_encoder_process(&mut command);
    let mut child = command.spawn().with_context(|| format!("lancement de {}", ffmpeg.display()))?;

    // Les erreurs sont lues à part pour que FFmpeg ne bloque jamais dessus.
    let stderr = child.stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = pipe.read_to_string(&mut text);
            text
        })
    });
    let mut cancelled = false;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                let _ = child.kill();
                break;
            }
            if let (Some(seconds), Some(total)) = (parse_progress(&line), source.duration) {
                if total > 0.0 {
                    on_progress((seconds / total).clamp(0.0, 1.0) as f32);
                }
            }
        }
    }
    let status = child.wait().context("attente de FFmpeg")?;
    let errors = stderr.and_then(|h| h.join().ok()).unwrap_or_default();

    let result = if cancelled || cancel.load(Ordering::Relaxed) {
        Err(anyhow::anyhow!("conversion annulée"))
    } else if !status.success() {
        Err(anyhow::anyhow!("échec de la conversion ({status}) : {}", last_line(&errors)))
    } else {
        inspect(platform, ffmpeg, &output).and_then(|converted| verify(&source, &converted))
    };
    match result {
        Ok(()) => {
            on_progress(1.0);
            log::info!("conversion réussie : {}", output.display());
            Ok(output)
        }
        Err(e) => {
            let _ = fs::remove_file(&output);
            log::warn!("conversion de {} : {e:#}", input.display());
            Err(e)
        }
    }
}

fn remux_args(input: &Path, output: &Path, hevc: bool) -> Vec<String> {
    let mut args: Vec<String> =
        ["-hide_banner", "-nostdin", "-loglevel", "error", "-nostats", "-progress", "pipe:1", "-i"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    args.push(input.to_string_lossy().into_owned());
    args.extend(["-map", "0", "-c", "copy"].iter().map(|s| s.to_string()));
    if hevc {
        // Étiquette attendue par la plupart des logiciels pour le HEVC en MP4.
        args.extend(["-tag:v".to_string(), "hvc1".to_string()]);
    }
    args.extend(["-f".to_string(), "mp4".to_string()]);
    args.push(output.to_string_lossy().into_owned());
    args
}

/// Le fichier converti doit avoir les mêmes pistes et la même durée.
fn verify(source: &MediaInfo, converted: &MediaInfo) -> Result<()> {
    if (converted.video_streams, converted.audio_streams) != (source.video_streams, source.audio_streams) {
        bail!(
            "pistes manquantes après conversion ({} vidéo et {} son au lieu de {} et {})",
            converted.video_streams,
            converted.audio_streams,
            source.video_streams,
            source.audio_streams
        );
    }
    match (source.duration, converted.duration) {
        (Some(expected), Some(actual)) if (expected - actual).abs() > 1.0 + expected * 0.001 => {
            bail!("durée différente après conversion ({actual:.1} s au lieu de {expected:.1} s)")
        }
        (_, None) => bail!("durée illisible après conversion"),
        _ => Ok(()),
    }
}

fn parse_media_info(text: &str) -> MediaInfo {
    let mut info = MediaInfo::default();
    // Seule la description de l'entrée compte.
    for line in text.lines().skip_while(|l| !l.starts_with("Input #0")) {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Duration:") {
            info.duration = rest.split(',').next().and_then(|d| parse_clock(d.trim()));
        } else if line.starts_with("Stream #") {
            if let Some(index) = line.find(": Video: ") {
                info.video_streams += 1;
                if info.video_codec.is_none() {
                    let codec = line[index + 9..].split([' ', ',']).next().unwrap_or("");
                    info.video_codec = Some(codec.to_string());
                }
            } else if line.contains(": Audio: ") {
                info.audio_streams += 1;
            }
        } else if line.starts_with("Output #") {
            break;
        }
    }
    info
}

/// `01:02:03.45` en secondes.
fn parse_clock(text: &str) -> Option<f64> {
    let mut parts = text.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Position atteinte, en secondes, d'après une ligne de `-progress`.
fn parse_progress(line: &str) -> Option<f64> {
    // `out_time_ms` est en réalité en microsecondes, comme `out_time_us`.
    let value = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms="))?;
    value.trim().parse::<i64>().ok().filter(|&us| us >= 0).map(|us| us as f64 / 1_000_000.0)
}

fn extension(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_lowercase())
}

fn last_line(text: &str) -> String {
    let line = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("aucun détail").trim();
    line.chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESCRIPTION: &str = "\
Input #0, matroska,webm, from 'a.mkv':
  Metadata:
    title           : Firefox 1
  Duration: 01:02:03.50, start: 0.000000, bitrate: 160 kb/s
  Stream #0:0: Video: hevc (Main), yuv420p(tv, bt709), 1920x1080, 60 fps (default)
  Stream #0:1(fre): Audio: aac (LC), 48000 Hz, stereo, fltp (default)
At least one output file must be specified";

    #[test]
    fn reads_media_description() {
        let info = parse_media_info(DESCRIPTION);
        assert_eq!(info.duration, Some(3723.5));
        assert_eq!((info.video_streams, info.audio_streams), (1, 1));
        assert_eq!(info.video_codec.as_deref(), Some("hevc"));
    }

    #[test]
    fn missing_duration_is_tolerated() {
        let info = parse_media_info("Input #0, matroska,webm, from 'a.mka':\n  Duration: N/A, start: 0.0\n  Stream #0:0: Audio: aac, 48000 Hz\n");
        assert_eq!(info.duration, None);
        assert_eq!((info.video_streams, info.audio_streams), (0, 1));
    }

    #[test]
    fn reads_progress() {
        assert_eq!(parse_progress("out_time_us=2500000"), Some(2.5));
        assert_eq!(parse_progress("out_time_ms=1000000"), Some(1.0));
        assert_eq!(parse_progress("out_time_us=N/A"), None);
        assert_eq!(parse_progress("progress=continue"), None);
    }

    #[test]
    fn verification_rules() {
        let source = MediaInfo { duration: Some(3600.0), video_streams: 1, audio_streams: 1, video_codec: None };
        let same = MediaInfo { duration: Some(3600.5), ..source.clone() };
        assert!(verify(&source, &same).is_ok());
        let short = MediaInfo { duration: Some(3500.0), ..source.clone() };
        assert!(verify(&source, &short).is_err());
        let no_audio = MediaInfo { audio_streams: 0, ..source.clone() };
        assert!(verify(&source, &no_audio).is_err());
        // Un MKV interrompu sans durée reste convertible.
        let unknown = MediaInfo { duration: None, ..source.clone() };
        assert!(verify(&unknown, &same).is_ok());
    }

    #[test]
    fn destination_names() {
        assert!(is_convertible(Path::new("a/b.MKV")));
        assert!(!is_convertible(Path::new("a/b.mp4")));
        let dir = std::env::temp_dir();
        let stem = format!("polyptyque-conv-{}", polyptyque_core::config::new_id());
        assert_eq!(converted_path(&dir.join(format!("{stem}.mkv"))), dir.join(format!("{stem}.mp4")));
        assert_eq!(converted_path(&dir.join(format!("{stem}.mka"))), dir.join(format!("{stem}.m4a")));
    }

    #[test]
    fn hevc_gets_compatible_tag() {
        let args = remux_args(Path::new("a.mkv"), Path::new("a.mp4"), true).join(" ");
        assert!(args.contains("-map 0 -c copy -tag:v hvc1 -f mp4 a.mp4"));
        let args = remux_args(Path::new("a.mkv"), Path::new("a.mp4"), false).join(" ");
        assert!(!args.contains("hvc1"));
    }
}
