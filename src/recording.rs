use ashpd::{
    desktop::screencast::{CursorMode, PersistMode, SourceType},
    enumflags2::BitFlags,
};
use error_stack::{IntoReport, Report, Result, ResultExt};
use gettextrs::gettext;
use gst::prelude::*;
use gtk::glib::{self, clone, subclass::prelude::*};
use once_cell::{sync::Lazy, unsync::OnceCell};

use std::{
    cell::{Cell, RefCell},
    fmt,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use crate::{
    area_selector::{AreaSelector, Response as AreaSelectorResponse},
    audio_device::{self, Class as AudioDeviceClass},
    clock_time::ClockTime,
    help::ResultExt as HelpResultExt,
    pipeline_builder::PipelineBuilder,
    screencast_session::{ScreencastSession, ScreencastSessionError},
    settings::CaptureMode,
    timer::Timer,
    utils, Application,
};

static PORTAL_ERROR_HELP: Lazy<String> = Lazy::new(|| {
    gettext("Make sure to check for the runtime dependencies and <a href=\"https://github.com/SeaDve/Kooha#-it-doesnt-work\">It Doesn't Work page</a>.")
});
static PIPELINE_ERROR_HELP: Lazy<String> = Lazy::new(|| {
    gettext("A GStreamer plugin may not be installed. If it is installed but still does not work properly, please report to <a href=\"https://github.com/SeaDve/Kooha/issues\">Kooha's issue page</a>.")
});
static WRITE_ERROR_HELP: Lazy<String> =
    Lazy::new(|| gettext("Make sure that the saving location exists or is accessible."));

#[derive(Debug, Default, Clone, glib::Boxed)]
#[boxed_type(name = "KoohaRecordingState")]
pub enum RecordingState {
    #[default]
    Null,
    Delayed {
        secs_left: u64,
    },
    Recording,
    Paused,
    Flushing,
    Finished(Rc<Result<PathBuf, RecordingError>>),
}

impl RecordingState {
    fn finished_ok(val: PathBuf) -> Self {
        Self::Finished(Rc::new(Ok(val)))
    }

    fn finished_err(err: Report<RecordingError>) -> Self {
        Self::Finished(Rc::new(Err(err)))
    }

    fn eq_variant(&self, rhs: &Self) -> bool {
        match (self, rhs) {
            (Self::Null, Self::Null) => true,
            (
                Self::Delayed { secs_left },
                Self::Delayed {
                    secs_left: rhs_secs_left,
                },
            ) => secs_left == rhs_secs_left,
            (Self::Recording, Self::Recording) => true,
            (Self::Paused, Self::Paused) => true,
            (Self::Flushing, Self::Flushing) => true,
            (Self::Finished(_), Self::Finished(_)) => true,
            _ => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    NoDeviceFound(AudioDeviceClass),
    Cancelled(String),
    Other,
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordingError::NoDeviceFound(class) => match class {
                AudioDeviceClass::Sink => f.write_str(&gettext("No desktop speaker source found")),
                AudioDeviceClass::Source => f.write_str(&gettext("No microphone found")),
            },
            RecordingError::Cancelled(task) => f.write_str(&gettext!("Cancelled {}", task)),
            RecordingError::Other => f.write_str(&gettext("Something went wrong")),
        }
    }
}

impl RecordingError {
    pub fn cancelled(task_name: &str) -> Report<Self> {
        Report::new(RecordingError::Cancelled(format!(
            "Cancelled {}",
            task_name
        )))
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Recording {
        pub(super) timer: RefCell<Option<Timer>>,
        pub(super) session: RefCell<Option<ScreencastSession>>,
        pub(super) duration_source_id: RefCell<Option<glib::SourceId>>,
        pub(super) pipeline: OnceCell<gst::Pipeline>,

        pub(super) state: RefCell<RecordingState>,
        pub(super) duration: Cell<ClockTime>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Recording {
        const NAME: &'static str = "KoohaRecording";
        type Type = super::Recording;
    }

    impl ObjectImpl for Recording {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecBoxed::builder("state", RecordingState::static_type())
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                    glib::ParamSpecBoxed::builder("duration", ClockTime::static_type())
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => obj.state().to_value(),
                "duration" => obj.duration().to_value(),
                _ => unimplemented!(),
            }
        }

        fn dispose(&self, _obj: &Self::Type) {
            if let Some(source_id) = self.duration_source_id.take() {
                source_id.remove();
            }

            if let Some(pipeline) = self.pipeline.get() {
                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::warn!("Failed to stop pipeline on dispose: {:?}", err);
                }
            }
        }
    }
}

