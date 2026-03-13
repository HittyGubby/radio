use crate::config::Config;
use crate::encoder::AudioPacket;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::protocol::Message;

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

pub async fn handle_client(
    mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mut rx: broadcast::Receiver<AudioPacket>,
    history: &[AudioPacket],
) {
    info!("Audio client connected");

    for packet in history {
        let data = serialize_audio_packet(packet);
        if let Err(e) = ws.send(Message::Binary(data)).await {
            error!("Failed to send history packet: {}", e);
            return;
        }
    }

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
                        error!("Audio client lagged, dropping {} packets", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Audio channel closed");
                        break;
                    }
                }
            }
            result = ws.next() => {
                match result {
                    Some(Ok(Message::Close(_))) => {
                        info!("Audio client disconnected");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("Audio client connection closed");
                        break;
                    }
                }
            }
        }
    }
}

fn serialize_audio_packet(packet: &AudioPacket) -> Vec<u8> {
    let mut data = Vec::with_capacity(8 + packet.payload.len());
    data.extend_from_slice(&packet.timestamp.to_le_bytes());
    data.extend_from_slice(&packet.payload);
    data
}

pub async fn run_audio_ws_server(
    config: Config,
    rx: broadcast::Receiver<AudioPacket>,
    history: Arc<tokio::sync::Mutex<AudioHistory>>,
    shutdown: Arc<tokio::sync::Notify>,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(&config.ws_bind).await?;
    let _full_audio_path = format!("{}{}", config.ws_bind, config.audio_path);

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                return Ok(());
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        info!("Audio connection from {}", addr);

                        let req = match tokio_tungstenite::accept_hdr_async(stream, |req: &tungstenite::handshake::server::Request, resp: tungstenite::handshake::server::Response| {
                            let path = req.uri().path();
                            if path == config.audio_path {
                                Ok(resp)
                            } else {
                                Err(tungstenite::handshake::server::ErrorResponse::new(Some("404 Not Found".to_string())))
                            }
                        }).await {
                            Ok(ws) => ws,
                            Err(e) => {
                                error!("WebSocket handshake failed: {}", e);
                                continue;
                            }
                        };

                        let rx_clone = rx.resubscribe();
                        let history_clone = history.clone();

                        tokio::spawn(async move {
                            handle_client(req, rx_clone, history_clone.lock().await.get_all()).await;
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
