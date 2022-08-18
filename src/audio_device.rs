use anyhow::{anyhow, Context, Error, Result};
use gettextrs::gettext;
use gst::prelude::*;

use crate::{help::ResultExt, THREAD_POOL};

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

pub async fn find_default_name(class: Class) -> Result<String> {
    THREAD_POOL
        .push_future(move || find_default_name_inner(class))
        .context("Failed to push future to main thread pool")?
        .await
}

fn find_default_name_inner(class: Class) -> Result<String> {
    let device_monitor = gst::DeviceMonitor::new();
    device_monitor.add_filter(Some(class.as_str()), None);

    device_monitor.start().map_err(Error::from).with_help(
        || gettext("Make sure that you have PulseAudio installed in your system."),
        || gettext("Failed to start device monitor"),
    )?;
    let devices = device_monitor.devices();
    device_monitor.stop();

    tracing::debug!("Finding device name for class `{:?}`", class);

    for device in devices {
        let device_class = match Class::for_str(&device.device_class()) {
            Some(device_class) => device_class,
            None => {
                tracing::debug!(
                    "Skipping device `{}` as it has unknown device class `{}`",
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
                tracing::warn!(
                    "Skipping device `{}` as it has no properties",
                    device.name()
                );
                continue;
            }
        };

        let is_default = match properties.get::<bool>("is-default") {
            Ok(is_default) => is_default,
            Err(err) => {
                tracing::warn!(
                    "Skipping device `{}` as it has no `is-default` property. {:?}",
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

        let mut node_name = match properties.get::<String>("node.name") {
            Ok(node_name) => node_name,
            Err(error) => {
                tracing::warn!(
                    "Skipping device `{}` as it has no node.name property. {:?}",
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

    Err(anyhow!("Failed to find a default device"))
}
