//! Fils d'écriture : vident une file vers un tube lu par FFmpeg.

use std::io;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::Receiver;
use polyptyque_core::platform::StreamPipe;

use crate::shared::Shared;

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
        for item in queue.iter() {
            if let Err(e) = output.write_all(item.bytes()) {
                if shared.stop_at().is_none() {
                    shared.fail(format!("{label} : l'encodeur ne reçoit plus les données ({e})"));
                }
                return;
            }
        }
        let _ = output.flush();
        // La fermeture du tube signale la fin du flux à FFmpeg.
    })
}
