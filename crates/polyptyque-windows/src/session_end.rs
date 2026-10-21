//! Fermeture de la session Windows (arrêt, redémarrage, déconnexion) et mise
//! en veille pendant un enregistrement.
//!
//! Pendant un enregistrement, Polyptyque déclare une raison de blocage :
//! Windows l'affiche dans son écran d'arrêt et laisse choisir d'annuler.
//! Si l'arrêt continue malgré tout, les fichiers sont finalisés avant que
//! Windows ne ferme Polyptyque.
//!
//! Une mise en veille demandée (capot, bouton, menu) ne peut pas être
//! empêchée : Windows prévient quelques secondes avant, le temps d'arrêter
//! proprement l'enregistrement, puis de nouveau au réveil.

use std::cell::RefCell;

use anyhow::{Context, Result};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Power::RegisterSuspendResumeNotification;
use windows::Win32::System::Recovery::{
    RegisterApplicationRestart, UnregisterApplicationRestart, REGISTER_APPLICATION_RESTART_FLAGS,
};
use windows::Win32::System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy};
use windows::Win32::System::Threading::SetProcessShutdownParameters;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DEVICE_NOTIFY_WINDOW_HANDLE, PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, WM_ENDSESSION, WM_NCDESTROY,
    WM_POWERBROADCAST, WM_QUERYENDSESSION,
};

use crate::windows_list::hwnd;

/// Identifiant de notre traitement des messages de la fenêtre.
const SUBCLASS_ID: usize = 0x5059_5054; // « PYPT »
/// Arrêt critique (batterie épuisée…) : il ne peut pas être bloqué.
const ENDSESSION_CRITICAL: u32 = 0x4000_0000;
/// Plus haut niveau réservé aux applications : prévenu avant les processus
/// de niveau ordinaire (0x280), dont les FFmpeg lancés par Polyptyque.
const SHUTDOWN_LEVEL_FIRST: u32 = 0x3FF;

pub enum SessionEnd {
    /// Windows demande s'il peut fermer la session. Le gestionnaire répond
    /// vrai pour bloquer.
    Query,
    /// La session se ferme : tout doit être finalisé avant de répondre.
    Ending,
    /// L'ordinateur va se mettre en veille, dans environ deux secondes.
    Suspend,
    /// L'ordinateur sort de veille.
    Resume,
}

type Handler = Box<dyn Fn(SessionEnd) -> bool>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
}

/// Surveille la fermeture de session sur la fenêtre principale `window`.
/// À appeler sur le fil qui a créé la fenêtre (celui de l'interface).
pub fn watch(window: isize, handler: impl Fn(SessionEnd) -> bool + 'static) -> Result<()> {
    HANDLER.with(|h| *h.borrow_mut() = Some(Box::new(handler)));
    // Polyptyque doit être prévenu avant ses encodeurs : s'ils étaient
    // fermés d'abord, leurs fichiers seraient coupés avant la finalisation.
    if let Err(e) = unsafe { SetProcessShutdownParameters(SHUTDOWN_LEVEL_FIRST, 0) } {
        log::warn!("ordre de fermeture non modifiable : {e}");
    }
    // Sur les PC en veille moderne, seules les fenêtres inscrites sont
    // prévenues de la mise en veille.
    if let Err(e) = unsafe { RegisterSuspendResumeNotification(HANDLE(hwnd(window).0), DEVICE_NOTIFY_WINDOW_HANDLE) } {
        log::warn!("avertissement de mise en veille indisponible : {e}");
    }
    unsafe { SetWindowSubclass(hwnd(window), Some(window_proc), SUBCLASS_ID, 0) }
        .ok()
        .context("surveillance de la fermeture de session")
}

/// Pendant un enregistrement : demande à Windows de relancer Polyptyque
/// s'il est fermé (redémarrage pour une mise à jour, plantage). Le bilan de
/// l'enregistrement interrompu est alors proposé. Windows ne le fait que si
/// la relance des applications à la connexion est activée dans ses
/// paramètres (désactivée par défaut), et si Polyptyque tournait depuis au
/// moins une minute.
pub fn restart_after_interruption(on: bool) {
    let result = unsafe {
        if on {
            RegisterApplicationRestart(w!("--apres-interruption"), REGISTER_APPLICATION_RESTART_FLAGS(0))
        } else {
            UnregisterApplicationRestart()
        }
    };
    if let Err(e) = result {
        log::warn!("relance par Windows : {e}");
    }
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
        WM_POWERBROADCAST => match wparam.0 as u32 {
            PBT_APMSUSPEND => {
                notify(SessionEnd::Suspend);
            }
            PBT_APMRESUMEAUTOMATIC => {
                notify(SessionEnd::Resume);
            }
            _ => {}
        },
        WM_NCDESTROY => {
            let _ = unsafe { RemoveWindowSubclass(window, Some(window_proc), SUBCLASS_ID) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
