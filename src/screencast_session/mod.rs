mod handle_token;
mod object_path;
mod types;
mod window_identifier;

use anyhow::{anyhow, ensure, Context, Result};
use futures_channel::oneshot;
use gtk::{gio, glib, prelude::*};

use std::{cell::RefCell, collections::HashMap, os::unix::io::RawFd, time::Duration};

pub use self::types::{CursorMode, PersistMode, SourceType, Stream};
use self::{
    handle_token::HandleToken, object_path::ObjectPath, window_identifier::WindowIdentifier,
};
use crate::cancelled::Cancelled;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct ScreencastSession {
    proxy: gio::DBusProxy,
    session_handle: ObjectPath,
}

impl ScreencastSession {
    pub async fn new() -> Result<Self> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.ScreenCast",
        )
        .await
        .context("Failed to create screencast proxy")?;

        let session_handle_token = HandleToken::new();
        let handle_token = HandleToken::new();

        let session_options = HashMap::from([
            ("handle_token", handle_token.to_variant()),
            ("session_handle_token", session_handle_token.to_variant()),
        ]);
        let response = screencast_request_call(
            &proxy,
            &handle_token,
            "CreateSession",
            &(session_options,).to_variant(),
        )
        .await
        .context("Failed to create session")?;

        tracing::info!("Created screencast session");

        let session_handle = response
            .get("session_handle")
            .ok_or_else(|| anyhow!("Expected session_handle"))?
            .get::<ObjectPath>()
            .ok_or_else(|| anyhow!("Expected session_handle of type o"))?;

        debug_assert!(session_handle
            .as_str()
            .ends_with(&session_handle_token.as_str()));

        Ok(Self {
            proxy,
            session_handle,
        })
    }

    pub async fn begin(
        &self,
        cursor_mode: CursorMode,
        source_type: SourceType,
        is_multiple_sources: bool,
        restore_token: Option<&str>,
        persist_mode: PersistMode,
        parent_window: Option<&impl IsA<gtk::Window>>,
    ) -> Result<(Vec<Stream>, Option<String>, RawFd)> {
        self.select_sources(
            source_type,
            is_multiple_sources,
            cursor_mode,
            restore_token.filter(|s| !s.is_empty()),
            persist_mode,
        )
        .await
        .context("Failed to select sources")?;

        let window_identifier = if let Some(window) = parent_window {
            WindowIdentifier::new(window.upcast_ref()).await
        } else {
            WindowIdentifier::None
        };

        let (streams, output_restore_token) = self
            .start(window_identifier)
            .await
            .context("Failed to start screencast session")?;

        let fd = self
            .open_pipe_wire_remote()
            .await
            .context("Failed to open pipe wire remote")?;

        Ok((streams, output_restore_token, fd))
    }

    pub async fn close(&self) -> Result<()> {
        let ret = self
            .proxy
            .connection()
            .call_future(
                Some("org.freedesktop.portal.Desktop"),
                self.session_handle.as_str(),
                "org.freedesktop.portal.Session",
                "Close",
                None,
                None,
                gio::DBusCallFlags::NONE,
                DEFAULT_TIMEOUT.as_millis() as i32,
            )
            .await
            .context("Failed to invoke Close on the session")?;

        debug_assert_eq!(ret.get(), Some(()));

        tracing::info!("Closed screencast session");

        Ok(())
    }

    pub async fn version(&self) -> Result<u32> {
        self.property("version")
    }

    pub async fn available_cursor_modes(&self) -> Result<CursorMode> {
        let value = self.property::<u32>("AvailableCursorModes")?;

        CursorMode::from_bits(value).ok_or_else(|| anyhow!("Invalid cursor mode: {}", value))
    }

    pub async fn available_source_types(&self) -> Result<SourceType> {
        let value = self.property::<u32>("AvailableSourceTypes")?;

        SourceType::from_bits(value).ok_or_else(|| anyhow!("Invalid source type: {}", value))
    }

    fn property<T: glib::FromVariant>(&self, name: &str) -> Result<T> {
        let variant = self
            .proxy
            .cached_property(name)
            .ok_or_else(|| anyhow!("No cached {} property", name))?;

        variant.get::<T>().ok_or_else(|| {
            anyhow!(
                "Expected {} type. Got {}",
                T::static_variant_type(),
                variant.type_()
            )
        })
    }

    async fn start(
        &self,
        window_identifier: WindowIdentifier,
    ) -> Result<(Vec<Stream>, Option<String>)> {
        let handle_token = HandleToken::new();

        let options = HashMap::from([("handle_token", handle_token.to_variant())]);

        let response = screencast_request_call(
            &self.proxy,
            &handle_token,
            "Start",
            &(&self.session_handle, window_identifier, options).to_variant(),
        )
        .await?;

        tracing::info!("Started screencast session");

        let streams_variant = response
            .get("streams")
            .ok_or_else(|| anyhow!("No streams received from response"))?;

        let streams = streams_variant.get::<Vec<Stream>>().ok_or_else(|| {
            anyhow!(
                "Expected streams signature of {}. Got {}",
                <Vec<Stream>>::static_variant_type(),
                streams_variant.type_()
            )
        })?;

        tracing::info!("Received streams {:?}", streams);

        if let Some(variant) = response.get("restore_token") {
            let restore_token = variant.get::<String>().ok_or_else(|| {
                anyhow!("Expected restore_token of type s. Got {}", variant.type_())
            })?;
            return Ok((streams, Some(restore_token)));
        }

        Ok((streams, None))
    }

    async fn select_sources(
        &self,
        source_type: SourceType,
        multiple: bool,
        cursor_mode: CursorMode,
        restore_token: Option<&str>,
        persist_mode: PersistMode,
    ) -> Result<()> {
        let handle_token = HandleToken::new();

        let mut options = vec![
            ("handle_token", handle_token.to_variant()),
            ("types", source_type.bits().to_variant()),
            ("multiple", multiple.to_variant()),
            ("cursor_mode", cursor_mode.bits().to_variant()),
            ("persist_mode", (persist_mode as u32).to_variant()),
        ];

        if let Some(restore_token) = restore_token.filter(|t| !t.is_empty()) {
            options.push(("restore_token", restore_token.to_variant()));
        }

        screencast_request_call(
            &self.proxy,
            &handle_token,
            "SelectSources",
            &(&self.session_handle, HashMap::from_iter(options)).to_variant(),
        )
        .await?;

        tracing::info!("Selected sources");

        Ok(())
    }

    async fn open_pipe_wire_remote(&self) -> Result<RawFd> {
        let (_, fd_list) = self
            .proxy
            .call_with_unix_fd_list_future(
                "OpenPipeWireRemote",
                Some(
                    &(
                        &self.session_handle,
                        HashMap::<String, glib::Variant>::new(),
                    )
                        .to_variant(),
                ),
                gio::DBusCallFlags::NONE,
                DEFAULT_TIMEOUT.as_millis() as i32,
                gio::UnixFDList::NONE,
            )
            .await?;

        tracing::info!("Opened pipe wire remote");

        let mut fds = fd_list.steal_fds();

        ensure!(fds.len() == 1, "Expected 1 fd, got {}", fds.len());

        Ok(fds.pop().unwrap())
    }
}

