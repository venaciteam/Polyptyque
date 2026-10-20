//! Capture du son d'un programme (et de ses sous-programmes) par WASAPI
//! « process loopback » : disponible depuis Windows 10 version 2004.

use anyhow::{Context, Result};
use polyptyque_core::platform::{AudioCapture, AudioPacket};
use wasapi::{AudioCaptureClient, AudioClient, Direction, Handle, SampleType, StreamMode, WaveFormat};

pub struct LoopbackCapture {
    // Gardé vivant tant que la capture tourne.
    client: AudioClient,
    capture: AudioCaptureClient,
    event: Handle,
    frame_bytes: usize,
    buffer: Vec<u8>,
}

impl LoopbackCapture {
    pub fn open(pid: u32, sample_rate: u32, channels: u16) -> Result<Self> {
        let _ = wasapi::initialize_mta();
        // Flottant 32 bits : Windows convertit si besoin depuis le format du
        // programme capturé.
        let format = WaveFormat::new(32, 32, &SampleType::Float, sample_rate as usize, channels as usize, None);
        let mut client = AudioClient::new_application_loopback_client(pid, true)
            .with_context(|| format!("activation de la capture audio du processus {pid}"))?;
        let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 };
        client.initialize_client(&format, &Direction::Capture, &mode).context("initialisation de la capture audio")?;
        let event = client.set_get_eventhandle().context("événement de capture audio")?;
        let capture = client.get_audiocaptureclient().context("client de capture audio")?;
        client.start_stream().context("démarrage de la capture audio")?;
        Ok(Self { client, capture, event, frame_bytes: channels as usize * 4, buffer: Vec::new() })
    }
}

impl AudioCapture for LoopbackCapture {
    fn read(&mut self, timeout_ms: u32, out: &mut Vec<AudioPacket>) -> Result<()> {
        // Un délai dépassé n'est pas une erreur : le programme est muet.
        let _ = self.event.wait_for_event(timeout_ms);
        loop {
            let frames = self.capture.get_next_packet_size()?.unwrap_or(0) as usize;
            if frames == 0 {
                return Ok(());
            }
            let bytes = frames * self.frame_bytes;
            if self.buffer.len() < bytes {
                self.buffer.resize(bytes, 0);
            }
            let (read, info) = self.capture.read_from_device(&mut self.buffer[..bytes])?;
            let read_bytes = read as usize * self.frame_bytes;
            let timestamp = (!info.flags.timestamp_error && info.timestamp != 0).then_some(info.timestamp);
            out.push(AudioPacket {
                data: self.buffer[..read_bytes].to_vec(),
                timestamp_hns: timestamp,
                silent: info.flags.silent,
            });
        }
    }
}

impl Drop for LoopbackCapture {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
    }
}
