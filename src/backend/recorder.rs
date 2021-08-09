use ashpd::desktop::screencast::Stream;
use gst::prelude::*;
use gtk::{
    glib::{self, clone, subclass::Signal, Continue, GBoxed, GEnum, SignalHandlerId},
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    time::Duration,
};

use crate::{
    backend::{PipelineBuilder, ScreencastPortal, ScreencastPortalResponse, Settings},
    error::Error,
    utils,
    widgets::{AreaSelector, AreaSelectorResponse, MainWindow},
};

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderState")]
pub enum RecorderState {
    Null,
    Paused,
    Playing,
    Flushing,
}

impl Default for RecorderState {
    fn default() -> Self {
        Self::Null
    }
}

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "RecorderResponse")]
pub enum RecorderResponse {
    Success(PathBuf),
    Failed(Error),
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Recorder {
        pub state: Cell<RecorderState>,

        pub pipeline: RefCell<Option<gst::Pipeline>>,
        pub current_file_path: RefCell<Option<PathBuf>>,
        pub settings: Settings,
        pub portal: ScreencastPortal,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Recorder {
        const NAME: &'static str = "Recorder";
        type Type = super::Recorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Cell::new(RecorderState::default()),

                pipeline: RefCell::new(None),
                current_file_path: RefCell::new(None),
                settings: Settings::new(),
                portal: ScreencastPortal::new(),
            }
        }
    }

    impl ObjectImpl for Recorder {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.setup_signals();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("ready", &[], <()>::static_type().into()).build(),
                    Signal::builder(
                        "response",
                        &[RecorderResponse::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpec::new_enum(
                    "state",
                    "state",
                    "Current state of Self",
                    RecorderState::static_type(),
                    RecorderState::default() as i32,
                    glib::ParamFlags::READWRITE,
                )]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "state" => {
                    let state = value.get().unwrap();
                    self.state.set(state);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => self.state.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Recorder(ObjectSubclass<imp::Recorder>);
}

impl Recorder {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create Recorder.")
    }

    fn private(&self) -> &imp::Recorder {
        imp::Recorder::from_instance(self)
    }

    fn setup_signals(&self) {
        let imp = self.private();

        imp.portal.connect_response(
            clone!(@weak self as obj => @default-return None, move | args | {
                let response = args[1].get().unwrap();
                match response {
                    ScreencastPortalResponse::Success(streams, fd) => {
                        obj.init_pipeline(streams, fd);
                    },
                    ScreencastPortalResponse::Failed(error_message) => {
                        obj.emit_response(&RecorderResponse::Failed(error_message));
                    }
                    ScreencastPortalResponse::Cancelled => (),
                };
                None
            }),
        );
    }

    fn portal(&self) -> &ScreencastPortal {
        let imp = self.private();
        &imp.portal
    }

    fn pipeline(&self) -> Option<gst::Pipeline> {
        let imp = self.private();
        imp.pipeline.borrow().clone()
    }

    fn set_pipeline(&self, new_pipeline: Option<gst::Pipeline>) {
        let imp = self.private();
        imp.pipeline.replace(new_pipeline);
    }

    pub fn state(&self) -> RecorderState {
        self.property("state")
            .unwrap()
            .get::<RecorderState>()
            .unwrap()
    }

    fn set_state(&self, state: RecorderState) {
        self.set_property("state", state).unwrap();

        let new_pipeline_state = match state {
            RecorderState::Null => gst::State::Null,
            RecorderState::Paused => gst::State::Paused,
            RecorderState::Playing => gst::State::Playing,
            RecorderState::Flushing => return,
        };

        let pipeline = self.pipeline().unwrap();
        if let Err(error) = pipeline.set_state(new_pipeline_state) {
            log::error!(
                "Failed to set pipeline state to {:?}: {}",
                new_pipeline_state,
                error
            );
        };
    }

    fn init_pipeline(&self, streams: Vec<Stream>, fd: i32) {
        let imp = self.private();

        let (speaker_source, mic_source) = utils::default_audio_sources();
        let file_path = imp.settings.file_path();
        imp.current_file_path.replace(Some(file_path.clone()));

        let pipeline_builder = PipelineBuilder::new()
            .streams(streams)
            .fd(fd)
            .framerate(imp.settings.video_framerate())
            .file_path(file_path)
            .record_speaker(imp.settings.is_record_speaker())
            .record_mic(imp.settings.is_record_mic())
            .speaker_source(speaker_source)
            .mic_source(mic_source);

        if !imp.settings.is_selection_mode() {
            self.setup_pipeline(pipeline_builder);
            return;
        }

        let area_selector = AreaSelector::new();
        area_selector.connect_response(
            clone!(@weak self as obj => @default-return None, move |args| {
                let response = args[1].get().unwrap();
                match response {
                    AreaSelectorResponse::Captured(coords, actual_screen) => {
                        let pipeline_builder = pipeline_builder.clone()
                            .coordinates(coords)
                            .actual_screen(actual_screen);

                        // Give area selector some time to disappear before setting up pipeline
                        // to avoid it being included in the recording.
                        glib::timeout_add_local_once(Duration::from_millis(5), move || {
                            obj.setup_pipeline(pipeline_builder);
                        });

                        log::info!("Captured coordinates");
                    },
                    AreaSelectorResponse::Cancelled => {
                        obj.portal().close_session();

                        log::info!("Cancelled capture");
                    },
                }
                None
            }),
        );
        area_selector.select_area();
    }

    fn setup_pipeline(&self, pipeline_builder: PipelineBuilder) {
        log::debug!("{:?}", &pipeline_builder);

        match pipeline_builder.build() {
            Ok(pipeline) => {
                self.set_pipeline(Some(pipeline.downcast().unwrap()));
                self.emit_ready();
            }
            Err(error) => {
                log::error!("Failed to build pipeline: {}", &error);

                self.portal().close_session();
                self.emit_response(&RecorderResponse::Failed(Error::Pipeline(error)));
            }
        };
    }

    fn close_pipeline(&self) {
        self.set_state(RecorderState::Null);
        self.portal().close_session();
    }

    fn emit_response(&self, response: &RecorderResponse) {
        self.emit_by_name("response", &[response]).unwrap();
    }

    fn emit_ready(&self) {
        self.emit_by_name("ready", &[]).unwrap();
    }

    fn parse_bus_message(&self, message: &gst::Message) {
        let imp = self.private();

        match message.view() {
            gst::MessageView::StateChanged(sc) => {
                if message.src().as_ref()
                    == Some(self.pipeline().unwrap().upcast_ref::<gst::Object>())
                {
                    log::info!(
                        "Pipeline state set from {:?} -> {:?}",
                        sc.old(),
                        sc.current()
                    );
                }
            }
            gst::MessageView::Eos(_) => {
                self.close_pipeline();
                let recording_file_path = imp.current_file_path.take().unwrap();
                self.emit_response(&RecorderResponse::Success(recording_file_path));

                log::info!("Eos signal received from record bus");
            }
            gst::MessageView::Error(error) => {
                let error_message = error.error().to_string();

                if let Some(debug) = error.debug() {
                    log::error!("Error from record bus: {} (debug {})", error_message, debug);
                } else {
                    log::error!("Error from record bus: {}", error_message);
                };

                self.close_pipeline();
                self.emit_response(&RecorderResponse::Failed(Error::Recorder(error.error())));
            }
            _ => (),
        }
    }

    pub fn set_window(&self, window: &MainWindow) {
        self.portal().set_window(window);
    }

    pub fn connect_state_notify<F: Fn(&Self, &glib::ParamSpec) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_notify_local(Some("state"), f)
    }

    pub fn connect_response<F: Fn(&[glib::Value]) -> Option<glib::Value> + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("response", false, f).unwrap()
    }

    pub fn connect_ready<F: Fn(&[glib::Value]) -> Option<glib::Value> + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("ready", false, f).unwrap()
    }

    pub fn ready(&self) {
        let imp = self.private();

        let is_show_pointer = imp.settings.is_show_pointer();
        let is_selection_mode = imp.settings.is_selection_mode();
        self.portal()
            .new_session(is_show_pointer, is_selection_mode);

        log::debug!("is_show_pointer: {}", is_show_pointer);
        log::debug!("is_selection_mode: {}", is_selection_mode);
    }

    pub fn start(&self) {
        let record_bus = self.pipeline().unwrap().bus().unwrap();
        record_bus
            .add_watch_local(
                clone!(@weak self as obj => @default-return Continue(true), move |_, message| {
                    obj.parse_bus_message(message);
                    Continue(true)
                }),
            )
            .unwrap();

        self.set_state(RecorderState::Playing);
    }

    pub fn pause(&self) {
        self.set_state(RecorderState::Paused);
    }

    pub fn resume(&self) {
        self.set_state(RecorderState::Playing);
    }

    pub fn stop(&self) {
        self.pipeline().unwrap().send_event(gst::event::Eos::new());
        log::info!("Sending eos event to pipeline");

        // Wait 120ms before showing flushing state to avoid showing the flushing screen even though
        // it will only process for a few ms. If the pipeline state is still not null after 120ms,
        // then show the flushing state.
        glib::timeout_add_local_once(
            Duration::from_millis(120),
            clone!(@weak self as obj => move || {
                if obj.state() != RecorderState::Null {
                    obj.set_state(RecorderState::Flushing);
                }
            }),
        );
    }

    pub fn cancel(&self) {
        self.portal().close_session();
    }
}
