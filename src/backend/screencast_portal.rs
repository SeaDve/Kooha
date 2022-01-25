use ashpd::{
    desktop::{
        screencast::{CursorMode, PersistMode, ScreenCastProxy, SourceType, Stream},
        ResponseError, SessionProxy,
    },
    enumflags2::BitFlags,
    zbus, WindowIdentifier,
};
use gtk::{
    glib::{self, clone},
    subclass::prelude::*,
};

use std::{cell::RefCell, os::unix::io::RawFd};

use crate::{error::Error, settings::Settings, Application};

#[derive(Debug)]
pub enum ScreencastPortalResponse {
    Success(Vec<Stream>, i32),
    Failed(Error),
    Cancelled,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct ScreencastPortal {
        pub session: RefCell<Option<SessionProxy<'static>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ScreencastPortal {
        const NAME: &'static str = "KoohaScreencastPortal";
        type Type = super::ScreencastPortal;
    }

    impl ObjectImpl for ScreencastPortal {}
}

glib::wrapper! {
    pub struct ScreencastPortal(ObjectSubclass<imp::ScreencastPortal>);
}

impl ScreencastPortal {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create ScreencastPortal.")
    }

    fn cursor_mode(is_show_pointer: bool) -> BitFlags<CursorMode> {
        if is_show_pointer {
            BitFlags::<CursorMode>::from_flag(CursorMode::Embedded)
        } else {
            BitFlags::<CursorMode>::from_flag(CursorMode::Hidden)
        }
    }

    fn source_type(is_selection_mode: bool) -> BitFlags<SourceType> {
        if is_selection_mode {
            BitFlags::<SourceType>::from_flag(SourceType::Monitor)
        } else {
            SourceType::Monitor | SourceType::Window
        }
    }

    fn settings() -> Settings {
        Application::default().settings()
    }

    async fn identifier() -> WindowIdentifier {
        let main_window = Application::default().main_window();
        WindowIdentifier::from_native(&main_window).await
    }

    pub async fn new_session(
        &self,
        is_show_pointer: bool,
        is_selection_mode: bool,
    ) -> ScreencastPortalResponse {
        let identifier = Self::identifier().await;
        let multiple = !is_selection_mode;
        let source_type = Self::source_type(is_selection_mode);
        let cursor_mode = Self::cursor_mode(is_show_pointer);
        let restore_token = Self::settings().screencast_restore_token();

        match screencast(
            identifier,
            multiple,
            source_type,
            cursor_mode,
            restore_token.as_deref(),
        )
        .await
        {
            Ok(result) => {
                let (streams, fd, restore_token, session) = result;
                self.imp().session.replace(Some(session));
                Self::settings().set_screencast_restore_token(restore_token.as_deref());

                ScreencastPortalResponse::Success(streams, fd)
            }
            Err(error) => match error {
                ashpd::Error::Response(ResponseError::Cancelled) => {
                    log::info!("Select sources cancelled");
                    ScreencastPortalResponse::Cancelled
                }
                other_error => {
                    log::error!("Error from screencast call: {:?}", other_error);
                    ScreencastPortalResponse::Failed(Error::from(other_error))
                }
            },
        }
    }

    pub fn close_session(&self) {
        let ctx = glib::MainContext::default();
        ctx.spawn_local(clone!(@weak self as obj => async move {
            if let Some(session) = obj.imp().session.take() {
                session.close().await.unwrap();
            };
        }));

        log::info!("Session closed");
    }
}

async fn screencast(
    window_identifier: WindowIdentifier,
    multiple: bool,
    types: BitFlags<SourceType>,
    cursor_mode: BitFlags<CursorMode>,
    restore_token: Option<&str>,
) -> Result<(Vec<Stream>, RawFd, Option<String>, SessionProxy<'static>), ashpd::Error> {
    let connection = zbus::Connection::session().await?;
    let proxy = ScreenCastProxy::new(&connection).await?;
    log::info!("ScreenCastProxy created");

    log::debug!("restore_token: {:?}", restore_token);

    log::debug!(
        "available_cursor_modes: {:?}",
        proxy.available_cursor_modes().await?
    );
    log::debug!(
        "available_source_types: {:?}",
        proxy.available_source_types().await?
    );

    let session = proxy.create_session().await?;
    log::info!("Session created");

    proxy
        .select_sources(
            &session,
            cursor_mode,
            types,
            multiple,
            restore_token,
            PersistMode::ExplicitlyRevoked,
        )
        .await?;
    log::info!("Select sources window showed");

    let (streams, output_restore_token) = proxy.start(&session, &window_identifier).await?;
    log::info!("Screencast session started");

    log::debug!("output_restore_token: {:?}", output_restore_token);

    let fd = proxy.open_pipe_wire_remote(&session).await?;
    log::info!("Ready for pipewire stream");

    Ok((streams, fd, output_restore_token, session))
}

impl Default for ScreencastPortal {
    fn default() -> Self {
        Self::new()
    }
}
