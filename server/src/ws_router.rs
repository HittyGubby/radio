use crate::config::Config;
use crate::encoder::AudioPacket;
use crate::spectrogram::SpectroPacket;
use futures_util::{SinkExt, StreamExt};
use hyper::server::conn::Http;
use hyper::service::service_fn;
use hyper::{Body, Request, Response, StatusCode};
use log::{error, info, warn};
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::protocol::Message;

async fn handle_http_request(
    req: Request<Body>,
    config: Config,
) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path();

    if path == "/config" {
        let body = serde_json::to_string(&config).unwrap();
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
    packets: Vec<SpectroPacket>,
    max_duration_ms: usize,
}

impl SpectroHistory {
    pub fn new(duration_seconds: usize) -> Self {
        Self {
            packets: Vec::new(),
            max_duration_ms: duration_seconds * 1000,
        }
    }

    pub fn add(&mut self, packet: SpectroPacket, fps: usize) {
        self.packets.push(packet);

        let total_ms = self.packets.len() * 1000 / fps;
        if total_ms > self.max_duration_ms {
            let to_remove = (total_ms - self.max_duration_ms) * fps / 1000 + 1;
            self.packets.drain(0..to_remove.min(self.packets.len()));
        }
    }

    pub fn get_all(&self) -> &[SpectroPacket] {
        &self.packets
    }
}

fn serialize_audio_packet(packet: &AudioPacket) -> Vec<u8> {
    let mut data = Vec::with_capacity(8 + packet.payload.len());
    data.extend_from_slice(&packet.timestamp.to_le_bytes());
    data.extend_from_slice(&packet.payload);
    data
}

fn serialize_spectro_packet(packet: &SpectroPacket) -> Vec<u8> {
    let row_count = packet.rows.len() as u8;
    let row_data: Vec<u8> = packet.rows.iter().flatten().copied().collect();

    let mut data = Vec::with_capacity(8 + 1 + row_data.len());
    data.extend_from_slice(&packet.start_timestamp.to_le_bytes());
    data.push(row_count);
    data.extend_from_slice(&row_data);
    data
}

enum ClientType {
    Audio(broadcast::Receiver<AudioPacket>),
    Spectro(broadcast::Receiver<SpectroPacket>),
}

async fn handle_client(
    mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    client_type: ClientType,
    client_count: Arc<AtomicUsize>,
) {
    let client_type_name = match &client_type {
        ClientType::Audio(_) => "audio",
        ClientType::Spectro(_) => "spectrogram",
    };

    match client_type {
        ClientType::Audio(mut rx) => loop {
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
        },
        ClientType::Spectro(mut rx) => loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(packet) => {
                            let data = serialize_spectro_packet(&packet);
                            if let Err(e) = ws.send(Message::Binary(data)).await {
                                error!("Failed to send spectro packet: {}", e);
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
        },
    }

    client_count.fetch_sub(1, Ordering::Relaxed);
    let current_clients = client_count.load(Ordering::Relaxed);
    info!("Client disconnected: {} (total: {})", client_type_name, current_clients);
}

pub async fn run_ws_server(
    config: Config,
    audio_rx: broadcast::Receiver<AudioPacket>,
    spectro_rx: broadcast::Receiver<SpectroPacket>,
    audio_history: Arc<Mutex<AudioHistory>>,
    spectro_history: Arc<Mutex<SpectroHistory>>,
    shutdown: Arc<tokio::sync::Notify>,
    client_count: Arc<AtomicUsize>,
) -> anyhow::Result<()> {
    info!("Starting WebSocket server on {}", config.ws_bind);

    let listener = tokio::net::TcpListener::bind(&config.ws_bind).await?;

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                return Ok(());
            }
            result = listener.accept() => {
                match result {
                    Ok((mut stream, _addr)) => {
                        let audio_path = config.audio_path.clone();
                        let spectro_path = config.spectro_path.clone();
                        let config_path = config.config_path.clone();
                        let config_clone = config.clone();
                        let audio_rx_clone = audio_rx.resubscribe();
                        let spectro_rx_clone = spectro_rx.resubscribe();
                        let audio_history_clone = audio_history.clone();
                        let spectro_history_clone = spectro_history.clone();
                        let client_count_clone = client_count.clone();

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
                            } else if path == audio_path || path == spectro_path {
                                let (path_tx, path_rx) = tokio::sync::oneshot::channel();

                                let mut ws = match tokio_tungstenite::accept_hdr_async(stream, |req: &tungstenite::handshake::server::Request, resp: tungstenite::handshake::server::Response| {
                                    let req_path = req.uri().path();
                                    if req_path == audio_path || req_path == spectro_path {
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

                                if ws_path == audio_path {
                                    let history = audio_history_clone.lock().await.get_all().to_vec();
                                    for packet in &history {
                                        let data = serialize_audio_packet(packet);
                                        if let Err(_e) = ws.send(Message::Binary(data)).await {
                                            return;
                                        }
                                    }
                                    handle_client(ws, ClientType::Audio(audio_rx_clone), client_count_clone).await;
                                } else if ws_path == spectro_path {
                                    let history = spectro_history_clone.lock().await.get_all().to_vec();
                                    for packet in &history {
                                        let data = serialize_spectro_packet(packet);
                                        if let Err(_e) = ws.send(Message::Binary(data)).await {
                                            return;
                                        }
                                    }
                                    handle_client(ws, ClientType::Spectro(spectro_rx_clone), client_count_clone).await;
                                }
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