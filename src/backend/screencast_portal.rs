use ashpd::{
    desktop::{
        screencast::{CursorMode, ScreenCastProxy, SourceType, Stream},
        SessionProxy,
    },
    enumflags2::BitFlags,
    zbus, WindowIdentifier,
};
use futures::lock::Mutex;
use gtk::{
    glib::{self, clone, subclass::Signal, GBoxed},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::{os::unix::io::RawFd, sync::Arc};

use crate::data_types::Screen;

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "ScreencastPortalResponse")]
pub enum ScreencastPortalResponse {
    Success(i32, u32, Screen),
    Revoked,
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct ScreencastPortal {
        pub session: Arc<Mutex<Option<SessionProxy<'static>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ScreencastPortal {
        const NAME: &'static str = "ScreencastPortal";
        type Type = super::ScreencastPortal;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                session: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl ObjectImpl for ScreencastPortal {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder(
                    "response",
                    &[ScreencastPortalResponse::static_type().into()],
                    <()>::static_type().into(),
                )
                .build()]
            });
            SIGNALS.as_ref()
        }
    }
}

glib::wrapper! {
    pub struct ScreencastPortal(ObjectSubclass<imp::ScreencastPortal>);
}

impl ScreencastPortal {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create KhaPortal")
    }

    fn private(&self) -> &imp::ScreencastPortal {
        &imp::ScreencastPortal::from_instance(self)
    }

    pub fn new_session(&self, is_show_pointer: bool, is_selection_mode: bool) {
        let ctx = glib::MainContext::default();
        ctx.spawn_local(clone!(@weak self as obj => async move {
            let imp = obj.private();

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
            let identifier = WindowIdentifier::default();
            let multiple = false;

            match screencast(identifier, multiple, source_type, cursor_mode).await {
                Ok(result) => {
                    let (streams, fd, session) = result;
                    let node_id = streams[0].pipe_wire_node_id();
                    let stream_size = streams[0].size().unwrap();
                    let stream_screen = Screen::new(stream_size.0, stream_size.1);

                    obj.emit_response(ScreencastPortalResponse::Success(fd, node_id, stream_screen));

                    imp.session.lock().await.replace(session);
                }
                Err(error) => {
                    log::warn!("{}", error);
                    obj.emit_response(ScreencastPortalResponse::Revoked)
                }
            };
        }));
    }

    pub fn close_session(&self) {
        let ctx = glib::MainContext::default();
        ctx.spawn_local(clone!(@weak self as obj => async move {
            let imp = obj.private();

            if let Some(session) = imp.session.lock().await.take() {
                session.close().await.unwrap();
            };
        }));
    }

    fn emit_response(&self, response: ScreencastPortalResponse) {
        self.emit_by_name("response", &[&response]).unwrap();
    }
}

pub async fn screencast(
    window_identifier: WindowIdentifier,
    multiple: bool,
    types: BitFlags<SourceType>,
    cursor_mode: BitFlags<CursorMode>,
) -> Result<(Vec<Stream>, RawFd, SessionProxy<'static>), ashpd::Error> {
    let connection = zbus::azync::Connection::new_session().await?;
    let proxy = ScreenCastProxy::new(&connection).await?;

    let session = proxy.create_session().await?;

    proxy
        .select_sources(&session, cursor_mode, types, multiple)
        .await?;

    let streams = proxy.start(&session, window_identifier).await?.to_vec();

    let fd = proxy.open_pipe_wire_remote(&session).await?;
    Ok((streams, fd, session))
}
