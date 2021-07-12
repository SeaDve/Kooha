use ashpd::{
    desktop::screencast::{
        CreateSession, CreateSessionOptions, CursorMode, ScreenCastProxy, SelectSourcesOptions,
        SourceType, StartCastOptions, Streams,
    },
    zvariant::{Fd, ObjectPath},
    BasicResponse, HandleToken, RequestProxy, Response, WindowIdentifier,
};
use enumflags2::BitFlags;
use gtk::glib::{self, GBoxed};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::os::unix::prelude::AsRawFd;
use zbus::{self, fdo::Result};

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, GBoxed)]
#[gboxed(type_name = "Screen")]
pub struct Screen {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, GBoxed)]
#[gboxed(type_name = "Stream")]
pub struct Stream {
    pub fd: i32,
    pub node_id: u32,
    pub screen: Screen,
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    pub struct KhaScreencastPortal {}

    #[glib::object_subclass]
    impl ObjectSubclass for KhaScreencastPortal {
        const NAME: &'static str = "KhaScreencastPortal";
        type Type = super::KhaScreencastPortal;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {}
        }
    }

    impl ObjectImpl for KhaScreencastPortal {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder(
                        "ready",
                        &[Stream::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                    Signal::builder("revoked", &[], <()>::static_type().into()).build(),
                ]
            });
            SIGNALS.as_ref()
        }
    }
}

glib::wrapper! {
    pub struct KhaScreencastPortal(ObjectSubclass<imp::KhaScreencastPortal>);
}

impl KhaScreencastPortal {
    pub fn new() -> Self {
        let obj: Self = glib::Object::new::<Self>(&[]).expect("Failed to initialize Portal object");
        obj
    }

    fn create_session(&self) -> Result<()> {
        let connection = zbus::Connection::new_session()?;
        let proxy = ScreenCastProxy::new(&connection)?;

        let session_token = HandleToken::try_from("session120").unwrap();

        let request_handle = proxy
            .create_session(CreateSessionOptions::default().session_handle_token(session_token))?;
        let request = RequestProxy::new(&connection, &request_handle)?;

        request.on_response(|r: Response<CreateSession>| {
            match r {
                Ok(session) => self
                    .select_sources(session.handle(), &proxy, &connection)
                    .unwrap(),
                Err(_) => println!("hello!"),
            };
        })?;
        Ok(())
    }

    fn select_sources(
        &self,
        session_handle: ObjectPath,
        proxy: &ScreenCastProxy,
        connection: &zbus::Connection,
    ) -> Result<()> {
        let request_handle = proxy.select_sources(
            session_handle.clone(),
            SelectSourcesOptions::default()
                .multiple(true)
                .cursor_mode(BitFlags::from(CursorMode::Metadata))
                .types(SourceType::Monitor | SourceType::Window),
        )?;

        let request = RequestProxy::new(&connection, &request_handle)?;
        request.on_response(move |response: Response<BasicResponse>| {
            if response.is_ok() {
                self.start_cast(session_handle, proxy, connection).unwrap();
            }
        })?;
        Ok(())
    }

    fn start_cast(
        &self,
        session_handle: ObjectPath,
        proxy: &ScreenCastProxy,
        connection: &zbus::Connection,
    ) -> Result<()> {
        let request_handle = proxy.start(
            session_handle.clone(),
            WindowIdentifier::default(),
            StartCastOptions::default(),
        )?;
        let request = RequestProxy::new(&connection, &request_handle)?;
        request.on_response(move |r: Response<Streams>| {
            r.unwrap().streams().iter().for_each(|stream| {
                let node_id = stream.pipewire_node_id();
                let (width, height) = stream.properties().size;
                let fd = self
                    .open_pipewire_remote(session_handle.clone(), proxy)
                    .unwrap()
                    .as_raw_fd();

                let stream = Stream {
                    fd,
                    node_id,
                    screen: Screen { width, height },
                };

                self.emit_by_name("ready", &[&stream])
                    .expect("Failed to emit ready");
            });
        })?;
        Ok(())
    }

    fn open_pipewire_remote(
        &self,
        session_handle: ObjectPath,
        proxy: &ScreenCastProxy,
    ) -> Result<Fd> {
        proxy.open_pipe_wire_remote(session_handle, HashMap::default())
    }

    pub fn open(&self) {
        self.create_session().expect("Failed to create session");
    }

    pub fn close(&self) {}
}
