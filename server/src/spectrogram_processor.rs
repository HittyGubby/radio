use crate::config::Config;
use anyhow::Result;
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

pub struct SpectrogramProcessor {
    fft_planner: FftPlanner<f32>,
    window: Vec<f32>,
    fft_size: usize,
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
        let max_freq = config.spectro_max_freq.min(nyquist_freq);
        let max_bins = std::cmp::min(
            config.spectro_fft_size / 2,
            ((max_freq as f32) / (input_sample_rate as f32) * (config.spectro_fft_size as f32))
                as usize,
        );

        Ok(Self {
            fft_planner: planner,
            window,
            fft_size: config.spectro_fft_size,
            max_bins,
        })
    }

    fn create_window(size: usize, window_type: &str) -> Vec<f32> {
        let n = size as f32;
        let mut window = Vec::with_capacity(size);

        for i in 0..size {
            let val = match window_type {
                "hamming" => 0.54 - 0.46 * (2.0 * PI * (i as f32) / (n - 1.0)).cos(),
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

    pub fn process(&mut self, samples: &[f32]) -> Vec<u8> {
        if samples.len() < self.fft_size {
            let mut padded = vec![0.0; self.fft_size];
            let copy_len = std::cmp::min(samples.len(), self.fft_size);
            padded[..copy_len].copy_from_slice(&samples[..copy_len]);
            return self.compute_fft(&padded);
        } else {
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
        let mut result = Vec::with_capacity(self.max_bins);
        for i in 0..self.max_bins {
            let magnitude = buffer[i].norm();
            result.push(magnitude as u8);
        }

        result
    }
}
