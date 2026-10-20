//! Implémentation Windows de [`Platform`] :
//! - image : Windows.Graphics.Capture, conversion NV12 par la carte graphique ;
//! - son : capture WASAPI « process loopback » (un programme et ses
//!   sous-programmes), et micro ;
//! - fenêtres, processus, navigateurs installés, tubes nommés.

#![cfg(windows)]

mod audio;
mod browsers;
pub mod hotkeys;
mod pipe;
mod process;
mod system;
mod video;
mod windows_list;

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use polyptyque_core::platform::{
    AudioCapture, AudioDevice, BrowserInstall, Platform, StreamPipe, VideoCapture, WindowHandle, WindowInfo,
};

/// Pas de fenêtre de console pour FFmpeg.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// FFmpeg ne reçoit pas le Ctrl+C de la console : c'est Polyptyque qui
/// l'arrête proprement en fermant ses entrées.
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;

pub struct WindowsPlatform {
    own_pid: u32,
}

impl WindowsPlatform {
    pub fn new() -> Self {
        Self { own_pid: std::process::id() }
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for WindowsPlatform {
    fn list_windows(&self) -> Vec<WindowInfo> {
        windows_list::list(self.own_pid)
    }

    fn is_window_alive(&self, handle: WindowHandle) -> bool {
        windows_list::is_alive(handle)
    }

    fn is_window_minimized(&self, handle: WindowHandle) -> bool {
        windows_list::is_minimized(handle)
    }

    fn process_command_line(&self, pid: u32) -> Option<String> {
        process::command_line(pid)
    }

    fn detect_browsers(&self) -> Vec<BrowserInstall> {
        browsers::detect()
    }

    fn now_hns(&self) -> u64 {
        system::now_hns()
    }

    fn open_video_capture(
        &self,
        handle: WindowHandle,
        width: u32,
        height: u32,
        capture_cursor: bool,
    ) -> anyhow::Result<Box<dyn VideoCapture>> {
        Ok(Box::new(video::WgcCapture::open(handle, width, height, capture_cursor)?))
    }

    fn open_audio_capture(&self, pid: u32, sample_rate: u32, channels: u16) -> anyhow::Result<Box<dyn AudioCapture>> {
        Ok(Box::new(audio::WasapiCapture::process(pid, sample_rate, channels)?))
    }

    fn list_microphones(&self) -> Vec<AudioDevice> {
        audio::microphones()
    }

    fn open_microphone_capture(
        &self,
        device_id: Option<&str>,
        sample_rate: u32,
        channels: u16,
    ) -> anyhow::Result<Box<dyn AudioCapture>> {
        Ok(Box::new(audio::WasapiCapture::microphone(device_id, sample_rate, channels)?))
    }

    fn create_stream_pipe(&self, name: &str) -> anyhow::Result<Box<dyn StreamPipe>> {
        Ok(Box::new(pipe::NamedPipe::create(name)?))
    }

    fn prepare_encoder_process(&self, command: &mut Command) {
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }

    fn begin_recording_session(&self) {
        system::begin_session();
    }

    fn end_recording_session(&self) {
        system::end_session();
    }

    fn free_disk_space(&self, path: &Path) -> Option<u64> {
        system::free_space(path)
    }
}
