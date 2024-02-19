use anyhow::{anyhow, Result};
use gettextrs::gettext;
use gst::prelude::*;

use crate::help::ResultExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    Source,
    Sink,
}

impl DeviceClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "Audio/Source",
            Self::Sink => "Audio/Sink",
        }
    }
}

pub fn find_default(class: DeviceClass) -> Result<gst::Device> {
    let provider = gst::DeviceProviderFactory::by_name("pulsedeviceprovider").with_help(
        || gettext("Make sure that you have PulseAudio installed in your system."),
        || gettext("No pulseaudio device provider found"),
    )?;

    provider.start()?;
    let devices = provider.devices();
    provider.stop();

    tracing::debug!("Finding device name for class `{:?}`", class);

    for device in devices {
        if !device.has_classes(class.as_str()) {
            tracing::debug!(
                "Skipping device `{}` as it has unknown device class `{}`",
                device.name(),
                device.device_class()
            );
            continue;
        }

        let Some(properties) = device.properties() else {
            tracing::warn!(
                "Skipping device `{}` as it has no properties",
                device.name()
            );
            continue;
        };

        let is_default = match properties.get::<bool>("is-default") {
            Ok(is_default) => is_default,
            Err(err) => {
                tracing::warn!(
                    "Skipping device `{}` as it has no `is-default` property: {:?}",
                    device.name(),
                    err
                );
                continue;
            }
        };

        if !is_default {
            tracing::debug!(
                "Skipping device `{}` as it is not the default",
                device.name()
            );
            continue;
        }

        return Ok(device);
    }

    Err(anyhow!("Failed to find a default device"))
}
