use error_stack::{IntoReport, Report, Result, ResultExt};
use gettextrs::gettext;
use gst::prelude::*;

use crate::{help::ResultExt as HelpResultExt, THREAD_POOL};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    #[default]
    Source,
    Sink,
}

impl Class {
    fn for_str(string: &str) -> Option<Self> {
        match string {
            "Audio/Source" => Some(Self::Source),
            "Audio/Sink" => Some(Self::Sink),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "Audio/Source",
            Self::Sink => "Audio/Sink",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Default device name find error")]
pub struct FindDefaultDeviceNameError;

pub async fn find_default_name(class: Class) -> Result<String, FindDefaultDeviceNameError> {
    THREAD_POOL
        .push_future(move || find_default_name_inner(class))
        .report()
        .change_context(FindDefaultDeviceNameError)
        .attach_printable("failed to push future to main thread pool")?
        .await
}

fn find_default_name_inner(class: Class) -> Result<String, FindDefaultDeviceNameError> {
    let device_monitor = gst::DeviceMonitor::new();
    device_monitor.add_filter(Some(class.as_str()), None);

    device_monitor
        .start()
        .report()
        .change_context(FindDefaultDeviceNameError)
        .attach_printable("failed to start device monitor")
        .attach_help_lazy(|| gettext("Make sure that you have pulseaudio in your system."))?;
    let devices = device_monitor.devices();
    device_monitor.stop();

    tracing::info!("Finding device name for class `{:?}`", class);

    for device in devices {
        let device_class = match Class::for_str(&device.device_class()) {
            Some(device_class) => device_class,
            None => {
                tracing::info!(
                    "Skipping device `{}` because it has unknown device class `{}`",
                    device.name(),
                    device.device_class()
                );
                continue;
            }
        };

        if device_class != class {
            continue;
        }

        let properties = match device.properties() {
            Some(properties) => properties,
            None => {
                tracing::warn!("Device `{}` somehow has no properties", device.name());
                continue;
            }
        };

        let is_default = match properties.get::<bool>("is-default") {
            Ok(is_default) => is_default,
            Err(err) => {
                tracing::warn!(
                    "Device `{}` somehow has no properties. {:?}",
                    device.name(),
                    err
                );
                continue;
            }
        };

        if !is_default {
            continue;
        }

        let mut node_name = match properties.get::<String>("node.name") {
            Ok(node_name) => node_name,
            Err(error) => {
                tracing::warn!(
                    "Device `{}` has no node.name property. {:?}",
                    device.name(),
                    error
                );
                continue;
            }
        };

        if device_class == Class::Sink {
            node_name.push_str(".monitor");
        }

        return Ok(node_name);
    }

    Err(Report::new(FindDefaultDeviceNameError)).attach_printable("failed to get a match")
}
