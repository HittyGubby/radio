mod audio_input;
mod config;
mod encoder;
mod pipewire_devices;
mod spectrogram;
mod ws_router;

use audio_input::start_audio_capture;
use clap::Parser;
use config::Config;
use encoder::run_encoder;
use log::error;
use spectrogram::run_spectrogram;
use std::sync::Arc;
use tokio::signal;
use ws_router::run_ws_server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = Config::parse();

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
    let (spectro_tx, _) = tokio::sync::broadcast::channel(100);

    let client_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

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

    let spectro_shutdown = shutdown.clone();
    let spectro_config = config.clone();
    let spectro_tx_clone = spectro_tx.clone();
    let spectro_client_count = client_count.clone();

    let spectro_task = tokio::spawn(async move {
        run_spectrogram(
            spectro_config,
            audio_samples_tx.subscribe(),
            spectro_tx_clone,
            spectro_shutdown,
            spectro_client_count,
        )
        .await
    });

    let audio_history = Arc::new(tokio::sync::Mutex::new(ws_router::AudioHistory::new(
        config.sample_rate,
        2,
    )));

    let spectro_history = Arc::new(tokio::sync::Mutex::new(ws_router::SpectroHistory::new(3)));

    let ws_shutdown = shutdown.clone();
    let ws_config = config.clone();
    let ws_audio_rx = audio_tx.subscribe();
    let ws_spectro_rx = spectro_tx.subscribe();
    let ws_audio_history = audio_history.clone();
    let ws_spectro_history = spectro_history.clone();
    let ws_client_count = client_count.clone();

    let ws_task = tokio::spawn(async move {
        run_ws_server(
            ws_config,
            ws_audio_rx,
            ws_spectro_rx,
            ws_audio_history,
            ws_spectro_history,
            ws_shutdown,
            ws_client_count,
        )
        .await
    });

    let history_task_shutdown = shutdown.clone();
    let history_task_config = config.clone();
    let mut history_task_audio_rx = audio_tx.subscribe();
    let mut history_task_spectro_rx = spectro_tx.subscribe();
    let history_task_audio_history = audio_history.clone();
    let history_task_spectro_history = spectro_history.clone();

    tokio::spawn(async move {
        let frame_samples = history_task_config.audio_frame_samples();
        let fps = history_task_config.spectro_fps;

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
                result = history_task_spectro_rx.recv() => {
                    if let Ok(packet) = result {
                        history_task_spectro_history.lock().await.add(packet, fps);
                    }
                }
            }
        }
    });

    let _capture_handle = audio_capture_handle;

    tokio::select! {
        _ = shutdown.notified() => {}
        result = encoder_task => {
            if let Err(e) = result {
                error!("Encoder task error: {}", e);
            }
        }
        result = spectro_task => {
            if let Err(e) = result {
                error!("Spectrogram task error: {}", e);
            }
        }
        result = ws_task => {
            if let Err(e) = result {
                error!("WebSocket server task error: {}", e);
            }
        }
    }
    Ok(())
}