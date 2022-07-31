use ashpd::desktop::screencast::Stream;
use gst::prelude::*;
use gtk::{
    glib::{self, clone},
    subclass::prelude::*,
};

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    time::Duration,
};

use crate::{
    application::Application,
    area_selector::{AreaSelector, AreaSelectorResponse},
    backend::{PipelineBuilder, ScreencastPortal, ScreencastPortalResponse},
    error::Error,
    pactl,
};

#[derive(Debug, PartialEq, Clone, Copy, glib::Enum)]
#[enum_type(name = "KoohaRecorderState")]
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

#[derive(Debug, Clone, glib::Boxed)]
#[boxed_type(name = "KoohaRecorderResponse")]
pub enum RecorderResponse {
    Success(PathBuf),
    Failed(Error),
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct Recorder {
        pub state: Cell<RecorderState>,

        pub pipeline: RefCell<Option<gst::Pipeline>>,
        pub portal: ScreencastPortal,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Recorder {
        const NAME: &'static str = "KoohaRecorder";
        type Type = super::Recorder;
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
                vec![glib::ParamSpecEnum::new(
                    "state",
                    "state",
                    "Current state of Self",
                    RecorderState::static_type(),
                    RecorderState::default() as i32,
                    glib::ParamFlags::READABLE,
                )]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => obj.state().to_value(),
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

    pub fn state(&self) -> RecorderState {
        self.imp().state.get()
    }

    pub fn connect_state_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("state"), move |obj, _| f(obj))
    }

    pub fn connect_response<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &RecorderResponse) + 'static,
    {
        self.connect_local("response", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let response = values[1].get::<RecorderResponse>().unwrap();
            f(&obj, &response);
            None
        })
    }

    pub fn connect_prepared<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_local("prepared", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            f(&obj);
            None
        })
    }

    pub fn prepare(&self) {
        let settings = Application::default().settings();

        let is_show_pointer = settings.is_show_pointer();
        let is_selection_mode = settings.is_selection_mode();

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

    pub fn cancel_prepare(&self) {
        self.imp().pipeline.take();
        self.portal().close_session();
    }

    pub fn start(&self) {
        let record_bus = self.pipeline().unwrap().bus().unwrap();
        record_bus
            .add_watch_local(
                clone!(@weak self as obj => @default-return Continue(false), move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();

        self.set_state(RecorderState::Playing).unwrap();
    }

    pub fn pause(&self) {
        self.set_state(RecorderState::Paused).unwrap();
    }

    pub fn resume(&self) {
        self.set_state(RecorderState::Playing).unwrap();
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
                    obj.set_state(RecorderState::Flushing).unwrap();
                }
            }),
        );
    }

    fn set_state(&self, state: RecorderState) -> anyhow::Result<()> {
        let pipeline = self
            .pipeline()
            .ok_or_else(|| anyhow::anyhow!("Pipeline not found"))?;

        match state {
            RecorderState::Null => {
                pipeline.set_state(gst::State::Null)?;
                log::info!("Player state changed to Stopped");

                // Changing the state to NULL flushes the pipeline.
                // Thus, the change message never arrives.
                self.imp().state.set(state);
                self.notify("state");
            }
            RecorderState::Flushing => {
                self.imp().state.set(state);
                self.notify("state");
            }
            RecorderState::Paused => {
                pipeline.set_state(gst::State::Paused)?;
            }
            RecorderState::Playing => {
                pipeline.set_state(gst::State::Playing)?;
            }
        }

        Ok(())
    }

    fn portal(&self) -> &ScreencastPortal {
        &self.imp().portal
    }

    fn pipeline(&self) -> Option<gst::Pipeline> {
        self.imp().pipeline.borrow().clone()
    }

    async fn init_pipeline(&self, streams: Vec<Stream>, fd: i32) {
        let settings = Application::default().settings();

        let pulse_server_version = pactl::server_version_info().unwrap_or_else(|| "None".into());
        log::debug!("pulse_server_version: {}", pulse_server_version);

        let (speaker_source, mic_source) = pactl::default_audio_devices_name();

        let pipeline_builder = PipelineBuilder::new()
            .record_speaker(settings.is_record_speaker())
            .record_mic(settings.is_record_mic())
            .framerate(settings.video_framerate())
            .file_path(settings.file_path())
            .fd(fd)
            .streams(streams)
            .speaker_source(speaker_source)
            .mic_source(mic_source);

        if !settings.is_selection_mode() {
            self.build_pipeline(pipeline_builder);
            return;
        }

        let area_selector = AreaSelector::new();
        match area_selector.select_area().await {
            AreaSelectorResponse::Captured(coords, actual_screen) => {
                let pipeline_builder = pipeline_builder
                    .coordinates(coords)
                    .actual_screen(actual_screen);

                // Give area selector some time to disappear before building pipeline
                // to avoid it being included in the recording.
                glib::timeout_future(Duration::from_millis(150)).await;
                self.build_pipeline(pipeline_builder);

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
                self.imp().pipeline.replace(Some(pipeline));
                self.emit_by_name::<()>("prepared", &[]);
            }
            Err(error) => {
                log::error!("Failed to build pipeline: {:?}", &error);

                self.portal().close_session();
                self.emit_response(&RecorderResponse::Failed(Error::Pipeline(error)));
            }
        };
    }

    fn close_pipeline(&self) {
        self.set_state(RecorderState::Null).unwrap();
        self.portal().close_session();
    }

    fn emit_response(&self, response: &RecorderResponse) {
        self.emit_by_name::<()>("response", &[response]);
    }

    fn handle_bus_message(&self, message: &gst::Message) -> Continue {
        match message.view() {
            gst::MessageView::Eos(_) => self.on_bus_eos(),
            gst::MessageView::Error(ref message) => self.on_bus_error(message),
            gst::MessageView::StateChanged(ref message) => self.on_state_changed(message),
            _ => Continue(true),
        }
    }

    fn on_bus_eos(&self) -> Continue {
        let filesink = self.pipeline().unwrap().by_name("filesink").unwrap();
        let recording_file_path = filesink.property::<String>("location").into();

        self.close_pipeline();
        self.emit_response(&RecorderResponse::Success(recording_file_path));
        log::info!("Eos signal received from record bus");

        Continue(false)
    }

    fn on_bus_error(&self, message: &gst::message::Error<'_>) -> Continue {
        log::error!(
            "Error from record bus: {:?} (debug {:?})",
            message.error(),
            message
        );

        self.close_pipeline();
        self.emit_response(&RecorderResponse::Failed(Error::Recorder(message.error())));

        Continue(false)
    }

    fn on_state_changed(&self, message: &gst::message::StateChanged<'_>) -> Continue {
        if message.src().as_ref() != Some(self.pipeline().unwrap().upcast_ref::<gst::Object>()) {
            return Continue(true);
        }

        let old_state = message.old();
        let new_state = message.current();

        log::info!(
            "Recorder state changed: `{:?}` -> `{:?}`",
            old_state,
            new_state
        );

        let state = match new_state {
            gst::State::Null => RecorderState::Null,
            gst::State::Paused => RecorderState::Paused,
            gst::State::Playing => RecorderState::Playing,
            _ => return Continue(true),
        };

        self.imp().state.set(state);
        self.notify("state");

        Continue(true)
    }
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}
