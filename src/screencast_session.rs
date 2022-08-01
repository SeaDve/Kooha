use ashpd::{
    desktop::{
        screencast::{CursorMode, PersistMode, ScreenCastProxy, SourceType, Stream},
        SessionProxy,
    },
    enumflags2::BitFlags,
    zbus, WindowIdentifier,
};
use gtk::prelude::*;

use std::os::unix::io::RawFd;

#[derive(Debug)]
pub struct ScreencastSession {
    proxy: ScreenCastProxy<'static>,
    session: SessionProxy<'static>,
}

impl ScreencastSession {
    pub async fn new() -> ashpd::Result<Self> {
        // TODO use gio dbus for less deps
        // TODO fix `Invalid client serial`
        let connection = zbus::Connection::session().await?;
        let proxy = ScreenCastProxy::new(&connection).await?;

        let session = proxy.create_session().await?;

        Ok(Self { proxy, session })
    }

    pub async fn available_cursor_modes(&self) -> ashpd::Result<BitFlags<CursorMode>> {
        self.proxy.available_cursor_modes().await
    }

    pub async fn available_source_types(&self) -> ashpd::Result<BitFlags<SourceType>> {
        self.proxy.available_source_types().await
    }

    pub async fn start(
        &self,
        cursor_mode: BitFlags<CursorMode>,
        source_type: BitFlags<SourceType>,
        is_multiple_sources: bool,
        restore_token: Option<&str>,
        persist_mode: PersistMode,
        parent_window: Option<&impl IsA<gtk::Window>>,
    ) -> ashpd::Result<(Vec<Stream>, Option<String>, RawFd)> {
        self.proxy
            .select_sources(
                &self.session,
                cursor_mode,
                source_type,
                is_multiple_sources,
                restore_token.filter(|s| !s.is_empty()),
                persist_mode,
            )
            .await?;

        let window_identifier = if let Some(window) = parent_window {
            WindowIdentifier::from_native(window.upcast_ref()).await
        } else {
            WindowIdentifier::None
        };

        let (streams, output_restore_token) =
            self.proxy.start(&self.session, &window_identifier).await?;

        let fd = self.proxy.open_pipe_wire_remote(&self.session).await?;

        Ok((streams, output_restore_token, fd))
    }

    pub async fn close(self) -> ashpd::Result<()> {
        self.session.close().await
    }
}
