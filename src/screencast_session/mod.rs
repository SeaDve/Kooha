mod handle_token;
mod object_path;
mod types;
mod window_identifier;

use error_stack::{report, Context, IntoReport, Report, Result, ResultExt};
use futures_channel::oneshot;
use gtk::{gio, glib, prelude::*};

use std::{cell::RefCell, collections::HashMap, fmt, os::unix::io::RawFd};

pub use self::types::{CursorMode, PersistMode, SourceType, Stream};
use self::{
    handle_token::HandleToken, object_path::ObjectPath, window_identifier::WindowIdentifier,
};

// TODO add timeout limit

#[derive(Debug)]

pub enum ScreencastSessionError {
    Cancelled,
    Response,
    Other,
}

impl fmt::Display for ScreencastSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("screencast session cancelled"),
            Self::Response => f.write_str("screencast session response error"),
            Self::Other => f.write_str("screencast session error"),
        }
    }
}

impl Context for ScreencastSessionError {}

#[derive(Debug)]
pub struct ScreencastSession {
    proxy: gio::DBusProxy,
    session_handle: ObjectPath,
}

impl ScreencastSession {
    pub async fn new() -> Result<Self, ScreencastSessionError> {
        let proxy = gio::DBusProxy::for_bus_future(
            gio::BusType::Session,
            gio::DBusProxyFlags::NONE,
            None,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.ScreenCast",
        )
        .await
        .report()
        .change_context(ScreencastSessionError::Other)
        .attach_printable("failed to create screencast proxy")?;

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
        .attach_printable("failed to create session")?;

        tracing::info!("Created screencast session");

        let session_handle = response
            .get("session_handle")
            .ok_or_else(|| Report::new(ScreencastSessionError::Other))?
            .get::<ObjectPath>()
            .ok_or_else(|| Report::new(ScreencastSessionError::Other))?;

        assert!(session_handle
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
    ) -> Result<(Vec<Stream>, Option<String>, RawFd), ScreencastSessionError> {
        self.select_sources(
            source_type,
            is_multiple_sources,
            cursor_mode,
            restore_token.filter(|s| !s.is_empty()),
            persist_mode,
        )
        .await
        .attach_printable("failed to invoke select sources method")?;

        let window_identifier = if let Some(window) = parent_window {
            WindowIdentifier::new(window.upcast_ref()).await
        } else {
            WindowIdentifier::None
        };

        let (streams, output_restore_token) = self
            .start(window_identifier)
            .await
            .attach_printable("failed to invoke start method")?;

        let fd = self
            .open_pipe_wire_remote()
            .await
            .attach_printable("failed to invoke open pipe wire remote method")?;

        Ok((streams, output_restore_token, fd))
    }

    pub async fn close(&self) -> Result<(), ScreencastSessionError> {
        tracing::info!("Created screencast session");

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
                -1,
            )
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("Failed to close session")?;

        assert_eq!(ret.get(), Some(()));

        Ok(())
    }

    async fn start(
        &self,
        window_identifier: WindowIdentifier,
    ) -> Result<(Vec<Stream>, Option<String>), ScreencastSessionError> {
        let handle_token = HandleToken::new();

        let options = HashMap::from([("handle_token", handle_token.to_variant())]);

        let response = screencast_request_call(
            &self.proxy,
            &handle_token,
            "Start",
            &(&self.session_handle, window_identifier, options).to_variant(),
        )
        .await
        .attach_printable("failed to start session")?;

        tracing::info!("Started screencast session");

        let streams = response
            .get("streams")
            .ok_or_else(|| {
                Report::new(ScreencastSessionError::Other)
                    .attach_printable("No streams received from response")
            })?
            .get::<Vec<Stream>>()
            .ok_or_else(|| {
                Report::new(ScreencastSessionError::Other)
                    .attach_printable("Invalid streams signature")
            })?;

        match response.get("restore_token") {
            Some(restore_token) => Ok((
                streams,
                Some(restore_token.get::<String>().ok_or_else(|| {
                    Report::new(ScreencastSessionError::Other)
                        .attach_printable("Invalid streams signature")
                })?),
            )),
            None => Ok((streams, None)),
        }
    }

    pub async fn available_cursor_modes(&self) -> Result<CursorMode, ScreencastSessionError> {
        self.proxy
            .cached_property("AvailableCursorModes")
            .and_then(|variant| variant.get::<u32>())
            .and_then(CursorMode::from_bits)
            .ok_or_else(|| Report::new(ScreencastSessionError::Other))
    }

    pub async fn available_source_types(&self) -> Result<SourceType, ScreencastSessionError> {
        self.proxy
            .cached_property("AvailableSourceTypes")
            .and_then(|variant| variant.get::<u32>())
            .and_then(SourceType::from_bits)
            .ok_or_else(|| Report::new(ScreencastSessionError::Other))
    }

    async fn select_sources(
        &self,
        source_type: SourceType,
        multiple: bool,
        cursor_mode: CursorMode,
        restore_token: Option<&str>,
        persist_mode: PersistMode,
    ) -> Result<(), ScreencastSessionError> {
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
        .await
        .attach_printable("failed to get available cursor modes property")?;

        tracing::info!("Selected sources");

        Ok(())
    }

    async fn open_pipe_wire_remote(&self) -> Result<RawFd, ScreencastSessionError> {
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
                -1,
                gio::UnixFDList::NONE,
            )
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to open pipe wire remote")?;

        tracing::info!("Opened pipe wire remote");

        let mut fds = fd_list.steal_fds();

        if fds.len() != 1 {
            return Err(
                Report::new(ScreencastSessionError::Other).attach_printable(format!(
                    "Expected 1 fd from OpenPipeWireRemote, got {}",
                    fds.len()
                )),
            );
        }

        Ok(fds.pop().unwrap())
    }
}

async fn screencast_request_call(
    proxy: &gio::DBusProxy,
    handle_token: &HandleToken,
    method: &str,
    parameters: &glib::Variant,
) -> Result<HashMap<String, glib::Variant>, ScreencastSessionError> {
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
        move |_connection, _sender_name, object_path, _interface_name, _signal_name, output| {
            if let Some(tx) = tx.take() {
                tracing::info!("Received response to request {}", object_path);

                let _ = tx.send(output.clone());
            } else {
                tracing::warn!("Received another response for already finished request");
            }
        },
    );

    tracing::info!("Subscribed to request response {}", request_path);

    let (path, response_variant) = futures_util::try_join!(
        async {
            proxy
                .call_future(method, Some(parameters), gio::DBusCallFlags::NONE, -1)
                .await
                .report()
                .change_context(ScreencastSessionError::Other)
        },
        async {
            rx.await
                .report()
                .change_context(ScreencastSessionError::Cancelled)
        }
    )?;

    assert_eq!(path.get::<(String,)>().map(|(p,)| p), Some(request_path));

    connection.signal_unsubscribe(subscription_id);

    let (response_no, response) = response_variant
        .get::<(u32, HashMap<String, glib::Variant>)>()
        .ok_or_else(|| {
            Report::new(ScreencastSessionError::Other).attach_printable(format!(
                "Expected return type of ua{{sv}}. Got {} with value {:?}",
                response_variant.type_(),
                response_variant.print(true)
            ))
        })?;

    match response_no {
        0 => Ok(response),
        1 => Err(report!(ScreencastSessionError::Cancelled)),
        2 => Err(report!(ScreencastSessionError::Response)),
        o => {
            tracing::warn!("Unexpected response number {}", o);
            Err(report!(ScreencastSessionError::Response))
        }
    }
}