glib::wrapper! {
     pub struct Recording(ObjectSubclass<imp::Recording>);
}

impl Recording {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create Recording.")
    }

    pub async fn start(&self, delay: Duration) {
        if !matches!(self.state(), RecordingState::Null) {
            tracing::error!("Trying to start recording on a non-null state");
            return;
        }

        if let Err(err) = self.start_inner(delay).await {
            if let Some(session) = self.imp().session.take() {
                if let Err(err) = session.close().await {
                    tracing::warn!(
                        "Failed to close session on failed to start recording: {:?}",
                        err
                    );
                };
            }

            self.set_state(RecordingState::finished_err(err));
        }
    }

    async fn start_inner(&self, delay: Duration) -> Result<(), RecordingError> {
        let imp = self.imp();

        let settings = Application::default().settings();

        // setup screencast session
        let screencast_session = ScreencastSession::new()
            .await
            .change_context(RecordingError::Other)
            .attach_help(&PORTAL_ERROR_HELP)?;
        tracing::debug!(
            "Available cursor modes: {:?}",
            screencast_session.available_cursor_modes().await
        );
        tracing::debug!(
            "Available source types: {:?}",
            screencast_session.available_source_types().await
        );
        let (streams, restore_token, fd) = match screencast_session
            .start(
                if settings.show_pointer() {
                    BitFlags::<CursorMode>::from_flag(CursorMode::Embedded)
                } else {
                    BitFlags::<CursorMode>::from_flag(CursorMode::Hidden)
                },
                if settings.capture_mode() == CaptureMode::Selection {
                    BitFlags::<SourceType>::from_flag(SourceType::Monitor)
                } else {
                    SourceType::Monitor | SourceType::Window
                },
                settings.capture_mode() == CaptureMode::MonitorWindow,
                Some(&settings.screencast_restore_token()),
                PersistMode::DoNot,
                Application::default().main_window().as_ref(),
            )
            .await
        {
            Ok(val) => val,
            Err(err) => {
                if matches!(err.current_context(), ScreencastSessionError::Cancelled) {
                    return Err(RecordingError::cancelled("screencast session"));
                }

                return Err(err
                    .change_context(RecordingError::Other)
                    .attach_printable("Failed to start screencast session"))
                .attach_help(&PORTAL_ERROR_HELP)?;
            }
        };
        imp.session.replace(Some(screencast_session));
        settings.set_screencast_restore_token(&restore_token.unwrap_or_default());

        // select area
        let mut pipeline_builder = PipelineBuilder::new(
            settings.video_framerate(),
            &settings.saving_location(),
            settings.video_format(),
            fd,
            streams,
        );
        if settings.capture_mode() == CaptureMode::Selection {
            match AreaSelector::select_area().await {
                AreaSelectorResponse::Ok { selection, screen } => {
                    pipeline_builder
                        .coordinates(selection)
                        .actual_screen(screen);
                }
                AreaSelectorResponse::Cancelled => {
                    return Err(RecordingError::cancelled("area selection"));
                }
            }
        }

        // setup timer
        let timer = Timer::new(
            delay,
            clone!(@weak self as obj => move |secs_left| {
                obj.set_state(RecordingState::Delayed {
                    secs_left
                });
            }),
        );
        imp.timer.replace(Some(Timer::clone(&timer)));
        let timer_res = timer.await;
        self.set_state(RecordingState::Null);
        if timer_res.is_cancelled() {
            return Err(RecordingError::cancelled("timer"));
        }

        // setup audio sources
        if settings.record_mic() {
            pipeline_builder.mic_source(
                audio_device::find_default_name(AudioDeviceClass::Source)
                    .await
                    .change_context(RecordingError::NoDeviceFound(AudioDeviceClass::Source))?,
            );
        }
        if settings.record_speaker() {
            pipeline_builder.speaker_source(
                audio_device::find_default_name(AudioDeviceClass::Sink)
                    .await
                    .change_context(RecordingError::NoDeviceFound(AudioDeviceClass::Sink))?,
            );
        }

        // build pipeline
        let pipeline = pipeline_builder
            .build()
            .change_context(RecordingError::Other)
            .attach_printable("failed to build pipeline")
            .attach_help(&PIPELINE_ERROR_HELP)?;
        imp.pipeline.set(pipeline.clone()).unwrap();
        pipeline
            .bus()
            .unwrap()
            .add_watch_local(
                clone!(@weak self as obj => @default-return Continue(false), move |_, message|  {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();
        imp.duration_source_id.replace(Some(glib::timeout_add_local(
            Duration::from_millis(200),
            clone!(@weak self as obj => @default-return Continue(false), move || {
                obj.update_duration();
                Continue(true)
            }),
        )));
        pipeline
            .set_state(gst::State::Playing)
            .report()
            .change_context(RecordingError::Other)
            .attach_printable("failed to set pipeline state to playing")?;
        // TODO Add preparing state
        self.update_duration();

        Ok(())
    }

    pub fn pause(&self) -> Result<(), RecordingError> {
        if !matches!(self.state(), RecordingState::Recording) {
            return Err(Report::new(RecordingError::Other))
                .attach_printable("recording can only be paused from recording state");
        }

        self.pipeline()
            .set_state(gst::State::Paused)
            .report()
            .change_context(RecordingError::Other)
            .attach_printable("failed to set pipeline state to paused")?;

        Ok(())
    }

    pub fn resume(&self) -> Result<(), RecordingError> {
        if matches!(self.state(), RecordingState::Paused) {
            return Err(Report::new(RecordingError::Other))
                .attach_printable("recording can only be resumed from paused state");
        }

        self.pipeline()
            .set_state(gst::State::Playing)
            .report()
            .change_context(RecordingError::Other)
            .attach_printable("failed to set pipeline state to playing from paused")?;

        Ok(())
    }

    pub async fn stop(&self) {
        let state = self.state();

        if matches!(
            state,
            RecordingState::Null | RecordingState::Flushing | RecordingState::Finished(_)
        ) {
            tracing::error!("Trying to stop recording on a `{:?}` state", state);
            return;
        }

        if let Err(err) = self.stop_inner() {
            if let Some(session) = self.imp().session.take() {
                if let Err(err) = session.close().await {
                    tracing::warn!(
                        "Failed to close session on failed to start recording: {:?}",
                        err
                    );
                };
            }

            self.set_state(RecordingState::finished_err(err));
        }
    }

    fn stop_inner(&self) -> Result<(), RecordingError> {
        tracing::info!("Sending eos event to pipeline");
        self.pipeline().send_event(gst::event::Eos::new());
        self.set_state(RecordingState::Flushing);

        Ok(())
    }

    pub async fn cancel(&self) {
        let imp = self.imp();

        tracing::info!("Cancelling recording");

        if let Some(timer) = imp.timer.take() {
            timer.cancel();
        }

        if let Some(pipeline) = imp.pipeline.get() {
            if let Err(err) = pipeline.set_state(gst::State::Null) {
                tracing::warn!("Failed to stop pipeline on cancel: {err:?}");
            }

            let _ = pipeline.bus().unwrap().remove_watch();
        }

        if let Some(session) = imp.session.take() {
            if let Err(err) = session.close().await {
                tracing::warn!("Failed to close screencast session on cancel: {err:?}");
            }
        }

        if let Some(source_id) = imp.duration_source_id.take() {
            source_id.remove();
        }

        self.set_state(RecordingState::finished_err(RecordingError::cancelled(
            "manually",
        )));

        // TODO delete recorded file
    }

    pub fn state(&self) -> RecordingState {
        self.imp().state.borrow().clone()
    }

    pub fn connect_state_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("state"), move |obj, _| f(obj))
    }

    pub fn duration(&self) -> ClockTime {
        self.imp().duration.get()
    }

    pub fn connect_duration_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("duration"), move |obj, _| f(obj))
    }

    fn set_state(&self, state: RecordingState) {
        if state.eq_variant(&self.state()) {
            return;
        }

        self.imp().state.replace(state);
        self.notify("state");
    }

    fn pipeline(&self) -> &gst::Pipeline {
        self.imp()
            .pipeline
            .get()
            .expect("pipeline not set, make sure to start recording first")
    }

    fn update_duration(&self) {
        let clock_time = self
            .imp()
            .pipeline
            .get()
            .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
            .unwrap_or(gst::ClockTime::ZERO);

        self.imp().duration.set(clock_time.into());
        self.notify("duration");
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::Continue {
        use gst::MessageView;

        let imp = self.imp();

        match message.view() {
            MessageView::Error(ref e) => {
                if let Err(err) = self.pipeline().set_state(gst::State::Null) {
                    tracing::warn!("Failed to stop pipeline on error: {err:?}");
                }

                if let Some(session) = imp.session.take() {
                    utils::spawn(async move {
                        if let Err(err) = session.close().await {
                            tracing::warn!("Failed to close screencast session on error: {err:?}");
                        }
                    });
                }

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                let state = Err(Report::from(e.error()))
                    .attach_printable_lazy(|| e.debug().unwrap_or_else(|| "<no debug>".to_string()))
                    .change_context(RecordingError::Other);

                if e.error().matches(gst::ResourceError::OpenWrite) {
                    self.set_state(RecordingState::Finished(Rc::new(
                        state.attach_help(&WRITE_ERROR_HELP),
                    )));
                } else {
                    self.set_state(RecordingState::Finished(Rc::new(state)));
                }

                Continue(false)
            }
            MessageView::Eos(..) => {
                tracing::info!("Eos signal received from record bus");

                if let Err(err) = self.pipeline().set_state(gst::State::Null) {
                    tracing::error!("Failed to stop pipeline on eos: {err:?}");
                }

                if let Some(session) = imp.session.take() {
                    utils::spawn(async move {
                        if let Err(err) = session.close().await {
                            tracing::warn!("Failed to close screencast session on eos: {err:?}");
                        }
                    });
                }

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                // TODO handle paths better here and in settings
                let filesink = self.pipeline().by_name("filesink").unwrap();
                let recording_file_path = filesink.property::<String>("location").into();

                self.set_state(RecordingState::finished_ok(recording_file_path));

                Continue(false)
            }
            MessageView::StateChanged(sc) => {
                if message.src().as_ref()
                    != imp
                        .pipeline
                        .get()
                        .map(|pipeline| pipeline.upcast_ref::<gst::Object>())
                {
                    return Continue(true);
                }

                let new_state = sc.current();

                tracing::info!(
                    "Pipeline state changed from `{:?}` -> `{:?}`",
                    sc.old(),
                    new_state,
                );

                let state = match new_state {
                    gst::State::Paused => RecordingState::Paused,
                    gst::State::Playing => RecordingState::Recording,
                    _ => return Continue(true),
                };
                self.set_state(state);

                Continue(true)
            }
            _ => Continue(true),
        }
    }
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}
