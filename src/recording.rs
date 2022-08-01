use ashpd::{
    desktop::screencast::{CursorMode, PersistMode, SourceType},
    enumflags2::BitFlags,
};
use gst::prelude::*;
use gtk::glib::{self, clone, subclass::prelude::*};
use once_cell::unsync::OnceCell;

use std::{
    cell::{Cell, RefCell},
    fmt,
    path::PathBuf,
    time::Duration,
};

use crate::{
    area_selector::AreaSelector,
    audio_device::{self, Class as AudioDeviceClass},
    cancelled::Cancelled,
    clock_time::ClockTime,
    pipeline_builder::PipelineBuilder,
    screencast_session::ScreencastSession,
    settings::CaptureMode,
    timer::Timer,
    utils, Application,
};

#[derive(Debug, Default, Clone, PartialEq, Eq, glib::Boxed)]
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
    Finished(Result<PathBuf, RecordingError>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordingError {
    Cancelled(Cancelled),
    Gstreamer(glib::Error),
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Recording error: {:?}", self)
    }
}

mod imp {
    use super::*;
    use once_cell::sync::Lazy;

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

    pub async fn start(&self, delay: Duration) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(self.state(), RecordingState::Null),
            "already started recording"
        );

        let imp = self.imp();

        let settings = Application::default().settings();

        // setup screencast session
        let screencast_session = ScreencastSession::new().await?;
        tracing::debug!(
            "Available cursor modes: {:?}",
            screencast_session.available_cursor_modes().await
        );
        tracing::debug!(
            "Available source types: {:?}",
            screencast_session.available_source_types().await
        );
        let (streams, restore_token, fd) = screencast_session
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
            .await?;
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
            match AreaSelector::new().select_area().await {
                Ok((coords, actual_screen)) => {
                    pipeline_builder
                        .coordinates(coords)
                        .actual_screen(actual_screen);
                }
                Err(err) => {
                    if let Some(session) = imp.session.take() {
                        if let Err(err) = session.close().await {
                            tracing::warn!("Failed to close session on timer cancelled: {:?}", err);
                        };
                    }

                    self.set_state(RecordingState::Finished(Err(RecordingError::Cancelled(
                        Cancelled::new("Cancelled timer"),
                    ))));

                    return Err(err.into());
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
            if let Some(session) = imp.session.take() {
                if let Err(err) = session.close().await {
                    tracing::warn!("Failed to close session on timer cancelled: {:?}", err);
                };
            }

            self.set_state(RecordingState::Finished(Err(RecordingError::Cancelled(
                Cancelled::new("Cancelled timer"),
            ))));

            return Ok(());
        }

        // setup audio sources
        if settings.record_mic() {
            pipeline_builder
                .mic_source(audio_device::find_default_name(AudioDeviceClass::Source).await?);
        }
        if settings.record_speaker() {
            pipeline_builder
                .speaker_source(audio_device::find_default_name(AudioDeviceClass::Sink).await?);
        }

        // build pipeline
        let pipeline = pipeline_builder.build()?;
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
        pipeline.set_state(gst::State::Playing)?;
        // TODO Add preparing state
        self.update_duration();

        Ok(())
    }

    pub fn pause(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(self.state(), RecordingState::Recording),
            "recording can only be paused from recording state"
        );

        self.pipeline().set_state(gst::State::Paused)?;

        Ok(())
    }

    pub fn resume(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(self.state(), RecordingState::Paused),
            "recording can only be resumed from paused state",
        );

        self.pipeline().set_state(gst::State::Playing)?;

        Ok(())
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !matches!(self.state(), RecordingState::Null),
            "recording has not started yet"
        );

        anyhow::ensure!(
            !matches!(self.state(), RecordingState::Flushing),
            "already flushing recording"
        );

        anyhow::ensure!(
            !matches!(self.state(), RecordingState::Finished(_)),
            "already finished recording"
        );

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

        self.set_state(RecordingState::Finished(Err(RecordingError::Cancelled(
            Cancelled::default(),
        ))));

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
        if state == self.state() {
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
            MessageView::Error(ref err) => {
                tracing::error!(
                    "Error from record bus: {:?} (debug {:#?})",
                    err.error(),
                    err
                );

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

                self.set_state(RecordingState::Finished(Err(RecordingError::Gstreamer(
                    err.error(),
                ))));

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

                self.set_state(RecordingState::Finished(Ok(recording_file_path)));

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
