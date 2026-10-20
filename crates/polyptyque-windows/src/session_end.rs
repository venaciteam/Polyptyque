//! Fermeture de la session Windows (arrêt, redémarrage, déconnexion)
//! pendant un enregistrement.
//!
//! Pendant un enregistrement, Polyptyque déclare une raison de blocage :
//! Windows l'affiche dans son écran d'arrêt et laisse choisir d'annuler.
//! Si l'arrêt continue malgré tout, les fichiers sont finalisés avant que
//! Windows ne ferme Polyptyque.

use std::cell::RefCell;

use anyhow::{Context, Result};
use windows::core::HSTRING;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{WM_ENDSESSION, WM_QUERYENDSESSION};

use crate::windows_list::hwnd;

/// Identifiant de notre traitement des messages de la fenêtre.
const SUBCLASS_ID: usize = 0x5059_5054; // « PYPT »
/// Arrêt critique (batterie épuisée…) : il ne peut pas être bloqué.
const ENDSESSION_CRITICAL: u32 = 0x4000_0000;

pub enum SessionEnd {
    /// Windows demande s'il peut fermer la session. Le gestionnaire répond
    /// vrai pour bloquer.
    Query,
    /// La session se ferme : tout doit être finalisé avant de répondre.
    Ending,
}

type Handler = Box<dyn Fn(SessionEnd) -> bool>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

/// Surveille la fermeture de session sur la fenêtre principale `window`.
/// À appeler sur le fil qui a créé la fenêtre (celui de l'interface).
pub fn watch(window: isize, handler: impl Fn(SessionEnd) -> bool + 'static) -> Result<()> {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    unsafe { SetWindowSubclass(hwnd(window), Some(window_proc), SUBCLASS_ID, 0) }
        .ok()
        .context("surveillance de la fermeture de session")
}

/// Raison affichée par Windows si l'on tente d'éteindre ou de se
/// déconnecter. Même fil que [`watch`].
pub fn block(window: isize, reason: &str) -> Result<()> {
    unsafe { ShutdownBlockReasonCreate(hwnd(window), &HSTRING::from(reason)) }
        .context("déclaration de l'enregistrement en cours à Windows")
}

pub fn unblock(window: isize) {
    let _ = unsafe { ShutdownBlockReasonDestroy(hwnd(window)) };
}

fn notify(event: SessionEnd) -> bool {
    HANDLER
        .try_with(|h| h.try_borrow().ok().and_then(|h| h.as_ref().map(|handler| handler(event))))
        .ok()
        .flatten()
        .unwrap_or(false)
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match message {
        WM_QUERYENDSESSION => {
            let critical = lparam.0 as u32 & ENDSESSION_CRITICAL != 0;
            if notify(SessionEnd::Query) && !critical {
                log::warn!("fermeture de session demandée pendant un enregistrement : blocage");
                return LRESULT(0);
            }
        }
        WM_ENDSESSION if wparam.0 != 0 => {
            log::warn!("fermeture de session : finalisation des fichiers");
            notify(SessionEnd::Ending);
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
