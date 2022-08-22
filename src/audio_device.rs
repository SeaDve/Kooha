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
    match THREAD_POOL
        .push_future(move || find_default_name_gst(class))
        .context("Failed to push future to main thread pool")?
        .await
    {
        Ok(res) => Ok(res),
        Err(err) => {
            tracing::warn!("Failed to find default name using gstreamer: {:?}", err);
            tracing::debug!("Manually using libpulse instead");

            let pa_context = pa::Context::connect().await?;
            pa_context.find_default_device_name(class).await
        }
    }
}

fn find_default_name_gst(class: Class) -> Result<String> {
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

mod pa {
    use anyhow::{bail, Context as ErrContext, Error, Result};
    use futures_channel::{mpsc, oneshot};
    use futures_util::{
        future::{self, Either},
        StreamExt,
    };
    use gettextrs::gettext;
    use gtk::glib;
    use pulse::{
        context::{Context as ContextInner, FlagSet, State},
        def::Retval,
        mainloop::api::Mainloop,
        proplist::{properties, Proplist},
    };

    use std::{cell::RefCell, fmt, time::Duration};

    use super::Class;
    use crate::{config::APP_ID, help::ResultExt};

    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

    pub struct Context {
        inner: ContextInner,
        main_loop: pulse_glib::Mainloop,
    }

    impl fmt::Debug for Context {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("Server")
        }
    }

    impl Context {
        pub async fn connect() -> Result<Self> {
            let main_loop =
                pulse_glib::Mainloop::new(None).context("Failed to create pulse Mainloop")?;

            let mut proplist = Proplist::new().unwrap();
            proplist
                .set_str(properties::APPLICATION_ID, APP_ID)
                .unwrap();
            proplist
                .set_str(properties::APPLICATION_NAME, "Kooha")
                .unwrap();

            let mut context = ContextInner::new_with_proplist(&main_loop, APP_ID, &proplist)
                .context("Failed to create pulse Context")?;

            context
                .connect(None, FlagSet::NOFLAGS, None)
                .map_err(Error::from)
                .with_help(
                    || gettext("Make sure that you have PulseAudio installed in your system."),
                    || gettext("Failed to connect to PulseAudio daemon"),
                )?;

            let (mut tx, mut rx) = mpsc::channel(1);

            context.set_state_callback(Some(Box::new(move || {
                let _ = tx.start_send(());
            })));

            tracing::debug!("Waiting for PA server connection");

            while rx.next().await.is_some() {
                match context.get_state() {
                    State::Ready => break,
                    State::Failed => bail!("Connection failed or disconnected"),
                    State::Terminated => bail!("Connection context terminated"),
                    _ => {}
                }
            }

            tracing::debug!("PA Server connected");

            context.set_state_callback(None);

            Ok(Self {
                inner: context,
                main_loop,
            })
        }

        pub async fn find_default_device_name(&self, class: Class) -> Result<String> {
            let (tx, rx) = oneshot::channel();
            let tx = RefCell::new(Some(tx));

            let mut operation = self.inner.introspect().get_server_info(move |server_info| {
                let tx = if let Some(tx) = tx.take() {
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

            let name = match future::select(rx, glib::timeout_future(DEFAULT_TIMEOUT)).await {
                Either::Left((name, _)) => name,
                Either::Right(_) => {
                    operation.cancel();
                    bail!("get_server_info operation timeout reached");
                }
            };

            name.unwrap().context("Found no default device")
        }
    }

    impl Drop for Context {
        fn drop(&mut self) {
            self.inner.disconnect();
            self.main_loop.quit(Retval(0));
        }
    }
}
