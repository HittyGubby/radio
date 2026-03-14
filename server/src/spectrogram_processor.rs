use crate::config::Config;
use anyhow::Result;
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

/// Spectrogram processor that performs FFT analysis on audio samples
pub struct SpectrogramProcessor {
    fft_planner: FftPlanner<f32>,
    window: Vec<f32>,
    fft_size: usize,
    sample_rate: u32,
    actual_max_freq: u32,  // Actual max frequency based on input sample rate
    min_db: f32,
    max_db: f32,
    max_bins: usize,
}

impl SpectrogramProcessor {
    pub fn new(config: &Config) -> Result<Self> {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(config.spectro_fft_size);
        let _ = fft; // We keep the planner alive

        let window = Self::create_window(config.spectro_fft_size, &config.spectro_window);

        let input_sample_rate = config.get_input_sample_rate();

        // Calculate actual max frequency based on input sample rate
        // Nyquist theorem: max frequency is half the sample rate
        let nyquist_freq = input_sample_rate / 2;
        let actual_max_freq = config.spectro_max_freq.min(nyquist_freq);

        // Calculate maximum frequency bins based on actual max frequency
        let max_bins = std::cmp::min(
            config.spectro_fft_size / 2,
            ((actual_max_freq as f32) / (input_sample_rate as f32) * (config.spectro_fft_size as f32)) as usize,
        );

        Ok(Self {
            fft_planner: planner,
            window,
            fft_size: config.spectro_fft_size,
            sample_rate: input_sample_rate,
            actual_max_freq,
            min_db: config.spectro_min_db,
            max_db: config.spectro_max_db,
            max_bins,
        })
    }

    /// Create a window function to reduce spectral leakage
    fn create_window(size: usize, window_type: &str) -> Vec<f32> {
        let n = size as f32;
        let mut window = Vec::with_capacity(size);

        for i in 0..size {
            let val = match window_type {
                "hamming" => {
                    0.54 - 0.46 * (2.0 * PI * (i as f32) / (n - 1.0)).cos()
                }
                "blackman" => {
                    let a0 = 0.42;
                    let a1 = 0.5;
                    let a2 = 0.08;
                    a0 - a1 * (2.0 * PI * (i as f32) / (n - 1.0)).cos()
                        + a2 * (4.0 * PI * (i as f32) / (n - 1.0)).cos()
                }
                _ => {
                    // Default to Hann window
                    0.5 * (1.0 - (2.0 * PI * (i as f32) / (n - 1.0)).cos())
                }
            };
            window.push(val);
        }

        window
    }

    /// Process audio samples and return frequency data in dB scale
    pub fn process(&mut self, samples: &[f32]) -> Vec<u8> {
        if samples.len() < self.fft_size {
            // Pad with zeros if we don't have enough samples
            let mut padded = vec![0.0; self.fft_size];
            let copy_len = std::cmp::min(samples.len(), self.fft_size);
            padded[..copy_len].copy_from_slice(&samples[..copy_len]);
            return self.compute_fft(&padded);
        } else {
            // Take the first fft_size samples
            return self.compute_fft(&samples[..self.fft_size]);
        }
    }

    fn compute_fft(&mut self, samples: &[f32]) -> Vec<u8> {
        let mut buffer: Vec<Complex<f32>> = samples
            .iter()
            .zip(self.window.iter())
            .map(|(&sample, &window)| Complex::new(sample * window, 0.0))
            .collect();

        let fft = self.fft_planner.plan_fft_forward(self.fft_size);
        fft.process(&mut buffer);

        // Convert to magnitude in dB scale
        let mut result = Vec::with_capacity(self.max_bins);

        for i in 0..self.max_bins {
            let magnitude = buffer[i].norm();
            let db = if magnitude > 0.0 {
                20.0 * magnitude.log10()
            } else {
                self.min_db
            };

            // Scale to 0-255 range
            let scaled = ((db - self.min_db) / (self.max_db - self.min_db) * 255.0).clamp(0.0, 255.0);
            result.push(scaled as u8);
        }

        result
    }

    /// Get the number of frequency bins
    pub fn bin_count(&self) -> usize {
        self.max_bins
    }

    /// Get the frequency for a given bin index
    pub fn bin_to_frequency(&self, bin_index: usize) -> f32 {
        (bin_index as f32) * (self.sample_rate as f32) / (self.fft_size as f32)
    }

    /// Get the actual maximum frequency that can be displayed
    pub fn get_actual_max_freq(&self) -> u32 {
        self.actual_max_freq
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_window() {
        let hann = SpectrogramProcessor::create_window(8, "hann");
        assert_eq!(hann.len(), 8);
        assert!(hann.iter().all(|&x| x >= 0.0 && x <= 1.0));
    }

    #[test]
    fn test_fft_size_validation() {
        let config = Config::default_config();
        let processor = SpectrogramProcessor::new(&config).unwrap();
        assert!(processor.fft_size.is_power_of_two());
    }
}