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
use log::error;
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
    let spectro_tx_for_ws = spectro_tx.clone();
    let spectro_tx_for_spectro = spectro_tx.clone();

    let client_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let spectro_client_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Create a single capture stream at input_sample_rate for both spectrogram and encoder
    // Both will receive audio at the same rate (48000 Hz by default)

    let capture_config = config.clone();
    let _audio_capture_handle =
        start_audio_capture(&capture_config, audio_samples_tx.clone(), true).await?;
    log::info!(
        "Audio capture started - spectrogram at {}Hz, encoder at {}Hz",
        config.get_input_sample_rate(),
        config.get_output_sample_rate()
    );

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

    let ws_shutdown = shutdown.clone();
    let ws_config = config.clone();
    let ws_audio_rx = audio_tx.subscribe();
    let ws_client_count = client_count.clone();

    let ws_task = tokio::spawn(async move {
        run_ws_server(
            ws_config,
            ws_audio_rx,
            ws_shutdown,
            ws_client_count,
            Some(spectro_tx_for_ws.subscribe()),
            Some(spectro_client_count.clone()),
        )
        .await
    });

    let spectro_shutdown = shutdown.clone();
    let spectro_config = config.clone();
    let mut spectro_audio_rx = audio_samples_tx.subscribe();
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

        let mut sample_buffer: Vec<f32> = Vec::new();
        let target_samples = spectro_config.spectro_frame_samples();
        let output_frame_interval =
            Duration::from_millis((spectro_config.spectro_frame_ms * frames_per_packet) as u64);
        let mut last_output_frame_time = Instant::now();
        let mut accumulated_frames: Vec<(u64, Vec<u8>)> = Vec::new();
        let mut global_frame_index = 0u64; // declare once before loop

        loop {
            tokio::select! {
                _ = spectro_shutdown.notified() => { break; }
                result = spectro_audio_rx.recv() => {
                    if let Ok(samples) = result {
                        sample_buffer.extend(samples.iter());

                        while sample_buffer.len() >= target_samples {
                            let frame_samples: Vec<f32> = sample_buffer.drain(..target_samples).collect();
                            let frequency_data = processor.process(&frame_samples);

                            // Push frame with global index
                            accumulated_frames.push((global_frame_index, frequency_data));
                            global_frame_index += 1;
                        }

                        // Keep buffer from growing too large
                        if sample_buffer.len() > target_samples * 2 {
                            let excess = sample_buffer.len() - target_samples;
                            sample_buffer.drain(0..excess);
                        }

                        // Send packet if enough frames or timeout
                        let now = Instant::now();
                        let elapsed_output = now.duration_since(last_output_frame_time);
                        if accumulated_frames.len() >= frames_per_packet
                            || (elapsed_output >= output_frame_interval && !accumulated_frames.is_empty())
                        {
                            // Only send if the packet contains non-empty frames (avoid sending packets where all frames are empty)
                            if !accumulated_frames.is_empty() {
                                let packet = spectrogram_ws::SpectrogramPacket::new(accumulated_frames.clone());
                                
                                // Check if all frames are empty (zero length data)
                                if !packet.is_empty() {
                                    // Send to clients
                                    let _ = spectro_tx_for_spectro.send(packet);
                                }
                            }

                            accumulated_frames.clear();
                            last_output_frame_time = now;
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        result = encoder_task => {
            match result {
                Ok(_) => {},
                Err(e) => error!("Encoder task error: {}", e),
            }
        }
        result = ws_task => {
            match result {
                Ok(_) => {},
                Err(e) => error!("WebSocket server task error: {}", e),
            }
        }
    }
    Ok(())
}