async fn screencast_request_call(
    proxy: &gio::DBusProxy,
    handle_token: &HandleToken,
    method: &str,
    parameters: &glib::Variant,
) -> Result<HashMap<String, glib::Variant>> {
    let connection = proxy.connection();

    let unique_identifier = connection
        .unique_name()
        .expect("Connection has no unique name")
        .trim_start_matches(':')
        .replace('.', "_");

    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        unique_identifier,
        handle_token.as_str()
    );

    let (tx, rx) = oneshot::channel();
    let tx = RefCell::new(Some(tx));

    let subscription_id = connection.signal_subscribe(
        Some("org.freedesktop.portal.Desktop"),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(&request_path),
        None,
        gio::DBusSignalFlags::NONE,
        move |_connection, _sender_name, _object_path, _interface_name, _signal_name, output| {
            if let Some(tx) = tx.take() {
                let _ = tx.send(output.clone());
            } else {
                tracing::warn!("Received another response for already finished request");
            }
        },
    );

    let path = proxy
        .call_future(
            method,
            Some(parameters),
            gio::DBusCallFlags::NONE,
            DEFAULT_TIMEOUT.as_millis() as i32,
        )
        .await
        .with_context(|| {
            format!(
                "Failed to call `{}` with parameters: {:?}",
                method, parameters
            )
        })?;

    let response_variant = rx
        .await
        .with_context(|| Cancelled::new(method))
        .context("Sender dropped")?;

    debug_assert_eq!(path.get::<(String,)>().map(|(p,)| p), Some(request_path));

    connection.signal_unsubscribe(subscription_id);

    let (response_no, response) = response_variant
        .get::<(u32, HashMap<String, glib::Variant>)>()
        .ok_or_else(|| {
            anyhow!(
                "Expected return type of ua{{sv}}. Got {} with value {:?}",
                response_variant.type_(),
                response_variant.print(true)
            )
        })?;

    match response_no {
        0 => Ok(response),
        1 => Err(Cancelled::new(method)).context("Cancelled by user"),
        2 => Err(anyhow!(
            "Interaction was ended in some other way with response {:?}",
            response
        )),
        no => Err(anyhow!("Unknown response number of {}", no)),
    }
}
