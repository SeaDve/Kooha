use ashpd::desktop::screencast::Stream;
use gst::prelude::*;
use gtk::{
    glib::{self, clone, subclass::Signal, Continue, GBoxed, GEnum},
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::{cell::RefCell, path::PathBuf};

use crate::{
    backend::{PipelineBuilder, ScreencastPortal, ScreencastPortalResponse, Settings},
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
    Failed(String),
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Recorder {
        pub settings: Settings,
        pub portal: ScreencastPortal,
        pub pipeline: RefCell<Option<gst::Pipeline>>,
        pub current_file_path: RefCell<Option<PathBuf>>,
        pub state: RefCell<RecorderState>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Recorder {
        const NAME: &'static str = "Recorder";
        type Type = super::Recorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                settings: Settings::new(),
                portal: ScreencastPortal::new(),
                pipeline: RefCell::new(None),
                current_file_path: RefCell::new(None),
                state: RefCell::new(RecorderState::default()),
            }
        }
    }

    impl ObjectImpl for Recorder {
        fn constructed(&self, obj: &Self::Type) {
            self.portal
                .connect_local(
                    "response",
                    false,
                    clone!(@weak obj => @default-return None, move | args | {
                        let response = args[1].get().unwrap();
                        match response {
                            ScreencastPortalResponse::Success(streams, fd) => {
                                obj.init_pipeline(streams, fd);
                            },
                            ScreencastPortalResponse::Error(error_message) => {
                                obj.emit_response(&RecorderResponse::Failed(error_message));
                            }
                            ScreencastPortalResponse::Cancelled => (),
                        };
                        None
                    }),
                )
                .unwrap();
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
                    self.state.replace(state);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => self.state.borrow().to_value(),
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
        glib::Object::new::<Self>(&[]).expect("Failed to create Recorder")
    }

    fn private(&self) -> &imp::Recorder {
        imp::Recorder::from_instance(self)
    }

    fn portal(&self) -> &ScreencastPortal {
        let imp = self.private();
        &imp.portal
    }

    fn settings(&self) -> &Settings {
        let imp = self.private();
        &imp.settings
    }

    fn pipeline(&self) -> Option<gst::Pipeline> {
        let imp = self.private();
        imp.pipeline.borrow().clone()
    }

    fn set_pipeline(&self, new_pipeline: Option<gst::Pipeline>) {
        let imp = self.private();
        imp.pipeline.replace(new_pipeline);
    }

    fn current_file_path(&self) -> Option<PathBuf> {
        let imp = self.private();
        imp.current_file_path.take()
    }

    fn set_current_file_path(&self, file_path: Option<PathBuf>) {
        let imp = self.private();
        imp.current_file_path.replace(file_path);
    }

    pub fn state(&self) -> RecorderState {
        self.property("state")
            .unwrap()
            .get::<RecorderState>()
            .unwrap()
    }

    fn set_state(&self, state: RecorderState) {
        self.set_property("state", state).unwrap();

        if state == RecorderState::Flushing {
            return;
        }

        let new_pipeline_state = match state {
            RecorderState::Null => gst::State::Null,
            RecorderState::Paused => gst::State::Paused,
            RecorderState::Playing => gst::State::Playing,
            RecorderState::Flushing => unreachable!(),
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
        let settings = self.settings();

        let (speaker_source, mic_source) = utils::default_audio_sources();
        let file_path = settings.file_path();
        self.set_current_file_path(Some(file_path.clone()));

        let pipeline_builder = PipelineBuilder::new()
            .streams(streams)
            .fd(fd)
            .framerate(settings.video_framerate())
            .file_path(file_path)
            .record_speaker(settings.is_record_speaker())
            .record_mic(settings.is_record_mic())
            .speaker_source(speaker_source)
            .mic_source(mic_source);

        if !settings.is_selection_mode() {
            self.setup_pipeline(pipeline_builder);
            return;
        }

        let area_selector = AreaSelector::new();
        area_selector.connect_local(
                "response",
                false,
                clone!(@weak self as obj, @strong pipeline_builder => @default-return None, move |args| {
                    let response = args[1].get().unwrap();
                    match response {
                        AreaSelectorResponse::Captured(coords, actual_screen) => {
                            let pipeline_builder = pipeline_builder.clone()
                                .coordinates(coords)
                                .actual_screen(actual_screen);

                            obj.setup_pipeline(pipeline_builder);

                            log::info!("Captured coordinates");
                        },
                        AreaSelectorResponse::Cancelled => {
                            obj.portal().close_session();

                            log::info!("Cancelled capture");
                        },
                    }
                    None
                }),
            ).unwrap();
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
                self.portal().close_session();
                self.emit_response(&RecorderResponse::Failed(error.to_string()));
                log::error!("Failed to build pipeline: {}", error);
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

    pub fn set_window(&self, window: &MainWindow) {
        self.portal().set_window(window);
    }

    pub fn ready(&self) {
        let is_show_pointer = self.settings().is_show_pointer();
        let is_selection_mode = self.settings().is_selection_mode();
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
                    match message.view() {
                        gst::MessageView::StateChanged(sc) => {
                            if message.src().as_ref() == Some(obj.pipeline().unwrap().upcast_ref::<gst::Object>()) {
                                log::info!("Pipeline state set from {:?} -> {:?}", sc.old(), sc.current());
                            }
                        },
                        gst::MessageView::Eos(_) => {
                            obj.close_pipeline();
                            let recording_file_path = obj.current_file_path().unwrap();
                            obj.emit_response(&RecorderResponse::Success(recording_file_path));

                            log::info!("Eos signal received from record bus");
                        },
                        gst::MessageView::Error(error) => {
                            let error_message = error.error().to_string();

                            if let Some(debug) = error.debug() {
                                log::error!("Error from record bus: {} (debug {})", error_message, debug);
                            } else {
                                log::error!("Error from record bus: {}", error_message);
                            };

                            obj.close_pipeline();
                            obj.emit_response(&RecorderResponse::Failed(error_message));
                        },
                        _ => (),
                    }

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
        self.set_state(RecorderState::Flushing);
        self.pipeline().unwrap().send_event(gst::event::Eos::new());

        log::info!("Sending eos event to pipeline");
    }

    pub fn cancel(&self) {
        self.portal().close_session();
    }
}
