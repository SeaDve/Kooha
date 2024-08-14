use anyhow::{anyhow, ensure, Context, Result};
use gettextrs::gettext;
use gst::prelude::*;

use crate::help::ContextWithHelp;

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
    let provider =
        gst::DeviceProviderFactory::by_name("pulsedeviceprovider").with_context(|| {
            ContextWithHelp::new(
                gettext("Failed to find the default audio device"),
                gettext("Make sure that you have PulseAudio installed in your system."),
            )
        })?;

    provider.start()?;
    let devices = provider.devices();
    provider.stop();

    tracing::debug!("Finding device name for class `{:?}`", class);

    for device in devices {
        if let Err(err) = validate_device(&device, class) {
            tracing::debug!("Skipping device `{}`: {:?}", device.name(), err);
            continue;
        }

        return Ok(device);
    }

    Err(anyhow!("Failed to find a default device"))
}

fn validate_device(device: &gst::Device, class: DeviceClass) -> Result<()> {
    ensure!(
        device.has_classes(class.as_str()),
        "Unknown device class `{}`",
        device.device_class()
    );

    let is_default = device
        .properties()
        .context("No properties")?
        .get::<bool>("is-default")
        .context("No `is-default` property")?;

    ensure!(is_default, "Not the default device");

    Ok(())
}
