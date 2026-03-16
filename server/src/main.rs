mod audio_input;
mod config;
mod encoder;
mod pipewire_devices;
mod spectrogram_processor;
mod spectrogram_ws;
mod ws_router;

use audio_input::start_audio_capture;
use config::Config;
use encoder::run_encoder;
use log::{error, info};
use std::sync::Arc;
use tokio::signal;
use ws_router::run_ws_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = Config::load();

    if config.list_devices {
        let devices = pipewire_devices::list_audio_devices().await?;

        println!("{:<6} {:<30} {}", "ID", "NAME", "DESCRIPTION");
        println!("{}", "-".repeat(70));

        for device in devices {
            println!(
                "{:<6} {:<30} {}",
                device.id,
                if device.name.len() > 29 {
                    format!("{}...", &device.name[..26])
                } else {
                    device.name.clone()
                },
                device.description
            );
        }

        return Ok(());
    }

    config.validate()?;

    let shutdown = Arc::new(tokio::sync::Notify::new());
    let shutdown_ctrl_c = shutdown.clone();

    tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Failed to setup Ctrl+C handler");
        shutdown_ctrl_c.notify_waiters();
    });

    let (audio_samples_tx, _) = tokio::sync::broadcast::channel(1000);
    let (audio_tx, _) = tokio::sync::broadcast::channel(200);
    let (spectro_tx, _) = tokio::sync::broadcast::channel::<spectrogram_ws::SpectrogramPacket>(500);
    let spectro_tx_for_task = spectro_tx.clone();

    let client_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let spectro_client_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let audio_capture_handle = start_audio_capture(&config, audio_samples_tx.clone()).await?;
    log::info!("Audio capture started");

    let encoder_shutdown = shutdown.clone();
    let encoder_config = config.clone();
    let encoder_tx = audio_tx.clone();
    let encoder_client_count = client_count.clone();
    let audio_samples_tx_for_encoder = audio_samples_tx.clone();

    let encoder_task = tokio::spawn(async move {
        run_encoder(
            encoder_config,
            audio_samples_tx_for_encoder.subscribe(),
            encoder_tx,
            encoder_shutdown,
            encoder_client_count,
        )
        .await
    });

    let audio_history = Arc::new(tokio::sync::Mutex::new(ws_router::AudioHistory::new(
        config.get_input_sample_rate(),
        2,
    )));

    let spectro_history = Arc::new(tokio::sync::Mutex::new(ws_router::SpectroHistory::new(500)));

    let ws_shutdown = shutdown.clone();
    let ws_config = config.clone();
    let ws_audio_rx = audio_tx.subscribe();
    let ws_audio_history = audio_history.clone();
    let ws_client_count = client_count.clone();
    let ws_spectro_history = spectro_history.clone();

    let ws_task = tokio::spawn(async move {
        run_ws_server(
            ws_config,
            ws_audio_rx,
            ws_audio_history,
            ws_shutdown,
            ws_client_count,
            Some(spectro_tx_for_task.subscribe()),
            Some(spectro_client_count.clone()),
            Some(ws_spectro_history),
        )
        .await
    });

    let history_task_shutdown = shutdown.clone();
    let history_task_config = config.clone();
    let mut history_task_audio_rx = audio_tx.subscribe();
    let history_task_audio_history = audio_history.clone();

    tokio::spawn(async move {
        let frame_samples = history_task_config.audio_frame_samples();

        loop {
            tokio::select! {
                _ = history_task_shutdown.notified() => {
                    break;
                }
                result = history_task_audio_rx.recv() => {
                    if let Ok(packet) = result {
                        history_task_audio_history.lock().await.add(packet, frame_samples);
                    }
                }
            }
        }
    });

    // Spectrogram history task
    let spectro_history_task_shutdown = shutdown.clone();
    let mut spectro_history_task_rx = spectro_tx.subscribe();
    let spectro_history_task_history = spectro_history.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = spectro_history_task_shutdown.notified() => {
                    break;
                }
                result = spectro_history_task_rx.recv() => {
                    if let Ok(packet) = result {
                        spectro_history_task_history.lock().await.add(packet);
                    }
                }
            }
        }
    });

    // Spectrogram processing task
    let spectro_shutdown = shutdown.clone();
    let spectro_config = config.clone();
    let mut spectro_audio_rx = audio_samples_tx.subscribe();
    let spectro_tx_clone = spectro_tx.clone();

    let _spectro_task = tokio::spawn(async move {
        use spectrogram_processor::SpectrogramProcessor;
        use std::time::{Duration, Instant};

        let mut processor = match SpectrogramProcessor::new(&spectro_config) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to create spectrogram processor: {}", e);
                return;
            }
        };

        // Calculate the ratio between input and output sample rates
        // This determines how many spectrogram frames to pack into each packet
        let input_rate = spectro_config.get_input_sample_rate();
        let output_rate = spectro_config.get_output_sample_rate();
        let frames_per_packet = if input_rate > output_rate {
            (input_rate / output_rate).max(1) as usize
        } else {
            1
        };

        info!("Spectrogram: input_rate={}, output_rate={}, frames_per_packet={}", input_rate, output_rate, frames_per_packet);

        let mut sample_buffer: Vec<f32> = Vec::new();
        let target_samples = spectro_config.spectro_frame_samples();
        let input_frame_interval = Duration::from_millis(spectro_config.spectro_frame_ms as u64);
        let output_frame_interval = Duration::from_millis((spectro_config.spectro_frame_ms * frames_per_packet) as u64);
        let mut last_input_frame_time = Instant::now();
        let mut last_output_frame_time = Instant::now();
        let mut accumulated_frames: Vec<(u64, Vec<u8>)> = Vec::new();

        // Track audio sample index for timestamp synchronization
        // Use the SAME timebase as audio encoder (output sample count)
        let mut audio_sample_index = 0u64;

        loop {
            tokio::select! {
                _ = spectro_shutdown.notified() => {
                    break;
                }
                result = spectro_audio_rx.recv() => {
                    if let Ok(samples) = result {
                        sample_buffer.extend(samples.iter());

                        // Check if we have enough samples for a frame
                        if sample_buffer.len() >= target_samples {
                            // Rate limiting - only process at input frame interval
                            let now = Instant::now();
                            let elapsed = now.duration_since(last_input_frame_time);

                            if elapsed >= input_frame_interval {
                                // Take samples for processing
                                let frame_samples: Vec<f32> = sample_buffer.drain(..target_samples).collect();

                                // Process FFT
                                let frequency_data = processor.process(&frame_samples);

                                // Each frame gets its own timestamp
                                let frame_timestamp = audio_sample_index;
                                accumulated_frames.push((frame_timestamp, frequency_data));

                                last_input_frame_time = now;
                            }
                        }

                        // Keep buffer from growing too large
                        if sample_buffer.len() > target_samples * 2 {
                            let excess = sample_buffer.len() - target_samples;
                            sample_buffer.drain(0..excess);
                        }

                        // Check if we have enough frames to send a packet
                        let now = Instant::now();
                        let elapsed_output = now.duration_since(last_output_frame_time);

                        if accumulated_frames.len() >= frames_per_packet || (elapsed_output >= output_frame_interval && !accumulated_frames.is_empty()) {
                            // Create spectrogram packet with accumulated frames (each with its own timestamp)
                            let packet = spectrogram_ws::SpectrogramPacket::new_with_frames(
                                accumulated_frames.clone(),
                            );

                            // Send to clients
                            if let Err(_) = spectro_tx_clone.send(packet) {
                                // No spectrogram clients connected
                            }

                            accumulated_frames.clear();
                            last_output_frame_time = now;
                        }

                        // Update audio sample index
                        // Calculate how many output samples this input chunk represents
                        let output_samples_for_input = samples.len() as u64 * output_rate as u64 / input_rate as u64;
                        audio_sample_index += output_samples_for_input;
                    }
                }
            }
        }
    });

    let _capture_handle = audio_capture_handle;

    tokio::select! {
        _ = shutdown.notified() => {
            info!("Shutdown signal received");
        }
        result = encoder_task => {
            match result {
                Ok(_) => info!("Encoder task completed"),
                Err(e) => error!("Encoder task error: {}", e),
            }
        }
        result = ws_task => {
            match result {
                Ok(_) => info!("WebSocket server task completed"),
                Err(e) => error!("WebSocket server task error: {}", e),
            }
        }
    }
    Ok(())
}