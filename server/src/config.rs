use clap::Parser;
use serde::{Deserialize, Serialize};

/// Config that can be loaded from TOML file (excludes CLI-only fields)
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ConfigFile {
    #[serde(default)]
    device_name: Option<String>,
    #[serde(default)]
    device_id: Option<u32>,
    #[serde(default = "default_input_sample_rate")]
    input_sample_rate: u32,
    #[serde(default = "default_output_sample_rate")]
    output_sample_rate: u32,
    #[serde(default = "default_channels")]
    channels: u16,
    #[serde(default = "default_ring_buffer_seconds")]
    ring_buffer_seconds: usize,
    #[serde(default = "default_audio_codec")]
    audio_codec: String,
    #[serde(default = "default_audio_bitrate")]
    audio_bitrate: u32,
    #[serde(default = "default_audio_bitdepth")]
    audio_bitdepth: u16,
    #[serde(default = "default_audio_frame_ms")]
    audio_frame_ms: usize,
    #[serde(default = "default_ws_bind")]
    ws_bind: String,
    #[serde(default = "default_audio_path")]
    audio_path: String,
    #[serde(default = "default_config_path")]
    config_path: String,
    #[serde(default = "default_max_clients")]
    max_clients: usize,
    #[serde(default = "default_jitter_buffer_ms")]
    jitter_buffer_ms: usize,
    #[serde(default = "default_spectro_enabled")]
    spectro_enabled: bool,
    #[serde(default = "default_spectro_path")]
    spectro_path: String,
    #[serde(default = "default_spectro_fft_size")]
    spectro_fft_size: usize,
    #[serde(default = "default_spectro_max_freq")]
    spectro_max_freq: u32,
    #[serde(default = "default_spectro_window")]
    spectro_window: String,
    #[serde(default = "default_spectro_smoothing")]
    spectro_smoothing: f32,
    #[serde(default = "default_spectro_min_db")]
    spectro_min_db: f32,
    #[serde(default = "default_spectro_max_db")]
    spectro_max_db: f32,
    #[serde(default = "default_spectro_frame_ms")]
    spectro_frame_ms: usize,
}

// Default functions for ConfigFile
fn default_input_sample_rate() -> u32 { 48000 }
fn default_output_sample_rate() -> u32 { 12000 }
fn default_channels() -> u16 { 1 }
fn default_ring_buffer_seconds() -> usize { 10 }
fn default_audio_codec() -> String { "opus".to_string() }
fn default_audio_bitrate() -> u32 { 24000 }
fn default_audio_bitdepth() -> u16 { 16 }
fn default_audio_frame_ms() -> usize { 50 }
fn default_ws_bind() -> String { "[::]:23331".to_string() }
fn default_audio_path() -> String { "/audio".to_string() }
fn default_config_path() -> String { "/config".to_string() }
fn default_max_clients() -> usize { 200 }
fn default_jitter_buffer_ms() -> usize { 500 }
fn default_spectro_enabled() -> bool { true }
fn default_spectro_path() -> String { "/spectro".to_string() }
fn default_spectro_fft_size() -> usize { 8192 }
fn default_spectro_max_freq() -> u32 { 16000 }
fn default_spectro_window() -> String { "hann".to_string() }
fn default_spectro_smoothing() -> f32 { 0.0 }
fn default_spectro_min_db() -> f32 { -100.0 }
fn default_spectro_max_db() -> f32 { 0.0 }
fn default_spectro_frame_ms() -> usize { 50 }

impl From<ConfigFile> for Config {
    fn from(file: ConfigFile) -> Self {
        Self {
            list_devices: false,
            device_name: file.device_name,
            device_id: file.device_id,
            input_sample_rate: file.input_sample_rate,
            output_sample_rate: file.output_sample_rate,
            channels: file.channels,
            ring_buffer_seconds: file.ring_buffer_seconds,
            audio_codec: file.audio_codec,
            audio_bitrate: file.audio_bitrate,
            audio_bitdepth: file.audio_bitdepth,
            audio_frame_ms: file.audio_frame_ms,
            ws_bind: file.ws_bind,
            audio_path: file.audio_path,
            config_path: file.config_path,
            max_clients: file.max_clients,
            jitter_buffer_ms: file.jitter_buffer_ms,
            spectro_enabled: file.spectro_enabled,
            spectro_path: file.spectro_path,
            spectro_fft_size: file.spectro_fft_size,
            spectro_max_freq: file.spectro_max_freq,
            spectro_window: file.spectro_window,
            spectro_smoothing: file.spectro_smoothing,
            spectro_min_db: file.spectro_min_db,
            spectro_max_db: file.spectro_max_db,
            spectro_frame_ms: file.spectro_frame_ms,
            config_file: None,
        }
    }
}

