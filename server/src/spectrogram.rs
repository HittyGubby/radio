use crate::config::Config;
use log::warn;
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct SpectroPacket {
    pub start_timestamp: u64,
    pub rows: Vec<Vec<u8>>,
}

pub struct SpectrogramProcessor {
    fft_size: usize,
    fft_step: usize,
    spectro_bins: usize,
    window: Vec<f32>,
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    sample_index: u64,
    db_min: f32,
    db_max: f32,
    smoothing_alpha: f32,
    previous_row: Vec<u8>,
}

impl SpectrogramProcessor {
    pub fn new(config: &Config) -> Self {
        let fft_size = config.fft_size;
        let fft_step = config.fft_step();
        let spectro_bins = config.spectro_bins;

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        let window = Self::create_hann_window(fft_size);
        let scratch = vec![Complex::new(0.0, 0.0); fft_size];

        Self {
            fft_size,
            fft_step,
            spectro_bins,
            window,
            fft,
            scratch,
            sample_index: 0,
            db_min: -90.0,
            db_max: -20.0,
            smoothing_alpha: 0.4,
            previous_row: vec![0u8; spectro_bins],
        }
    }

    fn create_hann_window(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                let phase = 2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32;
                0.5 * (1.0 - phase.cos())
            })
            .collect()
    }

    pub fn process(&mut self, samples: &[f32]) -> Option<SpectroPacket> {
        if samples.len() < self.fft_size {
            return None;
        }

        let mut rows = Vec::new();
        let mut current_sample = 0;

        while current_sample + self.fft_size <= samples.len() {
            let _timestamp = self.sample_index + current_sample as u64;

            for i in 0..self.fft_size {
                self.scratch[i] = Complex::new(samples[current_sample + i] * self.window[i], 0.0);
            }

            self.fft.process(&mut self.scratch);

            let row = self.compute_row();
            rows.push(row);

            current_sample += self.fft_step;
        }

        self.sample_index += current_sample as u64;

        if rows.is_empty() {
            return None;
        }

        Some(SpectroPacket {
            start_timestamp: self.sample_index - current_sample as u64,
            rows,
        })
    }

    fn compute_row(&mut self) -> Vec<u8> {
        let nyquist = self.fft_size / 2;
        let mut magnitudes = vec![0.0f32; nyquist];

        for i in 0..nyquist {
            let mag = self.scratch[i].norm();
            let db = 20.0 * mag.log10().max(self.db_min);
            magnitudes[i] = db;
        }

        let downsampled = self.downsample(&magnitudes);
        let normalized = self.normalize(&downsampled);
        let smoothed = self.smooth(&normalized);

        smoothed
    }

    fn downsample(&self, magnitudes: &[f32]) -> Vec<f32> {
        let input_len = magnitudes.len();
        let output_len = self.spectro_bins;
        let mut result = vec![0.0f32; output_len];

        if input_len <= output_len {
            result[..input_len].copy_from_slice(magnitudes);
            return result;
        }

        let bin_size = input_len as f64 / output_len as f64;

        for out_idx in 0..output_len {
            let start = (out_idx as f64 * bin_size) as usize;
            let end = ((out_idx + 1) as f64 * bin_size).min(input_len as f64) as usize;
            let count = end - start;

            if count > 0 {
                let sum: f32 = magnitudes[start..end].iter().sum();
                result[out_idx] = sum / count as f32;
            }
        }

        result
    }

    fn normalize(&self, values: &[f32]) -> Vec<u8> {
        let range = self.db_max - self.db_min;
        values
            .iter()
            .map(|&v| {
                let normalized = ((v - self.db_min) / range).clamp(0.0, 1.0);
                (normalized * 255.0) as u8
            })
            .collect()
    }

    fn smooth(&mut self, current: &[u8]) -> Vec<u8> {
        let mut result = vec![0u8; current.len()];
        let alpha = self.smoothing_alpha;

        for i in 0..current.len() {
            result[i] =
                (alpha * current[i] as f32 + (1.0 - alpha) * self.previous_row[i] as f32) as u8;
        }

        self.previous_row.clone_from(&result);
        result
    }

    pub fn batch_rows(&self, rows: Vec<Vec<u8>>, batch_size: usize) -> Vec<Vec<u8>> {
        rows.chunks(batch_size)
            .map(|chunk| chunk.iter().flatten().copied().collect())
            .collect()
    }
}

pub async fn run_spectrogram(
    config: Config,
    mut audio_rx: broadcast::Receiver<Vec<f32>>,
    sender: broadcast::Sender<SpectroPacket>,
    shutdown: Arc<tokio::sync::Notify>,
    client_count: Arc<std::sync::atomic::AtomicUsize>,
) -> anyhow::Result<()> {
    let mut processor = SpectrogramProcessor::new(&config);
    let interval_duration = Duration::from_millis(config.spectro_interval_ms());
    let mut interval = tokio::time::interval(interval_duration);
    interval.tick().await;

    let fft_step = config.fft_step();
    let batch_size = 5;

    let mut internal_buffer: Vec<f32> = Vec::new();

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                return Ok(());
            }
            _ = interval.tick() => {
                if client_count.load(std::sync::atomic::Ordering::Relaxed) == 0 {
                    internal_buffer.clear();
                    continue;
                }

                let total_samples = fft_step * batch_size;

                if internal_buffer.len() >= total_samples {
                    let samples: Vec<f32> = internal_buffer.drain(0..total_samples).collect();
                    if let Some(mut packet) = processor.process(&samples) {
                        packet.rows = processor.batch_rows(packet.rows, batch_size);
                        let _ = sender.send(packet);
                    }
                }
            }
            result = audio_rx.recv() => {
                match result {
                    Ok(samples_vec) => {
                        internal_buffer.extend_from_slice(&samples_vec);
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Spectrogram lagged, dropping {} sample chunks", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        return Ok(());
                    }
                }
            }
        }
    }
}
