//! Fils d'écriture : vident une file vers un tube lu par FFmpeg.

use std::io;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};
use polyptyque_core::platform::StreamPipe;

use crate::shared::Shared;

const POLL: Duration = Duration::from_millis(500);

/// Données envoyables dans un tube : une image partagée ou un bloc de son.
pub(crate) trait Chunk: Send + 'static {
    fn bytes(&self) -> &[u8];
}

impl Chunk for Vec<u8> {
    fn bytes(&self) -> &[u8] {
        self
    }
}

impl Chunk for Arc<Vec<u8>> {
    fn bytes(&self) -> &[u8] {
        self.as_slice()
    }
}

pub(crate) fn spawn<T: Chunk>(
    thread_name: String,
    pipe: Box<dyn StreamPipe>,
    queue: Receiver<T>,
    shared: Arc<Shared>,
    label: &'static str,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new().name(thread_name).spawn(move || {
        let mut output = match pipe.connect() {
            Ok(output) => output,
            Err(e) => {
                shared.fail(format!("{label} : {e:#}"));
                return;
            }
        };
        loop {
            match queue.recv_timeout(POLL) {
                Ok(item) => {
                    let bytes = item.bytes();
                    if let Err(e) = output.write_all(bytes) {
                        if shared.stop_at().is_none() {
                            shared.fail(format!("{label} : l'encodeur ne reçoit plus les données ({e})"));
                        }
                        return;
                    }
                    shared.bytes_sent.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                }
                // File vide : on continue, sauf si la capture a été abandonnée
                // à l'arrêt (fil bloqué), auquel cas on ferme le flux.
                Err(RecvTimeoutError::Timeout) if shared.capture_abandoned() => break,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = output.flush();
        // La fermeture du tube signale la fin du flux à FFmpeg.
    })
}
