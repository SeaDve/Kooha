use ashpd::desktop::screencast::Stream;
use gst::prelude::*;
use gtk::{
    glib::{self, clone, subclass::Signal, Continue, GBoxed, GEnum, SignalHandlerId, WeakRef},
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
    pactl,
    widgets::{AreaSelector, AreaSelectorResponse, MainWindow},
};

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "KoohaRecorderState")]
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
#[gboxed(type_name = "KoohaRecorderResponse")]
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
        pub settings: Settings,
        pub portal: ScreencastPortal,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Recorder {
        const NAME: &'static str = "KoohaRecorder";
        type Type = super::Recorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Cell::new(RecorderState::default()),

                pipeline: RefCell::new(None),
                settings: Settings::new(),
                portal: ScreencastPortal::new(),
            }
        }
    }

    impl ObjectImpl for Recorder {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("prepared", &[], <()>::static_type().into()).build(),
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

        let pipeline = match new_pipeline_state {
            gst::State::Null => {
                let imp = self.private();
                imp.pipeline.take().unwrap()
            }
            _ => self.pipeline().unwrap(),
        };

        if let Err(error) = pipeline.set_state(new_pipeline_state) {
            log::error!(
                "Failed to set pipeline state to {:?}: {}",
                new_pipeline_state,
                error
            );
        };
    }

    async fn init_pipeline(&self, streams: Vec<Stream>, fd: i32) {
        let imp = self.private();

        let pulse_server_version = pactl::server_version_info().unwrap_or_else(|| "None".into());
        log::debug!("pulse_server_version: {}", pulse_server_version);

        let (speaker_source, mic_source) = pactl::default_audio_devices_name();

        let pipeline_builder = PipelineBuilder::new()
            .record_speaker(imp.settings.is_record_speaker())
            .record_mic(imp.settings.is_record_mic())
            .framerate(imp.settings.video_framerate())
            .file_path(imp.settings.file_path())
            .fd(fd)
            .streams(streams)
            .speaker_source(speaker_source)
            .mic_source(mic_source);

        if !imp.settings.is_selection_mode() {
            self.build_pipeline(pipeline_builder);
            return;
        }

        let area_selector = AreaSelector::new();
        match area_selector.select_area().await {
            AreaSelectorResponse::Captured(coords, actual_screen) => {
                let pipeline_builder = pipeline_builder
                    .coordinates(coords)
                    .actual_screen(actual_screen);

                // Give area selector some time to disappear before setting up pipeline
                // to avoid it being included in the recording.
                glib::timeout_add_local_once(
                    Duration::from_millis(5),
                    clone!(@weak self as obj => move || {
                        obj.build_pipeline(pipeline_builder);
                    }),
                );

                log::info!("Captured coordinates");
            }
            AreaSelectorResponse::Cancelled => {
                self.portal().close_session();

                log::info!("Cancelled capture");
            }
        };
    }

    fn build_pipeline(&self, pipeline_builder: PipelineBuilder) {
        log::debug!("{:?}", &pipeline_builder);

        match pipeline_builder.build() {
            Ok(pipeline) => {
                self.set_pipeline(Some(pipeline));
                self.emit_prepared();
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

    fn emit_prepared(&self) {
        self.emit_by_name("prepared", &[]).unwrap();
    }

    fn parse_bus_message(&self, message: &gst::Message) -> Continue {
        match message.view() {
            gst::MessageView::Eos(_) => {
                let filesink = self.pipeline().unwrap().by_name("filesink").unwrap();
                let recording_file_path = filesink
                    .property("location")
                    .unwrap()
                    .get::<String>()
                    .unwrap()
                    .into();

                self.close_pipeline();
                self.emit_response(&RecorderResponse::Success(recording_file_path));
                log::info!("Eos signal received from record bus");

                Continue(false)
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

                Continue(false)
            }
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
                Continue(true)
            }
            _ => Continue(true),
        }
    }

    pub fn set_window(&self, window: WeakRef<MainWindow>) {
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

    pub fn connect_prepared<F: Fn(&[glib::Value]) -> Option<glib::Value> + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("prepared", false, f).unwrap()
    }

    pub fn prepare(&self) {
        let imp = self.private();

        let is_show_pointer = imp.settings.is_show_pointer();
        let is_selection_mode = imp.settings.is_selection_mode();

        log::debug!("is_show_pointer: {}", is_show_pointer);
        log::debug!("is_selection_mode: {}", is_selection_mode);

        let ctx = glib::MainContext::default();
        ctx.spawn_local(clone!(@weak self as obj => async move {
            match obj.portal().new_session(is_show_pointer, is_selection_mode).await {
                ScreencastPortalResponse::Success(streams, fd) => {
                    obj.init_pipeline(streams, fd).await;
                }
                ScreencastPortalResponse::Failed(error) => {
                    obj.emit_response(&RecorderResponse::Failed(error));
                }
                ScreencastPortalResponse::Cancelled => (),
            }
        }));
    }

    pub fn start(&self) {
        let record_bus = self.pipeline().unwrap().bus().unwrap();
        record_bus
            .add_watch_local(
                clone!(@weak self as obj => @default-return Continue(true), move |_, message| {
                    obj.parse_bus_message(message)
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
