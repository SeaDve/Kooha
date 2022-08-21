use anyhow::{anyhow, bail, Context, Error, Result};
use futures_channel::oneshot;
use gettextrs::gettext;
use gtk::glib::{self, clone};

use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::{config::APP_ID, help::ResultExt, utils, THREAD_POOL};

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
    match THREAD_POOL
        .push_future(move || find_default_name_gst(class))
        .context("Failed to push future to main thread pool")?
        .await
    {
        Ok(res) => Ok(res),
        Err(err) => {
            tracing::warn!("Failed to find default name using gstreamer: {:?}", err);
            tracing::debug!("Falling back with pulse");
            find_default_name_pulse(class).await
        }
    }
}

fn find_default_name_gst(class: Class) -> Result<String> {
    use gst::prelude::*;

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
            Err(err) => {
                tracing::warn!(
                    "Skipping device `{}` as it has no node.name property. {:?}",
                    device.name(),
                    err
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

const DEFAULT_PULSE_TIMEOUT: Duration = Duration::from_secs(2);

async fn find_default_name_pulse(class: Class) -> Result<String> {
    use pulse::{
        context::{Context, FlagSet, State},
        proplist::{properties, Proplist},
    };
    use pulse_glib::Mainloop;

    let mainloop = Mainloop::new(None).context("Failed to create pulse Mainloop")?;

    let mut proplist = Proplist::new().unwrap();
    proplist
        .set_str(properties::APPLICATION_ID, APP_ID)
        .unwrap();
    proplist
        .set_str(properties::APPLICATION_NAME, "Kooha")
        .unwrap();

    let mut context = Context::new_with_proplist(&mainloop, APP_ID, &proplist)
        .context("Failed to create pulse Context")?;

    context
        .connect(None, FlagSet::NOFLAGS, None)
        .map_err(Error::from)
        .with_help(
            || gettext("Make sure that you have PulseAudio installed in your system."),
            || gettext("Failed to connect to PulseAudio daemon"),
        )?;

    let context = Rc::new(RefCell::new(context));

    let (state_tx, state_rx) = oneshot::channel();
    let state_tx = RefCell::new(Some(state_tx));

    context
        .borrow_mut()
        .set_state_callback(Some(Box::new(clone!(@weak context => move  || {
            match context.borrow().get_state() {
                State::Ready => {
                    if let Some(tx) = state_tx.take() {
                        let _ = tx.send(Ok(()));
                    } else {
                        tracing::error!("Received ready state twice!");
                    }
                }
                State::Failed => {
                    if let Some(tx) = state_tx.take() {
                        let _ = tx.send(Err(anyhow!("Received failed state on context")));
                    } else {
                        tracing::error!("Received failed state twice!");
                    }
                }
                State::Terminated => {
                    if let Some(tx) = state_tx.take() {
                        let _ = tx.send(Err(anyhow!("Context connection terminated")));
                    } else {
                        tracing::error!("Received failed state twice!");
                    }
                }
                _ => {}
            };
        }))));

    utils::future_timeout(state_rx, DEFAULT_PULSE_TIMEOUT)
        .await
        .context("Waiting context ready timeout")?
        .unwrap()?;

    let (operation_tx, operation_rx) = oneshot::channel();
    let operation_tx = RefCell::new(Some(operation_tx));

    let mut operation = context
        .borrow()
        .introspect()
        .get_server_info(move |server_info| {
            let tx = if let Some(tx) = operation_tx.take() {
                tx
            } else {
                tracing::error!("Called get_server_info twice!");
                return;
            };

            match class {
                Class::Source => {
                    let _ = tx.send(
                        server_info
                            .default_source_name
                            .as_ref()
                            .map(|s| s.to_string()),
                    );
                }
                Class::Sink => {
                    let _ = tx.send(
                        server_info
                            .default_sink_name
                            .as_ref()
                            .map(|s| format!("{}.monitor", s)),
                    );
                }
            }
        });

    let name = match utils::future_timeout(operation_rx, DEFAULT_PULSE_TIMEOUT).await {
        Ok(name) => name.unwrap().context("Found no default device")?,
        Err(err) => {
            operation.cancel();
            bail!("Failed to receive get_server_info result: {:?}", err)
        }
    };

    Ok(name)
}
