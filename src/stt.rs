pub mod engine;
pub mod whisperx;

use anyhow::{bail, Result};

use crate::config::AppConfig;
use engine::{Transcriber, WhisperCliTranscriber};
use whisperx::WhisperXTranscriber;

#[derive(Clone)]
pub enum BackendTranscriber {
    WhisperCpp(WhisperCliTranscriber),
    WhisperX(WhisperXTranscriber),
}

impl Transcriber for BackendTranscriber {
    fn transcribe(
        &self,
        pcm_mono_16khz: &[f32],
        options: &engine::DecodeOptions,
    ) -> Result<engine::Transcript> {
        match self {
            Self::WhisperCpp(transcriber) => transcriber.transcribe(pcm_mono_16khz, options),
            Self::WhisperX(transcriber) => transcriber.transcribe(pcm_mono_16khz, options),
        }
    }
}

pub fn create_transcriber(config: &AppConfig) -> Result<BackendTranscriber> {
    if config.uses_whisperx() {
        return Ok(BackendTranscriber::WhisperX(WhisperXTranscriber::new(config)?));
    }

    if config.uses_whispercpp() {
        return Ok(BackendTranscriber::WhisperCpp(WhisperCliTranscriber::new(
            config,
        )?));
    }

    bail!("unknown stt_backend: {}", config.stt_backend);
}
