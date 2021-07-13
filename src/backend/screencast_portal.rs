use ashpd::{
    desktop::{screencast, SessionProxy},
    enumflags2::BitFlags,
    zbus, WindowIdentifier,
};
use futures::lock::Mutex;
use glib::clone;
use gtk::{
    glib::{self, GBoxed},
    prelude::*,
    subclass::prelude::*,
};

use std::os::unix::io::RawFd;
use std::sync::Arc;

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

    pub struct KhaScreencastPortal {
        pub session: Arc<Mutex<Option<SessionProxy<'static>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaScreencastPortal {
        const NAME: &'static str = "KhaScreencastPortal";
        type Type = super::KhaScreencastPortal;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                session: Arc::new(Mutex::new(None)),
            }
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
        glib::Object::new::<Self>(&[]).expect("Failed to initialize Portal object")
    }

    // fn create_session(&self) -> Result<()> {
    //     let connection = zbus::Connection::new_session()?;
    //     let proxy = ScreenCastProxy::new(&connection)?;

    //     let session_token = HandleToken::try_from("session120").unwrap();

    //     let request_handle = proxy
    //         .create_session(CreateSessionOptions::default().session_handle_token(session_token))?;
    //     let request = RequestProxy::new(&connection, &request_handle)?;

    //     request.on_response(|r: Response<CreateSession>| {
    //         match r {
    //             Ok(session) => self
    //                 .select_sources(session.handle(), &proxy, &connection)
    //                 .unwrap(),
    //             Err(_) => println!("hello!"),
    //         };
    //     })?;
    //     Ok(())
    // }

    // fn select_sources(
    //     &self,
    //     session_handle: ObjectPath,
    //     proxy: &ScreenCastProxy,
    //     connection: &zbus::Connection,
    // ) -> Result<()> {
    //     let request_handle = proxy.select_sources(
    //         session_handle.clone(),
    //         SelectSourcesOptions::default()
    //             .multiple(true)
    //             .cursor_mode(BitFlags::from(CursorMode::Metadata))
    //             .types(SourceType::Monitor | SourceType::Window),
    //     )?;

    //     let request = RequestProxy::new(&connection, &request_handle)?;
    //     request.on_response(move |response: Response<BasicResponse>| {
    //         if response.is_ok() {
    //             match self.start_cast(session_handle, proxy, connection) {
    //                 Ok(_) => (),
    //                 Err(error) => println!("Cancelled: {}", error),
    //             }
    //         }
    //     })?;
    //     Ok(())
    // }

    // fn start_cast(
    //     &self,
    //     session_handle: ObjectPath,
    //     proxy: &ScreenCastProxy,
    //     connection: &zbus::Connection,
    // ) -> Result<()> {
    //     let request_handle = proxy.start(
    //         session_handle.clone(),
    //         WindowIdentifier::default(),
    //         StartCastOptions::default(),
    //     )?;
    //     let request = RequestProxy::new(&connection, &request_handle)?;
    //     request.on_response(move |r: Response<Streams>| {
    //         r.unwrap().streams().iter().for_each(|stream| {
    //             let node_id = stream.pipewire_node_id();
    //             let (width, height) = stream.properties().size;
    //             let fd = self
    //                 .open_pipewire_remote(session_handle.clone(), proxy)
    //                 .unwrap()
    //                 .as_raw_fd();

    //             let stream = Stream {
    //                 fd,
    //                 node_id,
    //                 screen: Screen { width, height },
    //             };

    //             self.emit_by_name("ready", &[&stream])
    //                 .expect("Failed to emit ready");
    //         });
    //     })?;
    //     Ok(())
    // }

    // fn open_pipewire_remote(
    //     &self,
    //     session_handle: ObjectPath,
    //     proxy: &ScreenCastProxy,
    // ) -> Result<Fd> {
    //     proxy.open_pipe_wire_remote(session_handle, HashMap::default())
    // }

    pub fn open(&self) {
        let ctx = glib::MainContext::default();
        println!("starting session");
        ctx.spawn_local(clone!(@weak self as portal => async move {

            let imp = imp::KhaScreencastPortal::from_instance(&portal);

            let identifier = WindowIdentifier::default();
            let multiple = false;
            let types = screencast::SourceType::Monitor | screencast::SourceType::Window;
            let cursor_mode = BitFlags::<screencast::CursorMode>::from_flag(screencast::CursorMode::Embedded);


            match screencast(identifier, multiple, types, cursor_mode).await {
                Ok((streams, fd, session)) => {
                    streams.iter().for_each(|stream| {
                        println!("{:?} {:?}", stream, fd);

                    });

                    imp.session.lock().await.replace(session);
                }
                Err(err) => {
                    println!("{:#?}", err);
                }
            };
            println!("hiiiiiiiiiiii");
        }));
    }

    pub fn close(&self) {
        // let ctx = glib::MainContext::default();
        // ctx.spawn_local(clone!(@weak self as portal => async move {
        //     let imp = imp::KhaScreencastPortal::from_instance(&portal);
        //     if let Some(session) = imp.session.lock().await.take() {
        //         let _ = session.close().await;
        //     }
        // }));
    }
}

pub async fn screencast(
    window_identifier: WindowIdentifier,
    multiple: bool,
    types: BitFlags<screencast::SourceType>,
    cursor_mode: BitFlags<screencast::CursorMode>,
) -> Result<(Vec<screencast::Stream>, RawFd, SessionProxy<'static>), ashpd::Error> {
    let connection = zbus::azync::Connection::new_session().await?;
    let proxy = screencast::ScreenCastProxy::new(&connection).await?;

    let session = proxy.create_session().await?;

    proxy
        .select_sources(&session, cursor_mode, types, multiple)
        .await?;

    let streams = proxy.start(&session, window_identifier).await?.to_vec();

    let node_id = proxy.open_pipe_wire_remote(&session).await?;
    Ok((streams, node_id, session))
}
