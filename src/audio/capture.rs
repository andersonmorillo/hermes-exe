use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone)]
pub struct CapturedAudio {
    pub pcm_16khz_mono: Vec<f32>,
    pub duration_ms: u64,
}

pub struct AudioCapture {
    device: cpal::Device,
    supported_config: cpal::SupportedStreamConfig,
}

pub struct AudioCaptureSession {
    stream: Stream,
    raw_mono: Arc<Mutex<Vec<f32>>>,
    input_sample_rate: u32,
    started_at: Instant,
}

impl AudioCapture {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("no default input device found"))?;
        let supported_config = device.default_input_config()?;

        Ok(Self {
            device,
            supported_config,
        })
    }

    pub fn start_session(&self) -> Result<AudioCaptureSession> {
        let stream_cfg = StreamConfig {
            channels: self.supported_config.channels(),
            sample_rate: self.supported_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let channels = stream_cfg.channels as usize;
        let sample_rate = stream_cfg.sample_rate.0;

        let raw_mono = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
            sample_rate as usize * 20,
        )));
        let buffer_ref = Arc::clone(&raw_mono);
        let err_fn = |err| eprintln!("audio stream error: {err}");

        let stream = match self.supported_config.sample_format() {
            SampleFormat::F32 => self.device.build_input_stream(
                &stream_cfg,
                move |data: &[f32], _| append_frames_f32(data, channels, &buffer_ref),
                err_fn,
                None,
            )?,
            SampleFormat::I16 => self.device.build_input_stream(
                &stream_cfg,
                move |data: &[i16], _| append_frames_i16(data, channels, &buffer_ref),
                err_fn,
                None,
            )?,
            SampleFormat::U16 => self.device.build_input_stream(
                &stream_cfg,
                move |data: &[u16], _| append_frames_u16(data, channels, &buffer_ref),
                err_fn,
                None,
            )?,
            other => return Err(anyhow!("unsupported input sample format: {other:?}")),
        };

        stream.play()?;
        Ok(AudioCaptureSession {
            stream,
            raw_mono,
            input_sample_rate: sample_rate,
            started_at: Instant::now(),
        })
    }
}

impl AudioCaptureSession {
    pub fn snapshot(&self) -> Result<CapturedAudio> {
        self.capture_current_audio()
    }

    pub fn finish(self) -> Result<CapturedAudio> {
        let AudioCaptureSession {
            stream,
            raw_mono,
            input_sample_rate,
            started_at,
        } = self;

        stream.pause().ok();
        drop(stream);

        capture_current_audio(&raw_mono, input_sample_rate, started_at)
    }

    fn capture_current_audio(&self) -> Result<CapturedAudio> {
        capture_current_audio(&self.raw_mono, self.input_sample_rate, self.started_at)
    }
}

impl CapturedAudio {
    pub fn tail_ms(&self, max_duration_ms: u64) -> Self {
        if max_duration_ms == 0 || self.duration_ms <= max_duration_ms {
            return self.clone();
        }

        let target_samples = ((TARGET_SAMPLE_RATE as u64 * max_duration_ms) / 1000).max(1) as usize;
        let start = self.pcm_16khz_mono.len().saturating_sub(target_samples);
        let pcm_16khz_mono = self.pcm_16khz_mono[start..].to_vec();
        let duration_ms = ((pcm_16khz_mono.len() as u64) * 1000 / TARGET_SAMPLE_RATE as u64).max(1);

        Self {
            pcm_16khz_mono,
            duration_ms,
        }
    }
}

fn append_frames_f32(data: &[f32], channels: usize, buffer: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut output) = buffer.lock() {
        for frame in data.chunks(channels) {
            let mono = frame.iter().copied().sum::<f32>() / frame.len() as f32;
            output.push(mono);
        }
    }
}

fn capture_current_audio(
    raw_mono: &Arc<Mutex<Vec<f32>>>,
    input_sample_rate: u32,
    started_at: Instant,
) -> Result<CapturedAudio> {
    let duration_ms = started_at.elapsed().as_millis() as u64;
    let raw_mono = raw_mono
        .lock()
        .map_err(|_| anyhow!("failed to lock audio buffer"))?
        .clone();

    let pcm_16khz_mono = if input_sample_rate == TARGET_SAMPLE_RATE {
        raw_mono
    } else {
        resample_linear(&raw_mono, input_sample_rate, TARGET_SAMPLE_RATE)
            .context("failed to resample audio to 16kHz")?
    };

    Ok(CapturedAudio {
        pcm_16khz_mono,
        duration_ms,
    })
}

fn append_frames_i16(data: &[i16], channels: usize, buffer: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut output) = buffer.lock() {
        for frame in data.chunks(channels) {
            let mono = frame
                .iter()
                .map(|s| *s as f32 / i16::MAX as f32)
                .sum::<f32>()
                / frame.len() as f32;
            output.push(mono);
        }
    }
}

fn append_frames_u16(data: &[u16], channels: usize, buffer: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut output) = buffer.lock() {
        for frame in data.chunks(channels) {
            let mono = frame
                .iter()
                .map(|s| ((*s as f32 / u16::MAX as f32) * 2.0) - 1.0)
                .sum::<f32>()
                / frame.len() as f32;
            output.push(mono);
        }
    }
}

fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    if from_rate == 0 || to_rate == 0 {
        return Err(anyhow!("sample rates must be non-zero"));
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = (samples.len() as f64 * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    for idx in 0..out_len {
        let source_pos = idx as f64 / ratio;
        let left_idx = source_pos.floor() as usize;
        let right_idx = (left_idx + 1).min(samples.len().saturating_sub(1));
        let frac = (source_pos - left_idx as f64) as f32;
        let left = samples[left_idx];
        let right = samples[right_idx];
        out.push(left + (right - left) * frac);
    }

    Ok(out)
}
