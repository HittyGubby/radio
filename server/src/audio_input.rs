use crate::config::Config;
use crate::pipewire_devices::{find_device_by_id, find_device_by_name, list_audio_devices};
use anyhow::{Context, Result};
use log::{error, info};
use pipewire as pw;
use pipewire::properties::properties;
use std::sync::Arc;

pub struct PipeWireCapture {
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for PipeWireCapture {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub async fn start_audio_capture(
    config: &Config,
    audio_tx: tokio::sync::broadcast::Sender<Vec<f32>>,
) -> Result<PipeWireCapture> {
    let devices = list_audio_devices()
        .await
        .context("Failed to list audio devices")?;

    let target_device = if let Some(device_name) = &config.device_name {
        find_device_by_name(&devices, device_name)
            .ok_or_else(|| anyhow::anyhow!("Device '{}' not found", device_name))?
            .clone()
    } else if let Some(device_id) = config.device_id {
        find_device_by_id(&devices, device_id)
            .ok_or_else(|| anyhow::anyhow!("Device with ID {} not found", device_id))?
            .clone()
    } else {
        return Err(anyhow::anyhow!("No device specified"));
    };

    info!(
        "Target device selected: ID={}, name={}, description={}",
        target_device.id, target_device.name, target_device.description
    );

    let config = config.clone();
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    tokio::task::spawn_blocking(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pw::init();
            let mainloop =
                pw::main_loop::MainLoop::new(None).expect("Failed to create PipeWire main loop");

            let context =
                pw::context::Context::new(&mainloop).expect("Failed to create PipeWire context");

            let core = context
                .connect(None)
                .expect("Failed to connect to PipeWire");

            let registry = core.get_registry().expect("Failed to get registry");

            let _registry_listener = registry
                .add_listener_local()
                .global(|_global| {})
                .register();

            let stream_props = properties! {
                *pw::keys::MEDIA_TYPE => "Audio",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Music",
                *pw::keys::NODE_NAME => "radio-capture",
                *pw::keys::NODE_AUTOCONNECT => "true",
                *pw::keys::AUDIO_CHANNELS => "2",
            };

            let stream = pw::stream::Stream::new(&core, "radio-capture", stream_props)
                .expect("Failed to create PipeWire stream");

            let audio_tx = Arc::new(audio_tx);
            let audio_tx_clone = Arc::clone(&audio_tx);

            let samples_written = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let samples_written_clone = Arc::clone(&samples_written);

            let _stream_listener = stream
                .add_local_listener_with_user_data::<()>(())
                .param_changed(|_stream, _user_data, id, param| {
                    if id == pw::spa::param::ParamType::Format.as_raw() {
                        if let Some(p) = param {
                            if let Ok((media_type, media_subtype)) =
                                pw::spa::param::format_utils::parse_format(p)
                            {
                                info!(
                                    "Format negotiated: media_type={:?}, media_subtype={:?}",
                                    media_type, media_subtype
                                );

                                let mut audio_info = pw::spa::param::audio::AudioInfoRaw::new();
                                let mut pos = [0u32; 64];
                                pos[0] = spa::sys::SPA_AUDIO_CHANNEL_FL as u32;
                                pos[1] = spa::sys::SPA_AUDIO_CHANNEL_FR as u32;
                                audio_info.set_channels(2);
                                audio_info.set_position(pos);
                                if audio_info.parse(p).is_ok() {
                                    info!(
                                        "Audio format: rate={}, channels={}, format={:?}",
                                        audio_info.rate(),
                                        audio_info.channels(),
                                        audio_info.format()
                                    );
                                }
                            }
                        }
                    }
                })
                .process(move |stream, _user_data| {
                    if let Some(mut buffer) = stream.dequeue_buffer() {
                        let datas = buffer.datas_mut();
                        if !datas.is_empty() {
                            let data = &mut datas[0];
                            let chunk = data.chunk();
                            let size = chunk.size();

                            if let Some(data_bytes) = data.data() {
                                let samples = size / 4;
                                if samples > 0 && data_bytes.len() >= size as usize {
                                    let sample_slice: &[f32] = unsafe {
                                        std::slice::from_raw_parts(
                                            data_bytes.as_ptr() as *const f32,
                                            samples as usize,
                                        )
                                    };

                                    // Convert stereo to mono by averaging channels
                                    // Samples are interleaved as [L, R, L, R, ...]
                                    let mono_samples: Vec<f32> = if config.channels == 2 {
                                        sample_slice
                                            .chunks(2)
                                            .map(|pair| (pair[0] + pair[1]) / 2.0)
                                            .collect()
                                    } else {
                                        sample_slice.to_vec()
                                    };

                                    let _ = audio_tx_clone.send(mono_samples);
                                    samples_written_clone.fetch_add(
                                        samples as u64,
                                        std::sync::atomic::Ordering::Relaxed,
                                    );
                                }
                            }
                        }
                    }
                })
                .register();

            let mut audio_info = pw::spa::param::audio::AudioInfoRaw::new();
            audio_info.set_format(pw::spa::param::audio::AudioFormat::F32LE);
            audio_info.set_rate(config.get_input_sample_rate());
            audio_info.set_channels(config.channels as u32);

            // Set channel positions based on configuration
            let mut pos = [0u32; 64];
            if config.channels == 2 {
                // Stereo
                pos[0] = spa::sys::SPA_AUDIO_CHANNEL_FL as u32;
                pos[1] = spa::sys::SPA_AUDIO_CHANNEL_FR as u32;
            } else {
                // Mono
                pos[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO as u32;
            }
            audio_info.set_position(pos);

            let obj = pw::spa::pod::Object {
                type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
                id: pw::spa::param::ParamType::EnumFormat.as_raw(),
                properties: audio_info.into(),
            };

            let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &pw::spa::pod::Value::Object(obj),
            )
            .unwrap()
            .0
            .into_inner();

            let mut params = [pw::spa::pod::Pod::from_bytes(&values).unwrap()];

            let flags = pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS;

            stream
                .connect(
                    pw::spa::utils::Direction::Input,
                    Some(target_device.id),
                    flags,
                    &mut params,
                )
                .expect("Failed to connect PipeWire stream");
            info!("Stream connected to node {}", target_device.id);

            stream
                .set_active(true)
                .expect("Failed to activate PipeWire stream");

            let mut last_time = std::time::Instant::now();

            loop {
                mainloop
                    .loop_()
                    .iterate(std::time::Duration::from_millis(100).into());

                if last_time.elapsed() >= std::time::Duration::from_secs(3) {
                    last_time = std::time::Instant::now();
                }

                if shutdown_clone.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }
            info!("Main loop stopped");
        }));

        if let Err(err) = result {
            error!("PipeWire capture thread panicked: {:?}", err);
        }
    });

    Ok(PipeWireCapture { shutdown })
}
