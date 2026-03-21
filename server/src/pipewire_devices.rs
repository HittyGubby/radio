use anyhow::{Context, Result};
use pipewire as pw;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: u32,
    pub name: String,
    pub description: String,
}

pub async fn list_audio_devices() -> Result<Vec<AudioDevice>> {
    let devices = tokio::task::spawn_blocking(move || enumerate_devices())
        .await
        .context("Failed to enumerate devices")??;

    Ok(devices)
}

pub fn enumerate_devices() -> Result<Vec<AudioDevice>> {
    pw::init();

    let mainloop =
        pw::main_loop::MainLoop::new(None).context("Failed to create PipeWire main loop")?;
    let context =
        pw::context::Context::new(&mainloop).context("Failed to create PipeWire context")?;
    let core = context
        .connect(None)
        .context("Failed to connect to PipeWire")?;
    let registry = core.get_registry().context("Failed to get registry")?;

    let devices = Arc::new(Mutex::new(Vec::new()));
    let devices_clone = devices.clone();

    // Keep listener alive
    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            // Only Node objects
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }

            let props = match &global.props {
                Some(p) => p,
                None => return,
            };

            let id = global.id;
            let name = props
                .get("node.name")
                .unwrap_or(&format!("node_{}", id))
                .to_string();
            let description = props.get("node.description").unwrap_or(&name).to_string();

            let device = AudioDevice {
                id,
                name,
                description,
            };

            devices_clone.lock().unwrap().push(device);
        })
        .register();

    // Run mainloop briefly to let callbacks fire
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(500);

    while start.elapsed() < timeout {
        mainloop
            .loop_()
            .iterate(std::time::Duration::from_millis(10)); // iterate 10ms
    }

    let result = devices.lock().unwrap().clone();
    Ok(result)
}

pub fn find_device_by_name<'a>(devices: &'a [AudioDevice], name: &str) -> Option<&'a AudioDevice> {
    devices.iter().find(|d| d.name == name)
}

pub fn find_device_by_id<'a>(devices: &'a [AudioDevice], id: u32) -> Option<&'a AudioDevice> {
    devices.iter().find(|d| d.id == id)
}
