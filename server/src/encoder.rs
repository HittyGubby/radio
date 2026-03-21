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
    buffer: Vec<f32>,
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
            buffer: Vec::new(),
        })
    }

    fn create_opus_encoder(config: &Config) -> anyhow::Result<Encoder> {
        let mut encoder = Encoder::new(
            config.get_input_sample_rate(),
            Channels::Mono,
            Application::Audio,
        )?;

        encoder.set_bitrate(opus::Bitrate::Bits(config.audio_bitrate as i32))?;
        encoder.set_complexity(10)?;

        Ok(encoder)
    }

    pub fn encode(&mut self, samples: &[f32]) -> Option<AudioPacket> {
        let frame_samples =
            (self.config.get_input_sample_rate() as usize * self.config.audio_frame_ms) / 1000;

        if samples.is_empty() {
            return None;
        }
        self.buffer.extend_from_slice(samples);
        if self.buffer.len() < frame_samples {
            return None;
        }
        let frame: Vec<f32> = self.buffer.drain(0..frame_samples).collect();

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
    let frame_samples = (config.get_input_sample_rate() as usize * config.audio_frame_ms) / 1000;

    let mut internal_buffer: Vec<f32> = Vec::new();

    log::info!(
        "Encoder started - input_rate={}, output_rate={}, frame_samples={}",
        config.get_input_sample_rate(),
        config.get_output_sample_rate(),
        frame_samples
    );

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
                        encoder.buffer.clear();
                    }
                    continue;
                }

                match result {
                    Ok(samples_vec) => {
                        if !samples_vec.is_empty() {
                            internal_buffer.extend_from_slice(&samples_vec);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Encoder lagged, dropping {} sample chunks", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(());
                    }
                }

                while internal_buffer.len() >= frame_samples {
                    let frame: Vec<f32> = internal_buffer.drain(0..frame_samples).collect();
                    if let Some(packet) = encoder.encode(&frame) {
                        // Don't send packets when idle - only send if payload has meaningful size
                        // For typical Opus: meaningful packets are >= 10 bytes
                        // For PCM: meaningful packets are full frame size (not just zeros)
                        if packet.payload.len() >= 10 {
                            let _ = sender.send(packet);
                        }
                    }
                }
            }
        }
    }
}
