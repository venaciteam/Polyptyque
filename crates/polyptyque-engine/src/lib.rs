//! Moteur d'enregistrement de Polyptyque.
//!
//! Indépendant de l'interface : la console du prototype, une future
//! interface graphique ou un test le pilotent de la même façon
//! ([`Session::start`], [`Session::status`], [`Session::stop`]).
//!
//! Pour chaque source :
//! - un fil vidéo capture la fenêtre et émet les images à cadence constante ;
//! - un fil audio capture le son du programme et le cale sur l'horloge ;
//! - deux fils d'écriture envoient ces flux à un processus FFmpeg dédié.

mod audio_task;
pub mod probe;
pub mod remux;
pub mod resolve;
mod session;
mod shared;
mod video_task;
mod writer;

pub use session::{Session, SessionPlan, SourceStatus, SourceSummary};
pub use shared::SourceState;
