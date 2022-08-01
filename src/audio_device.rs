use gst::prelude::*;

use crate::THREAD_POOL;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    #[default]
    Source,
    Sink,
}

impl Class {
    fn for_str(string: &str) -> anyhow::Result<Self> {
        match string {
            "Audio/Source" => Ok(Self::Source),
            "Audio/Sink" => Ok(Self::Sink),
            unknown => Err(anyhow::anyhow!("Unknown device class `{unknown}`")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "Audio/Source",
            Self::Sink => "Audio/Sink",
        }
    }
}

pub async fn find_default_name(class: Class) -> anyhow::Result<String> {
    THREAD_POOL
        .push_future(move || find_default_name_inner(class))?
        .await
}

fn find_default_name_inner(class: Class) -> anyhow::Result<String> {
    let device_monitor = gst::DeviceMonitor::new();
    device_monitor.add_filter(Some(class.as_str()), None);

    device_monitor.start()?;
    let devices = device_monitor.devices();
    device_monitor.stop();

    log::info!("Finding device name for class `{:?}`", class);

    for device in devices {
        let device_class = Class::for_str(&device.device_class())?;

        if device_class == class {
            let properties = device
                .properties()
                .ok_or_else(|| anyhow::anyhow!("No properties found for device"))?;

            if properties.get::<bool>("is-default")? {
                let mut node_name = properties.get::<String>("node.name")?;

                if device_class == Class::Sink {
                    node_name.push_str(".monitor");
                }

                return Ok(node_name);
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to find audio device for class `{:?}`",
        class
    ))
}
