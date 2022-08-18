mod handle;
mod handle_token;
mod object_path;
mod types;
mod variant_dict;
mod window_identifier;

use anyhow::{anyhow, bail, Context, Result};
use futures_channel::oneshot;
use futures_util::future::{self, Either};
use gtk::{
    gio,
    glib::{self, FromVariant},
    prelude::*,
};

use std::{cell::RefCell, os::unix::io::RawFd, time::Duration};

pub use self::types::{CursorMode, PersistMode, SourceType, Stream};
use self::{
    handle::Handle, handle_token::HandleToken, object_path::ObjectPath, variant_dict::VariantDict,
    window_identifier::WindowIdentifier,
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

        let session_options = VariantDict::from_iter([
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

        tracing::debug!(?response, "Created screencast session");

        let session_handle = response.get::<ObjectPath>("session_handle")?;

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

    pub async fn close(self) -> Result<()> {
        let response = self
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
        debug_assert!(variant_get::<()>(&response).is_ok());

        tracing::debug!(?response, "Closed screencast session");

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

    fn property<T: FromVariant>(&self, name: &str) -> Result<T> {
        let variant = self
            .proxy
            .cached_property(name)
            .ok_or_else(|| anyhow!("No cached property named `{}`", name))?;

        variant_get::<T>(&variant)
    }

    async fn start(
        &self,
        window_identifier: WindowIdentifier,
    ) -> Result<(Vec<Stream>, Option<String>)> {
        let handle_token = HandleToken::new();

        let options = VariantDict::from_iter([("handle_token", handle_token.to_variant())]);

        let response = screencast_request_call(
            &self.proxy,
            &handle_token,
            "Start",
            &(&self.session_handle, window_identifier, options).to_variant(),
        )
        .await?;

        tracing::debug!(?response, "Started screencast session");

        let streams = response.get::<Vec<Stream>>("streams")?;

        if let Some(restore_token) = response.get_optional("restore_token")? {
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

        let response = screencast_request_call(
            &self.proxy,
            &handle_token,
            "SelectSources",
            &(&self.session_handle, VariantDict::from_iter(options)).to_variant(),
        )
        .await?;
        debug_assert!(response.is_empty());

        tracing::debug!(?response, "Selected sources");

        Ok(())
    }

    async fn open_pipe_wire_remote(&self) -> Result<RawFd> {
        let (response, fd_list) = self
            .proxy
            .call_with_unix_fd_list_future(
                "OpenPipeWireRemote",
                Some(&(&self.session_handle, VariantDict::new()).to_variant()),
                gio::DBusCallFlags::NONE,
                DEFAULT_TIMEOUT.as_millis() as i32,
                gio::UnixFDList::NONE,
            )
            .await?;

        tracing::debug!(?response, fd_list = ?fd_list.peek_fds(), "Opened pipe wire remote");

        let (fd_index,) = variant_get::<(Handle,)>(&response)?;

        debug_assert_eq!(fd_list.length(), 1);

        let fd = fd_list
            .get(fd_index.inner())
            .with_context(|| format!("Failed to get fd at index `{}`", fd_index.inner()))?;

        Ok(fd)
    }
}

async fn screencast_request_call(
    proxy: &gio::DBusProxy,
    handle_token: &HandleToken,
    method: &str,
    parameters: &glib::Variant,
) -> Result<VariantDict> {
    let connection = proxy.connection();

    let unique_identifier = connection
        .unique_name()
        .expect("Connection has no unique name")
        .trim_start_matches(':')
        .replace('.', "_");

    let request_path = {
        let path = format!(
            "/org/freedesktop/portal/desktop/request/{}/{}",
            unique_identifier,
            handle_token.as_str()
        );
        ObjectPath::new(&path)
            .ok_or_else(|| anyhow!("Failed to create object path from `{}`", path))?
    };

    let request_proxy = gio::DBusProxy::for_bus_future(
        gio::BusType::Session,
        gio::DBusProxyFlags::DO_NOT_AUTO_START
            | gio::DBusProxyFlags::DO_NOT_CONNECT_SIGNALS
            | gio::DBusProxyFlags::DO_NOT_LOAD_PROPERTIES,
        None,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .await?;

    let (name_owner_lost_tx, name_owner_lost_rx) = oneshot::channel();
    let name_owner_lost_tx = RefCell::new(Some(name_owner_lost_tx));

    let handler_id = request_proxy.connect_notify_local(Some("g-name-owner"), move |proxy, _| {
        if proxy.g_name_owner().is_none() {
            tracing::warn!("Lost request name owner");

            if let Some(tx) = name_owner_lost_tx.take() {
                let _ = tx.send(());
            } else {
                tracing::warn!("Received another g name owner notify");
            }
        }
    });

    let (response_tx, response_rx) = oneshot::channel();
    let response_tx = RefCell::new(Some(response_tx));

    let subscription_id = connection.signal_subscribe(
        Some("org.freedesktop.portal.Desktop"),
        Some("org.freedesktop.portal.Request"),
        Some("Response"),
        Some(request_path.as_str()),
        None,
        gio::DBusSignalFlags::NONE,
        move |_connection, _sender_name, _object_path, _interface_name, _signal_name, output| {
            if let Some(tx) = response_tx.take() {
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
    debug_assert_eq!(variant_get::<(ObjectPath,)>(&path).unwrap().0, request_path);

    tracing::debug!("Waiting request response for method `{}`", method);

    let response = match future::select(response_rx, name_owner_lost_rx).await {
        Either::Left((res, _)) => res
            .with_context(|| Cancelled::new(method))
            .context("Sender dropped")?,
        Either::Right(_) => bail!("Lost name owner for request"),
    };
    request_proxy.disconnect(handler_id);
    connection.signal_unsubscribe(subscription_id);

    tracing::debug!("Request response received for method `{}`", method);

    let (response_no, response) = variant_get::<(u32, VariantDict)>(&response)?;

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

/// Provides error messages on incorrect variant type.
fn variant_get<T: FromVariant>(variant: &glib::Variant) -> Result<T> {
    variant.get::<T>().ok_or_else(|| {
        anyhow!(
            "Expected type `{}`; got `{}` with value `{}`",
            T::static_variant_type(),
            variant.type_(),
            variant.print(true)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_variant_ok() {
        let variant = ("foo",).to_variant();
        let (value,) = variant_get::<(String,)>(&variant).unwrap();
        assert_eq!(value, "foo");
    }

    #[test]
    fn get_variant_wrong_type() {
        let variant = "foo".to_variant();
        let err = variant_get::<u32>(&variant).unwrap_err();
        assert_eq!(
            "Expected type `u`; got `s` with value `'foo'`",
            err.to_string()
        );
    }
}
