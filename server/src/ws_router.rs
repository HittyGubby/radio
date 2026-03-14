use crate::config::Config;
use crate::encoder::AudioPacket;
use futures_util::{SinkExt, StreamExt};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Request, Response, StatusCode};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::protocol::Message;

/// Config sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrontendConfig {
    pub output_sample_rate: u32,
    pub audio_bitrate: u32,
    pub jitter_buffer_ms: usize,
    pub input_sample_rate: u32,
    pub spectro_max_freq: u32,
}

impl From<&Config> for FrontendConfig {
    fn from(config: &Config) -> Self {
        // Calculate actual max frequency based on input sample rate
        let input_sample_rate = config.get_input_sample_rate();
        let nyquist_freq = input_sample_rate / 2;
        let actual_max_freq = config.spectro_max_freq.min(nyquist_freq);

        Self {
            output_sample_rate: config.get_output_sample_rate(),
            audio_bitrate: config.audio_bitrate,
            jitter_buffer_ms: config.jitter_buffer_ms,
            input_sample_rate: config.get_input_sample_rate(),
            spectro_max_freq: actual_max_freq,
        }
    }
}

async fn handle_http_request(
    req: Request<Body>,
    config: Config,
) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path();

    if path == "/config" {
        let frontend_config = FrontendConfig::from(&config);
        let body = serde_json::to_string(&frontend_config).unwrap();
        Ok(Response::builder()
            .header("content-type", "application/json")
            .header("access-control-allow-origin", "*")
            .body(Body::from(body))
            .unwrap())
    } else {
        Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .unwrap())
    }
}

pub struct AudioHistory {
    packets: Vec<AudioPacket>,
    max_duration_samples: usize,
}

impl AudioHistory {
    pub fn new(sample_rate: u32, duration_seconds: usize) -> Self {
        Self {
            packets: Vec::new(),
            max_duration_samples: sample_rate as usize * duration_seconds,
        }
    }

    pub fn add(&mut self, packet: AudioPacket, frame_samples: usize) {
        self.packets.push(packet);

        let total_samples = self.packets.len() * frame_samples;
        if total_samples > self.max_duration_samples {
            let to_remove = (total_samples - self.max_duration_samples) / frame_samples + 1;
            self.packets.drain(0..to_remove.min(self.packets.len()));
        }
    }

    pub fn get_all(&self) -> &[AudioPacket] {
        &self.packets
    }
}

pub struct SpectroHistory {
    packets: Vec<crate::spectrogram_ws::SpectrogramPacket>,
    max_duration_frames: usize,
}

impl SpectroHistory {
    pub fn new(max_duration_frames: usize) -> Self {
        Self {
            packets: Vec::new(),
            max_duration_frames,
        }
    }

    pub fn add(&mut self, packet: crate::spectrogram_ws::SpectrogramPacket) {
        self.packets.push(packet);

        if self.packets.len() > self.max_duration_frames {
            let to_remove = self.packets.len() - self.max_duration_frames;
            self.packets.drain(0..to_remove);
        }
    }

    pub fn get_all(&self) -> &[crate::spectrogram_ws::SpectrogramPacket] {
        &self.packets
    }
}

fn serialize_audio_packet(packet: &AudioPacket) -> Vec<u8> {
    let mut data = Vec::with_capacity(8 + packet.payload.len());
    data.extend_from_slice(&packet.timestamp.to_le_bytes());
    data.extend_from_slice(&packet.payload);
    data
}

