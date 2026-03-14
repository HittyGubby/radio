use crate::config::Config;
use log::{error, warn};
use opus::{Application, Channels, Encoder};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AudioPacket {
    pub timestamp: u64,
    pub payload: Vec<u8>,
}

pub struct AudioEncoder {
    encoder: Option<Encoder>,
    sample_index: u64,
    config: Config,
    resample_buffer: Vec<f32>,
    resample_position: f64,
    resampled_output_buffer: Vec<f32>,
}

impl AudioEncoder {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let encoder = if config.audio_codec == "opus" {
            Some(Self::create_opus_encoder(config)?)
        } else {
            None
        };

        Ok(Self {
            encoder,
            sample_index: 0,
            config: config.clone(),
            resample_buffer: Vec::new(),
            resample_position: 0.0,
            resampled_output_buffer: Vec::new(),
        })
    }

    fn create_opus_encoder(config: &Config) -> anyhow::Result<Encoder> {
        let mut encoder = Encoder::new(config.get_output_sample_rate(), Channels::Mono, Application::Audio)?;

        encoder.set_bitrate(opus::Bitrate::Bits(config.audio_bitrate as i32))?;
        encoder.set_complexity(10)?;

        Ok(encoder)
    }

    /// Resample audio from input sample rate to output sample rate using linear interpolation
    fn resample(&mut self, input_samples: &[f32]) -> Vec<f32> {
        let input_rate = self.config.get_input_sample_rate() as f64;
        let output_rate = self.config.get_output_sample_rate() as f64;
        let ratio = input_rate / output_rate;

        // Add input samples to buffer
        self.resample_buffer.extend_from_slice(input_samples);

        // Calculate how many output samples we can produce
        let output_samples_count = ((self.resample_buffer.len() as f64 - 1.0) / ratio).floor() as usize;

        if output_samples_count == 0 {
            return Vec::new();
        }

        let mut output_samples = Vec::with_capacity(output_samples_count);

        for _ in 0..output_samples_count {
            let position = self.resample_position;
            let index_float = position.floor();
            let index = index_float as usize;
            let fraction = (position - index_float) as f32;

            if index + 1 < self.resample_buffer.len() {
                // Linear interpolation
                let sample = self.resample_buffer[index] * (1.0 - fraction)
                    + self.resample_buffer[index + 1] * fraction;
                output_samples.push(sample);
            } else {
                output_samples.push(0.0);
            }

            self.resample_position += ratio;
        }

        // Remove used samples from buffer
        let used_samples = self.resample_position.ceil() as usize;
        if used_samples > 0 && used_samples <= self.resample_buffer.len() {
            self.resample_buffer.drain(0..used_samples);
            self.resample_position -= used_samples as f64;
        }

        output_samples
    }

    pub fn encode(&mut self, samples: &[f32]) -> Option<AudioPacket> {
        let frame_samples = self.config.audio_frame_samples();

        if samples.is_empty() {
            return None;
        }

        // Resample input samples to output sample rate
        let resampled = self.resample(samples);

        // Add resampled samples to output buffer
        self.resampled_output_buffer.extend_from_slice(&resampled);

        // Check if we have enough samples for a frame
        if self.resampled_output_buffer.len() < frame_samples {
            return None;
        }

        // Take exactly frame_samples from the buffer
        let frame: Vec<f32> = self.resampled_output_buffer.drain(0..frame_samples).collect();

        let payload = if let Some(encoder) = &mut self.encoder {
            let mut output = vec![0u8; 4000];
            let len = encoder
                .encode_float(&frame, &mut output)
                .map_err(|e| error!("Opus encode error: {}", e))
                .ok()?;
            output.truncate(len);
            output
        } else {
            match self.config.audio_bitdepth {
                16 => {
                    let pcm_samples: Vec<u8> = frame
                        .iter()
                        .flat_map(|s| {
                            let sample_i16 = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                            sample_i16.to_le_bytes()
                        })
                        .collect();
                    pcm_samples
                }
                24 => {
                    let pcm_samples: Vec<u8> = frame
                        .iter()
                        .flat_map(|s| {
                            let sample_i24 = (s.clamp(-1.0, 1.0) * 8388607.0) as i32;
                            [
                                (sample_i24 & 0xFF) as u8,
                                ((sample_i24 >> 8) & 0xFF) as u8,
                                ((sample_i24 >> 16) & 0xFF) as u8,
                            ]
                        })
                        .collect();
                    pcm_samples
                }
                _ => {
                    error!("Unsupported bit depth: {}", self.config.audio_bitdepth);
                    return None;
                }
            }
        };

        let timestamp = self.sample_index;
        self.sample_index += frame_samples as u64;

        Some(AudioPacket { timestamp, payload })
    }
}

pub async fn run_encoder(
    config: Config,
    mut audio_rx: broadcast::Receiver<Vec<f32>>,
    sender: broadcast::Sender<AudioPacket>,
    shutdown: Arc<tokio::sync::Notify>,
    client_count: Arc<std::sync::atomic::AtomicUsize>,
) -> anyhow::Result<()> {
    let mut encoder = AudioEncoder::new(&config)?;
    let frame_samples = config.audio_frame_samples();

    // Calculate how many input samples we need to produce one output frame
    let input_rate = config.get_input_sample_rate() as f64;
    let output_rate = config.get_output_sample_rate() as f64;
    let ratio = input_rate / output_rate;
    let input_samples_per_frame = (frame_samples as f64 * ratio).ceil() as usize;

    let mut internal_buffer: Vec<f32> = Vec::new();

    log::info!("Encoder started - input_rate={}, output_rate={}, ratio={}, input_samples_per_frame={}, output_frame={}",
        config.get_input_sample_rate(), config.get_output_sample_rate(), ratio, input_samples_per_frame, frame_samples);

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                return Ok(());
            }
            result = audio_rx.recv() => {
                let clients = client_count.load(std::sync::atomic::Ordering::Relaxed);
                if clients == 0 {
                    if let Ok(_) = result {
                        internal_buffer.clear();
                        encoder.resample_buffer.clear();
                        encoder.resampled_output_buffer.clear();
                    }
                    continue;
                }

                match result {
                    Ok(samples_vec) => {
                        internal_buffer.extend_from_slice(&samples_vec);
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Encoder lagged, dropping {} sample chunks", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(());
                    }
                }

                // Process in chunks that can produce output frames
                while internal_buffer.len() >= input_samples_per_frame {
                    let frame: Vec<f32> = internal_buffer.drain(0..input_samples_per_frame).collect();
                    if let Some(packet) = encoder.encode(&frame) {
                        let _ = sender.send(packet);
                    }
                }
            }
        }
    }
}
