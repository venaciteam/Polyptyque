//! Lecture du clavier sans blocage pendant un enregistrement.

extern "C" {
    fn _kbhit() -> i32;
    fn _getwch() -> u16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Q ou Échap : arrêter l'enregistrement.
    Stop,
    /// R : rouvrir les sources perdues.
    Relaunch,
}

/// Touche d'action pressée depuis le dernier appel (l'arrêt est prioritaire).
pub fn poll() -> Option<Key> {
    let mut key = None;
    unsafe {
        while _kbhit() != 0 {
            match _getwch() {
                0x71 | 0x51 | 0x1b => key = Some(Key::Stop),
                0x72 | 0x52 if key.is_none() => key = Some(Key::Relaunch),
                // Touches spéciales (flèches…) : un second code suit.
                0x00 | 0xE0 => {
                    let _ = _getwch();
                }
                _ => {}
            }
        }
    }
    key
}