#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
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

    /// Input sample rate in Hz (for audio capture and spectrogram)
    #[arg(long, default_value = "48000")]
    pub input_sample_rate: u32,

    /// Output sample rate in Hz (for audio encoding)
    #[arg(long, default_value = "12000")]
    pub output_sample_rate: u32,

    /// Number of audio channels
    #[arg(long, default_value = "1")]
    pub channels: u16,

    /// Ring buffer duration in seconds
    #[arg(long, default_value = "10")]
    pub ring_buffer_seconds: usize,

    /// Audio codec: opus or pcm
    #[arg(long, default_value = "opus")]
    pub audio_codec: String,

    /// Audio bitrate in bps
    #[arg(long, default_value = "24000")]
    pub audio_bitrate: u32,

    /// Audio bit depth
    #[arg(long, default_value = "16")]
    pub audio_bitdepth: u16,

    /// Audio frame duration in milliseconds
    #[arg(long, default_value = "50")]
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
    #[arg(long, default_value = "500")]
    pub jitter_buffer_ms: usize,

    /// Enable spectrogram WebSocket
    #[arg(long, default_value = "true")]
    pub spectro_enabled: bool,

    /// Spectrogram WebSocket endpoint path
    #[arg(long, default_value = "/spectro")]
    pub spectro_path: String,

    /// Spectrogram FFT size
    #[arg(long, default_value = "8192")]
    pub spectro_fft_size: usize,

    /// Spectrogram maximum frequency in Hz
    #[arg(long, default_value = "16000")]
    pub spectro_max_freq: u32,

    /// Spectrogram window function (hann, hamming, blackman)
    #[arg(long, default_value = "hann")]
    pub spectro_window: String,

    /// Spectrogram smoothing time constant
    #[arg(long, default_value = "0")]
    pub spectro_smoothing: f32,

    /// Spectrogram minimum decibels
    #[arg(long, default_value = "-100")]
    pub spectro_min_db: f32,

    /// Spectrogram maximum decibels
    #[arg(long, default_value = "0")]
    pub spectro_max_db: f32,

    /// Spectrogram frame duration in milliseconds
    #[arg(long, default_value = "50")]
    pub spectro_frame_ms: usize,

    /// Config file path (optional)
    #[arg(long)]
    pub config_file: Option<String>,
}

impl Config {
    pub fn audio_frame_samples(&self) -> usize {
        (self.output_sample_rate as usize * self.audio_frame_ms) / 1000
    }

    pub fn spectro_frame_samples(&self) -> usize {
        (self.input_sample_rate as usize * self.spectro_frame_ms) / 1000
    }

    /// Get the effective input sample rate (for capture and spectrogram)
    pub fn get_input_sample_rate(&self) -> u32 {
        self.input_sample_rate
    }

    /// Get the effective output sample rate (for encoding)
    pub fn get_output_sample_rate(&self) -> u32 {
        self.output_sample_rate
    }

    pub fn get_config_file_path() -> Option<std::path::PathBuf> {
        directories::ProjectDirs::from("com", "radio", "radio")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config_file: ConfigFile = toml::from_str(&contents)?;
        Ok(Config::from(config_file))
    }

