//! Niveau sonore affiché : crête des échantillons, seuil du silence, et
//! délai pendant lequel une source reste « Son actif ». Communs au moteur
//! (pendant l'enregistrement) et aux vumètres de l'interface (hors
//! enregistrement), pour qu'un même son s'affiche de la même façon.

use crate::timeline::HNS_PER_SEC;

/// En dessous, le son est considéré comme muet pour l'affichage.
pub const SOUND_THRESHOLD: f32 = 0.0005;

/// Après le dernier son au-dessus du seuil, la source reste « Son actif »
/// pendant ce délai (en centaines de nanosecondes).
pub const RECENT_SOUND_HNS: u64 = 2 * HNS_PER_SEC;

/// Crête (0 à 1) d'échantillons flottants 32 bits entrelacés.
pub fn peak_of(data: &[u8]) -> f32 {
    data.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]).abs()).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_is_the_loudest_sample_in_absolute_value() {
        let data: Vec<u8> = [0.25f32, -0.75, 0.5].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(peak_of(&data), 0.75);
        assert_eq!(peak_of(&[]), 0.0);
    }
}