async fn handle_client(
    mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mut rx: broadcast::Receiver<AudioPacket>,
    client_count: Arc<AtomicUsize>,
) {
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(packet) => {
                        let data = serialize_audio_packet(&packet);
                        if let Err(e) = ws.send(Message::Binary(data)).await {
                            error!("Failed to send audio packet: {}", e);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Audio client lagged, dropping {} packets", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            result = ws.next() => {
                match result {
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    client_count.fetch_sub(1, Ordering::Relaxed);
    let current_clients = client_count.load(Ordering::Relaxed);
    info!("Client disconnected: audio (total: {})", current_clients);
}

async fn handle_spectro_client(
    mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mut rx: broadcast::Receiver<crate::spectrogram_ws::SpectrogramPacket>,
    client_count: Arc<AtomicUsize>,
    spectro_history: Option<Arc<Mutex<SpectroHistory>>>,
) {
    // Send historical frames on initial connection
    if let Some(history) = spectro_history {
        let history_packets = history.lock().await.get_all().to_vec();
        for packet in &history_packets {
            let data = packet.to_bytes();
            if let Err(_e) = ws.send(Message::Binary(data)).await {
                return;
            }
        }
        info!("Sent {} historical spectrogram packets to new client", history_packets.len());
    }

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(packet) => {
                        let data = packet.to_bytes();
                        if let Err(e) = ws.send(Message::Binary(data)).await {
                            error!("Failed to send spectrogram packet: {}", e);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!("Spectrogram client lagged, dropping {} packets", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            result = ws.next() => {
                match result {
                    Some(Ok(Message::Close(_))) => {
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    client_count.fetch_sub(1, Ordering::Relaxed);
    let current_clients = client_count.load(Ordering::Relaxed);
    info!("Client disconnected: spectrogram (total: {})", current_clients);
}

pub async fn run_ws_server(
    config: Config,
    audio_rx: broadcast::Receiver<AudioPacket>,
    audio_history: Arc<Mutex<AudioHistory>>,
    shutdown: Arc<tokio::sync::Notify>,
    client_count: Arc<AtomicUsize>,
    spectro_rx: Option<broadcast::Receiver<crate::spectrogram_ws::SpectrogramPacket>>,
    spectro_client_count: Option<Arc<AtomicUsize>>,
    spectro_history: Option<Arc<Mutex<SpectroHistory>>>,
) -> anyhow::Result<()> {
    info!("Starting WebSocket server on {}", config.ws_bind);
    log::info!("WebSocket server task started");

    log::info!("Attempting to bind listener to {}", config.ws_bind);
    let listener = tokio::net::TcpListener::bind(&config.ws_bind).await.map_err(|e| {
        log::error!("Failed to bind listener to {}: {}", config.ws_bind, e);
        e
    })?;
    info!("WebSocket listener bound successfully");

    log::info!("Entering WebSocket server loop");
    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                info!("WebSocket server received shutdown signal");
                return Ok(());
            }
            result = listener.accept() => {
                match result {
                    Ok((mut stream, _addr)) => {
                        let audio_path = config.audio_path.clone();
                        let config_path = config.config_path.clone();
                        let spectro_path = config.spectro_path.clone();
                        let config_clone = config.clone();
                        let audio_rx_clone = audio_rx.resubscribe();
                        let audio_history_clone = audio_history.clone();
                        let client_count_clone = client_count.clone();
                        let spectro_rx_clone = spectro_rx.as_ref().map(|rx| rx.resubscribe());
                        let spectro_client_count_clone = spectro_client_count.as_ref().cloned();
                        let spectro_history_clone = spectro_history.clone();

                        tokio::spawn(async move {
                            let mut peek_buf = [0u8; 2048];

                            let n = match (&mut stream).peek(&mut peek_buf).await {
                                Ok(n) => n,
                                Err(e) => {
                                    error!("Failed to peek at stream: {}", e);
                                    return;
                                }
                            };

                            if n == 0 {
                                return;
                            }

                            let request_str = String::from_utf8_lossy(&peek_buf[..n]);

                            let path = if let Some(start) = request_str.find("GET ") {
                                if let Some(end) = request_str[start..].find(" HTTP") {
                                    request_str[start + 4..start + end].to_string()
                                } else {
                                    "/".to_string()
                                }
                            } else {
                                "/".to_string()
                            };

                            if path == config_path {
                                let service = service_fn(move |req| {
                                    handle_http_request(req, config_clone.clone())
                                });

                                if let Err(e) = Http::new().serve_connection(stream, service).await {
                                    error!("Error serving HTTP connection: {}", e);
                                }
                            } else if path == spectro_path {
                                if !config_clone.spectro_enabled {
                                    return;
                                }

                                if spectro_rx_clone.is_none() || spectro_client_count_clone.is_none() {
                                    error!("Spectrogram channel or client count not initialized");
                                    return;
                                }

                                let (path_tx, path_rx) = tokio::sync::oneshot::channel();

                                let mut ws = match tokio_tungstenite::accept_hdr_async(stream, |req: &tungstenite::handshake::server::Request, resp: tungstenite::handshake::server::Response| {
                                    let req_path = req.uri().path();
                                    if req_path == spectro_path {
                                        let _ = path_tx.send(req_path.to_string());
                                        Ok(resp)
                                    } else {
                                        Err(tungstenite::handshake::server::ErrorResponse::new(Some("404 Not Found".to_string())))
                                    }
                                }).await {
                                    Ok(ws) => ws,
                                    Err(_e) => {
                                        return;
                                    }
                                };

                                let ws_path = match path_rx.await {
                                    Ok(p) => p,
                                    Err(_) => {
                                        let _ = ws.close(None).await;
                                        return;
                                    }
                                };

                                let client_count = spectro_client_count_clone.unwrap();
                                client_count.fetch_add(1, Ordering::Relaxed);
                                let current_clients = client_count.load(Ordering::Relaxed);
                                info!("Client connected: {} (total: {})", ws_path, current_clients);

                                handle_spectro_client(
                                    ws,
                                    spectro_rx_clone.unwrap(),
                                    client_count,
                                    spectro_history_clone,
                                ).await;
                            } else if path == audio_path {
                                let (path_tx, path_rx) = tokio::sync::oneshot::channel();

                                let mut ws = match tokio_tungstenite::accept_hdr_async(stream, |req: &tungstenite::handshake::server::Request, resp: tungstenite::handshake::server::Response| {
                                    let req_path = req.uri().path();
                                    if req_path == audio_path {
                                        let _ = path_tx.send(req_path.to_string());
                                        Ok(resp)
                                    } else {
                                        Err(tungstenite::handshake::server::ErrorResponse::new(Some("404 Not Found".to_string())))
                                    }
                                }).await {
                                    Ok(ws) => ws,
                                    Err(_e) => {
                                        return;
                                    }
                                };

                                let ws_path = match path_rx.await {
                                    Ok(p) => p,
                                    Err(_) => {
                                        let _ = ws.close(None).await;
                                        return;
                                    }
                                };

                                client_count_clone.fetch_add(1, Ordering::Relaxed);
                                let current_clients = client_count_clone.load(Ordering::Relaxed);
                                info!("Client connected: {} (total: {})", ws_path, current_clients);

                                let history = audio_history_clone.lock().await.get_all().to_vec();
                                for packet in &history {
                                    let data = serialize_audio_packet(packet);
                                    if let Err(_e) = ws.send(Message::Binary(data)).await {
                                        return;
                                    }
                                }
                                handle_client(ws, audio_rx_clone, client_count_clone).await;
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept connection: {}", e);
                    }
                }
            }
        }
    }
}