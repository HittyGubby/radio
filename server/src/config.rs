use clap::Parser;
use serde::Serialize;

#[derive(Parser, Debug, Clone, Serialize)]
#[command(name = "radio-back")]
#[command(about = "Personal radio streaming backend server", long_about = None)]
pub struct Config {
    /// List available PipeWire audio capture devices
    #[arg(long)]
    pub list_devices: bool,

    /// Audio input device name (node.name in PipeWire)
    #[arg(long, conflicts_with = "device_id")]
    pub device_name: Option<String>,

    /// Audio input device ID (object.id in PipeWire)
    #[arg(long)]
    pub device_id: Option<u32>,

    /// Sample rate in Hz
    #[arg(long, default_value = "48000")]
    pub sample_rate: u32,

    /// Number of audio channels
    #[arg(long, default_value = "1")]
    pub channels: u16,

    /// Ring buffer duration in seconds
    #[arg(long, default_value = "2")]
    pub ring_buffer_seconds: usize,

    /// Audio codec: opus or pcm
    #[arg(long, default_value = "opus")]
    pub audio_codec: String,

    /// Audio bitrate in bps
    #[arg(long, default_value = "48000")]
    pub audio_bitrate: u32,

    /// Audio bit depth
    #[arg(long, default_value = "16")]
    pub audio_bitdepth: u16,

    /// Audio frame duration in milliseconds
    #[arg(long, default_value = "20")]
    pub audio_frame_ms: usize,

    /// WebSocket bind address
    #[arg(long, default_value = "[::]:23331")]
    pub ws_bind: String,

    /// WebSocket audio endpoint path
    #[arg(long, default_value = "/audio")]
    pub audio_path: String,

    /// Config endpoint path
    #[arg(long, default_value = "/config")]
    pub config_path: String,

    /// Maximum number of connected clients
    #[arg(long, default_value = "200")]
    pub max_clients: usize,

    /// Client jitter buffer size in milliseconds
    #[arg(long, default_value = "200")]
    pub jitter_buffer_ms: usize,
}

impl Config {
    pub fn audio_frame_samples(&self) -> usize {
        (self.sample_rate as usize * self.audio_frame_ms) / 1000
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.audio_codec != "opus" && self.audio_codec != "pcm" {
            anyhow::bail!("Audio codec must be 'opus' or 'pcm'");
        }

        if self.audio_path == self.config_path {
            anyhow::bail!("WebSocket endpoint paths must be different");
        }

        // Validate device selection
        if !self.list_devices {
            match (&self.device_name, &self.device_id) {
                (None, None) => {
                    anyhow::bail!("Either --device-name or --device-id must be specified")
                }
                (Some(_), Some(_)) => {
                    anyhow::bail!("Only one of --device-name or --device-id can be specified")
                }
                _ => anyhow::Ok(()),
            }?;
        }

        Ok(())
    }
}