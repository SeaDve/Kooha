use ashpd::{
    desktop::{
        screencast::{CursorMode, ScreenCastProxy, SourceType, Stream},
        ResponseError, SessionProxy,
    },
    enumflags2::BitFlags,
    zbus, WindowIdentifier,
};
use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use std::{cell::RefCell, os::unix::io::RawFd};

use crate::{application::Application, error::Error};

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
        type ParentType = glib::Object;
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

    fn private(&self) -> &imp::ScreencastPortal {
        imp::ScreencastPortal::from_instance(self)
    }

    pub async fn new_session(
        &self,
        is_show_pointer: bool,
        is_selection_mode: bool,
    ) -> ScreencastPortalResponse {
        let imp = self.private();

        let source_type = if is_selection_mode {
            BitFlags::<SourceType>::from_flag(SourceType::Monitor)
        } else {
            SourceType::Monitor | SourceType::Window
        };

        let cursor_mode = if is_show_pointer {
            BitFlags::<CursorMode>::from_flag(CursorMode::Embedded)
        } else {
            BitFlags::<CursorMode>::from_flag(CursorMode::Hidden)
        };

        let main_window = Application::default().main_window();
        let identifier = WindowIdentifier::from_native(&main_window.native().unwrap()).await;

        let multiple = !is_selection_mode;

        match screencast(identifier, multiple, source_type, cursor_mode).await {
            Ok(result) => {
                let (streams, fd, session) = result;
                imp.session.replace(Some(session));

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
            let imp = obj.private();

            if let Some(session) = imp.session.take() {
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
) -> Result<(Vec<Stream>, RawFd, SessionProxy<'static>), ashpd::Error> {
    let connection = zbus::azync::Connection::session().await?;
    let proxy = ScreenCastProxy::new(&connection).await?;
    log::info!("ScreenCastProxy created");

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
        .select_sources(&session, cursor_mode, types, multiple)
        .await?;
    log::info!("Select sources window showed");

    let streams = proxy.start(&session, &window_identifier).await?;
    log::info!("Screencast session started");

    let fd = proxy.open_pipe_wire_remote(&session).await?;
    log::info!("Ready for pipewire stream");

    Ok((streams, fd, session))
}

impl Default for ScreencastPortal {
    fn default() -> Self {
        Self::new()
    }
}
