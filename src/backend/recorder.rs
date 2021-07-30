use gst::prelude::*;
use gtk::{
    glib::{self, clone, subclass::Signal, Continue, GBoxed, GEnum},
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::cell::{Cell, RefCell};

use crate::{
    backend::{PipelineBuilder, ScreencastPortal, ScreencastPortalResponse, Settings},
    data_types::Stream,
    utils,
    widgets::{AreaSelector, AreaSelectorResponse},
};

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderState")]
pub enum RecorderState {
    Null,
    Paused,
    Playing,
}

impl Default for RecorderState {
    fn default() -> Self {
        RecorderState::Null
    }
}

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "RecorderResponse")]
pub enum RecorderResponse {
    Success(String),
    Failed(String),
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Recorder {
        pub settings: Settings,
        pub portal: ScreencastPortal,
        pub pipeline: RefCell<Option<gst::Pipeline>>,
        pub state: RefCell<RecorderState>,
        pub is_readying: Cell<bool>,
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
                state: RefCell::new(RecorderState::default()),
                is_readying: Cell::new(false),
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
                        dbg!(&response);
                        match response {
                            ScreencastPortalResponse::Success(fd, node_id, screen) => {
                                let stream = Stream { fd, node_id, screen };
                                obj.init_pipeline(stream);
                            },
                            ScreencastPortalResponse::Revoked => {
                                // FIXME handle errors and cancelled
                                obj.emit_response(RecorderResponse::Failed("Cancelled session".into()));
                            }
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
                vec![
                    glib::ParamSpec::new_enum(
                        "state",
                        "state",
                        "State",
                        RecorderState::static_type(),
                        RecorderState::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_boolean(
                        "is-readying",
                        "is-readying",
                        "Is readying",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                ]
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
                "is-readying" => {
                    let is_readying = value.get().unwrap();
                    self.is_readying.set(is_readying);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => self.state.borrow().to_value(),
                "is-readying" => self.is_readying.get().to_value(),
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
        &imp::Recorder::from_instance(self)
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

    pub fn state(&self) -> RecorderState {
        self.property("state")
            .unwrap()
            .get::<RecorderState>()
            .unwrap()
    }

    fn set_state(&self, state: RecorderState) {
        self.set_property("state", state).unwrap();

        let pipeline = self.pipeline().unwrap();

        let new_pipeline_state = match state {
            RecorderState::Null => gst::State::Null,
            RecorderState::Paused => gst::State::Paused,
            RecorderState::Playing => gst::State::Playing,
        };

        pipeline.set_state(new_pipeline_state).unwrap();
        log::info!("Pipeline set to {:?}", new_pipeline_state);
    }

    fn set_is_readying(&self, is_readying: bool) {
        self.set_property("is-readying", is_readying).unwrap();
    }

    fn init_pipeline(&self, stream: Stream) {
        let settings = self.settings();

        let (speaker_source, mic_source) = utils::default_audio_sources();

        let pipeline_builder = PipelineBuilder::new()
            .pipewire_stream(stream)
            .framerate(settings.video_framerate())
            .file_path(settings.file_path())
            .record_speaker(settings.is_record_speaker())
            .record_mic(settings.is_record_mic())
            .speaker_source(speaker_source)
            .mic_source(mic_source);

        if settings.is_selection_mode() {
            let area_selector = AreaSelector::new();
            area_selector.select_area();
            area_selector.connect_local(
                "response",
                false,
                clone!(@weak self as obj, @strong pipeline_builder => @default-return None, move | args | {
                    let response = args[1].get().unwrap();
                    match response {
                        AreaSelectorResponse::Captured(coords, actual_screen) => {
                            let pipeline_builder = pipeline_builder.clone();

                            let pipeline = pipeline_builder
                                .coordinates(coords)
                                .actual_screen(actual_screen)
                                .build()
                                .unwrap();

                            obj.set_pipeline(Some(pipeline.downcast().unwrap()));
                            obj.emit_ready();
                        },
                        AreaSelectorResponse::Cancelled => {
                            obj.set_is_readying(false);
                            obj.portal().close_session();
                        },
                    }
                    None
                }),
            ).unwrap();
        } else {
            let pipeline = pipeline_builder.build().unwrap();
            self.set_pipeline(Some(pipeline.downcast().unwrap()));
            self.emit_ready();
        };

        // FIXME handle invalid pipeline errors
        // log::debug!("Pipeline: {}", pipeline_builder.clone().parse_into_string());
    }

    fn close_pipeline(&self) {
        self.set_state(RecorderState::Null);
        self.portal().close_session();
    }

    fn emit_response(&self, response: RecorderResponse) {
        self.emit_by_name("response", &[&response]).unwrap();
    }

    fn emit_ready(&self) {
        self.emit_by_name("ready", &[]).unwrap();
    }

    pub fn ready(&self) {
        self.set_is_readying(true);
        let is_show_pointer = self.settings().is_show_pointer();
        let is_selection_mode = self.settings().is_selection_mode();
        self.portal()
            .new_session(is_show_pointer, is_selection_mode);

        log::debug!("is_show_pointer: {}", is_show_pointer);
        log::debug!("is_selection_mode: {}", is_selection_mode);
    }

    pub fn start(&self) {
        let record_bus = self.pipeline().unwrap().bus().unwrap();
        record_bus.add_watch_local(clone!(@weak self as obj => @default-return Continue(true), move |_, message: &gst::Message| {
            match message.view() {
                gst::MessageView::Eos(..) => {
                    obj.close_pipeline();
                    obj.emit_response(RecorderResponse::Success("Done Recording".into()));
                },
                gst::MessageView::Error(error) => {
                    obj.close_pipeline();
                    obj.emit_response(RecorderResponse::Failed("Error recording".into()));
                    log::warn!("{}", error.debug().unwrap());
                },
                _ => (),
            }

            Continue(true)
        })).unwrap();

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
    }

    pub fn cancel(&self) {
        self.portal().close_session();
    }
}
