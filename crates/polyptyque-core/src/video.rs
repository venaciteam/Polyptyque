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

/// Convertit une image NV12 BT.709 « TV » (16-235) en RVBA, pour les
/// aperçus. `out` doit contenir `width * height * 4` octets.
pub fn nv12_to_rgba(src: &[u8], width: usize, height: usize, out: &mut [u8]) {
    let luma = width * height;
    debug_assert!(src.len() >= luma + luma / 2 && out.len() >= luma * 4);
    let (y_plane, uv_plane) = src.split_at(luma);
    for row in 0..height {
        let uv_row = &uv_plane[(row / 2) * width..];
        for col in 0..width {
            let y = (y_plane[row * width + col] as f32 - 16.0) * 1.164_383;
            let u = uv_row[col & !1] as f32 - 128.0;
            let v = uv_row[(col & !1) + 1] as f32 - 128.0;
            let r = y + 1.792_741 * v;
            let g = y - 0.213_249 * u - 0.532_909 * v;
            let b = y + 2.112_402 * u;
            let pixel = &mut out[(row * width + col) * 4..][..4];
            pixel[0] = r.round().clamp(0.0, 255.0) as u8;
            pixel[1] = g.round().clamp(0.0, 255.0) as u8;
            pixel[2] = b.round().clamp(0.0, 255.0) as u8;
            pixel[3] = 255;
        }
    }
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
    fn nv12_conversion_extremes() {
        let mut out = vec![0u8; 4 * 2 * 4];
        nv12_to_rgba(&black_nv12(4, 2), 4, 2, &mut out);
        assert!(out.chunks(4).all(|p| p == [0, 0, 0, 255]));
        let mut white = vec![235u8; 8];
        white.extend([128u8; 4]);
        nv12_to_rgba(&white, 4, 2, &mut out);
        assert!(out.chunks(4).all(|p| p == [255, 255, 255, 255]));
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
