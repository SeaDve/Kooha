use ashpd::{
    desktop::{
        screencast::{CursorMode, PersistMode, ScreenCastProxy, SourceType, Stream},
        ResponseError, SessionProxy,
    },
    enumflags2::BitFlags,
    zbus, PortalError, WindowIdentifier,
};
use error_stack::{IntoReport, Report, Result, ResultExt, Context};
use gtk::prelude::*;

use std::{fmt, os::unix::io::RawFd};

#[derive(Debug)]

pub enum ScreencastSessionError {
    Cancelled,
    Other,
}

impl fmt::Display for ScreencastSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("screencast session cancelled"),
            Self::Other => f.write_str("screencast session error"),
        }
    }
}

impl Context for ScreencastSessionError {}

#[derive(Debug)]
pub struct ScreencastSession {
    proxy: ScreenCastProxy<'static>,
    session: SessionProxy<'static>,
}

impl ScreencastSession {
    pub async fn new() -> Result<Self, ScreencastSessionError> {
        // TODO fix `Invalid client serial`
        let connection = zbus::Connection::session()
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to create zbus connection")?;
        let proxy = ScreenCastProxy::new(&connection)
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to create ashpd screencast proxy")?;

        let session = proxy
            .create_session()
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to create screencast proxy session")?;

        Ok(Self { proxy, session })
    }

    pub async fn available_cursor_modes(
        &self,
    ) -> Result<BitFlags<CursorMode>, ScreencastSessionError> {
        self.proxy
            .available_cursor_modes()
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to get available cursor modes property")
    }

    pub async fn available_source_types(
        &self,
    ) -> Result<BitFlags<SourceType>, ScreencastSessionError> {
        self.proxy
            .available_source_types()
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to get available source types property")
    }

    pub async fn start(
        &self,
        cursor_mode: BitFlags<CursorMode>,
        source_type: BitFlags<SourceType>,
        is_multiple_sources: bool,
        restore_token: Option<&str>,
        persist_mode: PersistMode,
        parent_window: Option<&impl IsA<gtk::Window>>,
    ) -> Result<(Vec<Stream>, Option<String>, RawFd), ScreencastSessionError> {
        self.proxy
            .select_sources(
                &self.session,
                cursor_mode,
                source_type,
                is_multiple_sources,
                restore_token.filter(|s| !s.is_empty()),
                persist_mode,
            )
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to invoke select sources method")?;

        let window_identifier = if let Some(window) = parent_window {
            WindowIdentifier::from_native(window.upcast_ref()).await
        } else {
            WindowIdentifier::None
        };

        let (streams, output_restore_token) =
            match self.proxy.start(&self.session, &window_identifier).await {
                Ok((streams, output_restore_token)) => (streams, output_restore_token),
                Err(err) => match err {
                    ashpd::Error::Portal(PortalError::Cancelled(msg)) => {
                        return Err(Report::new(ScreencastSessionError::Cancelled))
                            .attach_printable(msg)
                    }
                    ashpd::Error::Response(ResponseError::Cancelled) => {
                        return Err(Report::new(ScreencastSessionError::Cancelled))
                    }
                    err => {
                        return Err(Report::from(err)
                            .change_context(ScreencastSessionError::Other)
                            .attach_printable("failed to invoke start method"));
                    }
                },
            };

        let fd = self
            .proxy
            .open_pipe_wire_remote(&self.session)
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to invoke open pipe wire remote method")?;

        Ok((streams, output_restore_token, fd))
    }

    pub async fn close(self) -> Result<(), ScreencastSessionError> {
        self.session
            .close()
            .await
            .report()
            .change_context(ScreencastSessionError::Other)
            .attach_printable("failed to close screencast proxy session")
    }
}
