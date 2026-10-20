//! Calculs d'image indépendants du système.

/// Image NV12 noire (luminance 16, chrominance neutre 128).
pub fn black_nv12(width: u32, height: u32) -> Vec<u8> {
    let luma = width as usize * height as usize;
    let mut frame = vec![16u8; luma + luma / 2];
    frame[luma..].fill(128);
    frame
}

/// Rectangle `(x, y, largeur, hauteur)` où placer une image `src` dans une
/// sortie `dst` en conservant ses proportions (bandes noires si besoin).
/// Toutes les valeurs sont paires, comme l'exige le NV12.
pub fn fit_rect(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> (u32, u32, u32, u32) {
    if src_w == 0 || src_h == 0 {
        return (0, 0, dst_w, dst_h);
    }
    let scale = f64::min(dst_w as f64 / src_w as f64, dst_h as f64 / src_h as f64);
    let w = (((src_w as f64 * scale).round() as u32).min(dst_w) & !1).max(2);
    let h = (((src_h as f64 * scale).round() as u32).min(dst_h) & !1).max(2);
    let x = ((dst_w - w) / 2) & !1;
    let y = ((dst_h - h) / 2) & !1;
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_frame_layout() {
        let frame = black_nv12(4, 2);
        assert_eq!(frame, vec![16, 16, 16, 16, 16, 16, 16, 16, 128, 128, 128, 128]);
    }

    #[test]
    fn same_ratio_fills_output() {
        assert_eq!(fit_rect(3840, 2160, 1920, 1080), (0, 0, 1920, 1080));
    }

    #[test]
    fn narrow_window_gets_side_bars() {
        let (x, y, w, h) = fit_rect(1000, 1000, 1920, 1080);
        assert_eq!((y, h), (0, 1080));
        assert_eq!(w, 1080);
        assert_eq!(x, 420);
    }

    #[test]
    fn wide_window_gets_top_bars() {
        let (x, y, w, h) = fit_rect(2560, 720, 1920, 1080);
        assert_eq!((x, w), (0, 1920));
        assert_eq!(h, 540);
        assert_eq!(y, 270);
    }
}