    pub fn load() -> Self {
        // First, try to load from config file
        let mut config = if let Some(config_path) = Self::get_config_file_path() {
            if config_path.exists() {
                log::info!("Loading config from: {:?}", config_path);
                match Self::load_from_file(&config_path) {
                    Ok(loaded_config) => {
                        log::info!("Successfully loaded config file - device_name: {:?}, device_id: {:?}",
                            loaded_config.device_name, loaded_config.device_id);
                        loaded_config
                    }
                    Err(e) => {
                        log::warn!("Failed to load config file: {}, using defaults", e);
                        Self::default_config()
                    }
                }
            } else {
                log::warn!("Config file does not exist at: {:?}, using defaults", config_path);
                Self::default_config()
            }
        } else {
            log::warn!("Could not determine config file path, using defaults");
            Self::default_config()
        };

        // Then parse CLI args to override config file values
        let cli_args = Self::parse();

        log::info!("CLI args parsed - device_name: {:?}, device_id: {:?}",
            cli_args.device_name, cli_args.device_id);

        // Override with CLI args (only non-None values)
        if cli_args.list_devices {
            config.list_devices = true;
        }
        if cli_args.device_name.is_some() {
            log::info!("CLI overriding device_name from {:?} to {:?}", config.device_name, cli_args.device_name);
            config.device_name = cli_args.device_name;
        }
        if cli_args.device_id.is_some() {
            log::info!("CLI overriding device_id from {:?} to {:?}", config.device_id, cli_args.device_id);
            config.device_id = cli_args.device_id;
        }

        if cli_args.input_sample_rate != Self::default_config().input_sample_rate {
            config.input_sample_rate = cli_args.input_sample_rate;
        }
        if cli_args.output_sample_rate != Self::default_config().output_sample_rate {
            config.output_sample_rate = cli_args.output_sample_rate;
        }
        if cli_args.channels != Self::default_config().channels {
            config.channels = cli_args.channels;
        }
        if cli_args.ring_buffer_seconds != Self::default_config().ring_buffer_seconds {
            config.ring_buffer_seconds = cli_args.ring_buffer_seconds;
        }
        if cli_args.audio_codec != Self::default_config().audio_codec {
            config.audio_codec = cli_args.audio_codec;
        }
        if cli_args.audio_bitrate != Self::default_config().audio_bitrate {
            config.audio_bitrate = cli_args.audio_bitrate;
        }
        if cli_args.audio_bitdepth != Self::default_config().audio_bitdepth {
            config.audio_bitdepth = cli_args.audio_bitdepth;
        }
        if cli_args.audio_frame_ms != Self::default_config().audio_frame_ms {
            config.audio_frame_ms = cli_args.audio_frame_ms;
        }
        if cli_args.ws_bind != Self::default_config().ws_bind {
            config.ws_bind = cli_args.ws_bind;
        }
        if cli_args.audio_path != Self::default_config().audio_path {
            config.audio_path = cli_args.audio_path;
        }
        if cli_args.config_path != Self::default_config().config_path {
            config.config_path = cli_args.config_path;
        }
        if cli_args.max_clients != Self::default_config().max_clients {
            config.max_clients = cli_args.max_clients;
        }
        if cli_args.jitter_buffer_ms != Self::default_config().jitter_buffer_ms {
            config.jitter_buffer_ms = cli_args.jitter_buffer_ms;
        }
        if cli_args.spectro_enabled != Self::default_config().spectro_enabled {
            config.spectro_enabled = cli_args.spectro_enabled;
        }
        if cli_args.spectro_path != Self::default_config().spectro_path {
            config.spectro_path = cli_args.spectro_path;
        }
        if cli_args.spectro_fft_size != Self::default_config().spectro_fft_size {
            config.spectro_fft_size = cli_args.spectro_fft_size;
        }
        if cli_args.spectro_max_freq != Self::default_config().spectro_max_freq {
            config.spectro_max_freq = cli_args.spectro_max_freq;
        }
        if cli_args.spectro_window != Self::default_config().spectro_window {
            config.spectro_window = cli_args.spectro_window;
        }
        if cli_args.spectro_smoothing != Self::default_config().spectro_smoothing {
            config.spectro_smoothing = cli_args.spectro_smoothing;
        }
        if cli_args.spectro_min_db != Self::default_config().spectro_min_db {
            config.spectro_min_db = cli_args.spectro_min_db;
        }
        if cli_args.spectro_max_db != Self::default_config().spectro_max_db {
            config.spectro_max_db = cli_args.spectro_max_db;
        }
        if cli_args.spectro_frame_ms != Self::default_config().spectro_frame_ms {
            config.spectro_frame_ms = cli_args.spectro_frame_ms;
        }

        config
    }

    fn default_config() -> Self {
        // Create a default config by parsing empty args
        // This is a workaround since we can't use Parser on empty args directly
        Self {
            list_devices: false,
            device_name: None,
            device_id: None,
            input_sample_rate: 48000,
            output_sample_rate: 12000,
            channels: 1,
            ring_buffer_seconds: 10,
            audio_codec: "opus".to_string(),
            audio_bitrate: 24000,
            audio_bitdepth: 16,
            audio_frame_ms: 20,
            ws_bind: "[::]:23331".to_string(),
            audio_path: "/audio".to_string(),
            config_path: "/config".to_string(),
            max_clients: 200,
            jitter_buffer_ms: 500,
            spectro_enabled: true,
            spectro_path: "/spectro".to_string(),
            spectro_fft_size: 8192,
            spectro_max_freq: 16000,
            spectro_window: "hann".to_string(),
            spectro_smoothing: 0.0,
            spectro_min_db: -100.0,
            spectro_max_db: 0.0,
            spectro_frame_ms: 20,
            config_file: None,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.audio_codec != "opus" && self.audio_codec != "pcm" {
            anyhow::bail!("Audio codec must be 'opus' or 'pcm'");
        }

        if self.audio_path == self.config_path {
            anyhow::bail!("WebSocket endpoint paths must be different");
        }

        if self.spectro_enabled && self.spectro_path == self.audio_path {
            anyhow::bail!("Spectrogram endpoint path must be different from audio path");
        }

        // Validate spectrogram window function
        if self.spectro_window != "hann" && self.spectro_window != "hamming" && self.spectro_window != "blackman" {
            anyhow::bail!("Window function must be 'hann', 'hamming', or 'blackman'");
        }

        // Validate spectrogram FFT size
        if !self.spectro_fft_size.is_power_of_two() {
            anyhow::bail!("Spectrogram FFT size must be a power of 2");
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