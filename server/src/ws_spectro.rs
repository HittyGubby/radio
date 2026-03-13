use crate::config::Config;
use crate::spectrogram::SpectroPacket;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::protocol::Message;

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

pub async fn handle_client(
    mut ws: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    mut rx: broadcast::Receiver<SpectroPacket>,
    history: &[SpectroPacket],
) {
    info!("Spectrogram client connected");

    for packet in history {
        let data = serialize_spectro_packet(packet);
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
                        let data = serialize_spectro_packet(&packet);
                        if let Err(e) = ws.send(Message::Binary(data)).await {
                            error!("Failed to send spectro packet: {}", e);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        error!("Spectrogram client lagged, dropping {} packets", count);
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Spectrogram channel closed");
                        break;
                    }
                }
            }
            result = ws.next() => {
                match result {
                    Some(Ok(Message::Close(_))) => {
                        info!("Spectrogram client disconnected");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("Spectrogram client connection closed");
                        break;
                    }
                }
            }
        }
    }
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

pub async fn run_spectro_ws_server(
    config: Config,
    rx: broadcast::Receiver<SpectroPacket>,
    history: Arc<tokio::sync::Mutex<SpectroHistory>>,
    shutdown: Arc<tokio::sync::Notify>,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(&config.ws_bind).await?;

    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                return Ok(());
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        info!("Spectrogram connection from {}", addr);

                        let req = match tokio_tungstenite::accept_hdr_async(stream, |req: &tungstenite::handshake::server::Request, resp: tungstenite::handshake::server::Response| {
                            let path = req.uri().path();
                            if path == config.spectro_path {
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
