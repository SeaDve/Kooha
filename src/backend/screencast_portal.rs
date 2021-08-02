use ashpd::{
    desktop::{
        screencast::{CursorMode, ScreenCastProxy, SourceType, Stream},
        ResponseError, SessionProxy,
    },
    enumflags2::BitFlags,
    zbus, Error, WindowIdentifier,
};
use gtk::{
    glib::{self, clone, subclass::Signal, GBoxed, WeakRef},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::{Lazy, OnceCell};

use std::{cell::RefCell, os::unix::io::RawFd};

use crate::{data_types::Screen, widgets::MainWindow};

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "ScreencastPortalResponse")]
pub enum ScreencastPortalResponse {
    Success(i32, u32, Screen),
    Error(String),
    Cancelled,
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct ScreencastPortal {
        pub window: OnceCell<WeakRef<MainWindow>>,
        pub session: RefCell<Option<SessionProxy<'static>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ScreencastPortal {
        const NAME: &'static str = "ScreencastPortal";
        type Type = super::ScreencastPortal;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                window: OnceCell::new(),
                session: RefCell::new(None),
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
        imp::ScreencastPortal::from_instance(self)
    }

    fn emit_response(&self, response: ScreencastPortalResponse) {
        self.emit_by_name("response", &[&response]).unwrap();
    }

    fn window(&self) -> MainWindow {
        let imp = self.private();
        imp.window.get().unwrap().upgrade().unwrap()
    }

    pub fn set_window(&self, window: &MainWindow) {
        let imp = self.private();
        imp.window.set(window.downgrade()).unwrap();
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
            let identifier = WindowIdentifier::from_native(&obj.window().native().unwrap()).await;
            let multiple = false;

            match screencast(identifier, multiple, source_type, cursor_mode).await {
                Ok(result) => {
                    let (streams, fd, session) = result;
                    let node_id = streams[0].pipe_wire_node_id();
                    let stream_size = streams[0].size().unwrap();
                    let stream_screen = Screen::new(stream_size.0, stream_size.1);

                    obj.emit_response(ScreencastPortalResponse::Success(fd, node_id, stream_screen));

                    imp.session.replace(Some(session));
                }
                Err(error) => {
                    log::warn!("Something occurred on screencast call: {}", &error);

                    match error {
                        Error::Portal(response_error) => {
                            match response_error {
                                ResponseError::Cancelled => {
                                    obj.emit_response(ScreencastPortalResponse::Cancelled);
                                },
                                ResponseError::Other => {
                                    obj.emit_response(ScreencastPortalResponse::Error(response_error.to_string()));
                                }
                            }
                        },
                        other => {
                            obj.emit_response(ScreencastPortalResponse::Error(other.to_string()));
                        }
                    };
                }
            };
        }));
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
