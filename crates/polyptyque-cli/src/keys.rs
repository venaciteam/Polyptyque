//! Lecture du clavier sans blocage, pour arrêter un enregistrement.

extern "C" {
    fn _kbhit() -> i32;
    fn _getwch() -> u16;
}

/// Vrai si l'utilisateur a appuyé sur Q ou Échap depuis le dernier appel.
pub fn stop_requested() -> bool {
    let mut stop = false;
    unsafe {
        while _kbhit() != 0 {
            match _getwch() {
                // q, Q, Échap
                0x71 | 0x51 | 0x1b => stop = true,
                // Touches spéciales (flèches…) : un second code suit.
                0x00 | 0xE0 => {
                    let _ = _getwch();
                }
                _ => {}
            }
        }
    }
    stop
}
