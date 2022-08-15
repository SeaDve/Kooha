use anyhow::{ensure, Context, Error, Result};
use gettextrs::gettext;
use gst::prelude::*;
use gtk::{
    gio::{self, prelude::*},
    glib::{self, clone, closure_local, subclass::prelude::*, translate::IntoGlib},
};
use once_cell::{sync::Lazy, unsync::OnceCell};

use std::{
    cell::{Cell, RefCell},
    path::Path,
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use crate::{
    area_selector::AreaSelector,
    audio_device::{self, Class as AudioDeviceClass},
    cancelled::Cancelled,
    help::{ErrorExt, ResultExt},
    pipeline_builder::PipelineBuilder,
    screencast_session::{CursorMode, PersistMode, ScreencastSession, SourceType},
    settings::{CaptureMode, VideoFormat},
    timer::Timer,
    utils, Application,
};

const IT_DOES_NOT_WORK_LINK: &str =
    r#"<a href="https://github.com/SeaDve/Kooha#-it-doesnt-work">It Doesn't Work page</a>"#;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "KoohaRecordingState")]
pub enum State {
    #[default]
    Init,
    Delayed {
        secs_left: u64,
    },
    Recording,
    Paused,
    Flushing,
    Finished,
}

#[derive(Debug, Clone, glib::SharedBoxed)]
#[shared_boxed_type(name = "KoohaRecordingResult")]
struct BoxedResult(Rc<Result<gio::File>>);

mod imp {
    use super::*;
    use glib::subclass::Signal;

    #[derive(Debug, Default)]
    pub struct Recording {
        pub(super) file: OnceCell<gio::File>,

        pub(super) timer: RefCell<Option<Timer>>,
        pub(super) session: RefCell<Option<ScreencastSession>>,
        pub(super) duration_source_id: RefCell<Option<glib::SourceId>>,
        pub(super) pipeline: OnceCell<gst::Pipeline>,

        pub(super) state: Cell<State>,
        pub(super) duration: Cell<gst::ClockTime>,
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
                    glib::ParamSpecBoxed::builder("state", State::static_type())
                        .flags(glib::ParamFlags::READABLE)
                        .build(),
                    glib::ParamSpecUInt64::builder("duration")
                        .maximum(gst::ClockTime::MAX.into_glib())
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

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder(
                    "finished",
                    &[BoxedResult::static_type().into()],
                    <()>::static_type().into(),
                )
                .build()]
            });

            SIGNALS.as_ref()
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
        if !matches!(self.state(), State::Init) {
            tracing::error!("Trying to start recording on a non-init state");
            return;
        }

        if let Err(err) = self.start_inner(delay).await {
            self.close_session();
            self.set_finished(Err(err));
        }
    }

    async fn start_inner(&self, delay: Duration) -> Result<()> {
        let imp = self.imp();

        let settings = Application::default().settings();

        // setup screencast session
        let screencast_session = ScreencastSession::new()
            .await
            .context("Failed to create ScreencastSession")
            .with_help(
                || gettext!("Check out {} for help.", IT_DOES_NOT_WORK_LINK),
                || gettext("Failed to start recording"),
            )?;
        tracing::debug!(
            "ScreenCast portal version: {:?}",
            screencast_session.version().await
        );
        tracing::debug!(
            "Available cursor modes: {:?}",
            screencast_session.available_cursor_modes().await
        );
        tracing::debug!(
            "Available source types: {:?}",
            screencast_session.available_source_types().await
        );
        let (streams, restore_token, fd) = screencast_session
            .begin(
                if settings.show_pointer() {
                    CursorMode::EMBEDDED
                } else {
                    CursorMode::HIDDEN
                },
                if settings.capture_mode() == CaptureMode::Selection {
                    SourceType::MONITOR
                } else {
                    SourceType::MONITOR | SourceType::WINDOW
                },
                settings.capture_mode() == CaptureMode::MonitorWindow,
                Some(&settings.screencast_restore_token()),
                PersistMode::ExplicitlyRevoked,
                Application::default().main_window().as_ref(),
            )
            .await
            .context("Failed to begine ScreencastSession")
            .with_help(
                || gettext!("Check out {} for help.", IT_DOES_NOT_WORK_LINK),
                || gettext("Failed to start recording"),
            )?;
        imp.session.replace(Some(screencast_session));
        settings.set_screencast_restore_token(&restore_token.unwrap_or_default());

        // setup path
        let video_format = settings.video_format();
        let recording_path = new_recording_path(&settings.saving_location(), video_format);
        let mut pipeline_builder = PipelineBuilder::new(
            &recording_path,
            settings.video_framerate(),
            video_format,
            fd,
            streams,
        );
        imp.file.set(gio::File::for_path(&recording_path)).unwrap();

        // select area
        if settings.capture_mode() == CaptureMode::Selection {
            let (selection, screen) = AreaSelector::select_area().await?;

            pipeline_builder
                .coordinates(selection)
                .actual_screen(screen);
        }

        // setup timer
        let timer = Timer::new(
            delay,
            clone!(@weak self as obj => move |secs_left| {
                obj.set_state(State::Delayed {
                    secs_left
                });
            }),
        );
        imp.timer.replace(Some(Timer::clone(&timer)));
        timer.await?;

        // setup audio sources
        if settings.record_mic() {
            pipeline_builder.mic_source(
                audio_device::find_default_name(AudioDeviceClass::Source)
                    .await
                    .with_context(|| gettext("No microphone source found"))?,
            );
        }
        if settings.record_speaker() {
            pipeline_builder.speaker_source(
                audio_device::find_default_name(AudioDeviceClass::Sink)
                    .await
                    .with_context(|| gettext("No desktop speaker source found"))?,
            );
        }

        // build pipeline
        let pipeline = pipeline_builder
            .build()
            .with_help(
                || gettext("A GStreamer plugin may not be installed. If it is installed but still does not work properly, please report to <a href=\"https://github.com/SeaDve/Kooha/issues\">Kooha's issue page</a>."),
                || gettext("Failed to start recording")
            )?;
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
            .map_err(Error::from)
            .context("Failed to initialize pipeline state to playing")
            .with_help(
                || gettext("Make sure that the saving location exists and is accessible."),
                || gettext("Failed to start recording"),
            )?;
        self.update_duration();

        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        ensure!(
            matches!(self.state(), State::Recording),
            "Recording can only be paused from recording state"
        );

        self.pipeline()
            .set_state(gst::State::Paused)
            .context("Failed to set pipeline state to paused")?;

        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        ensure!(
            matches!(self.state(), State::Paused),
            "Recording can only be resumed from paused state"
        );

        self.pipeline()
            .set_state(gst::State::Playing)
            .context("Failed to set pipeline state to playing from paused")?;

        Ok(())
    }

    pub fn stop(&self) {
        let state = self.state();

        if matches!(state, State::Init | State::Flushing | State::Finished) {
            tracing::error!("Trying to stop recording on a `{:?}` state", state);
            return;
        }

        if let Err(err) = self.stop_inner() {
            self.close_session();
            self.set_finished(Err(err));
        }
    }

    fn stop_inner(&self) -> Result<()> {
        tracing::info!("Sending eos event to pipeline");
        self.pipeline().send_event(gst::event::Eos::new());
        self.set_state(State::Flushing);

        Ok(())
    }

    pub fn cancel(&self) {
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

        self.close_session();

        if let Some(source_id) = imp.duration_source_id.take() {
            source_id.remove();
        }

        self.set_finished(Err(Error::from(Cancelled::new("recording"))));

        self.delete_file();
    }

    pub fn state(&self) -> State {
        self.imp().state.get()
    }

    pub fn connect_state_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("state"), move |obj, _| f(obj))
    }

    pub fn duration(&self) -> gst::ClockTime {
        self.imp().duration.get()
    }

    pub fn connect_duration_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("duration"), move |obj, _| f(obj))
    }

    pub fn connect_finished<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Result<gio::File>) + 'static,
    {
        self.connect_closure(
            "finished",
            true,
            closure_local!(|obj: &Self, result: BoxedResult| {
                f(obj, &result.0);
            }),
        )
    }

    fn set_state(&self, state: State) {
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

    fn set_finished(&self, res: Result<gio::File>) {
        self.set_state(State::Finished);

        let result = BoxedResult(Rc::new(res));
        self.emit_by_name::<()>("finished", &[&result]);
    }

    /// Closes session on the background
    fn close_session(&self) {
        if let Some(session) = self.imp().session.take() {
            utils::spawn(async move {
                if let Err(err) = session.close().await {
                    tracing::warn!("Failed to close screencast session: {:?}", err);
                }
            });
        }
    }

    /// Deletes recording file on background
    fn delete_file(&self) {
        if let Some(file) = self.imp().file.get() {
            file.delete_async(glib::PRIORITY_DEFAULT_IDLE, gio::Cancellable::NONE, |res| {
                if let Err(err) = res {
                    tracing::warn!("Failed to delete recording file: {:?}", err);
                }
            });
        }
    }

    fn update_duration(&self) {
        let clock_time = self
            .imp()
            .pipeline
            .get()
            .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
            .unwrap_or(gst::ClockTime::ZERO);

        self.imp().duration.set(clock_time);
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

                self.close_session();

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                // TODO print error quarks for all glib::Error

                let error = Error::from(e.error())
                    .context(e.debug().unwrap_or_else(|| "<no debug>".to_string()))
                    .context(gettext("An error occurred while recording"));

                if e.error().matches(gst::ResourceError::OpenWrite) {
                    let error = error.help(
                        gettext("Make sure that the saving location exists and is accessible."),
                        if let Some(ref path) = imp.file.get().and_then(|f| f.path()) {
                            gettext!("Failed to open “{}” for writing", path.display())
                        } else {
                            gettext("Failed to open file for writing")
                        },
                    );
                    self.set_finished(Err(error));
                } else {
                    self.set_finished(Err(error));
                }

                self.delete_file();

                Continue(false)
            }
            MessageView::Eos(..) => {
                tracing::info!("Eos signal received from record bus");

                if let Err(err) = self.pipeline().set_state(gst::State::Null) {
                    tracing::error!("Failed to stop pipeline on eos: {err:?}");
                }

                self.close_session();

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                let pipeline_file_path = self
                    .pipeline()
                    .by_name("filesink")
                    .map(|fs| fs.property::<String>("location"));
                let file = imp.file.get().unwrap();
                debug_assert_eq!(
                    pipeline_file_path.map(|path| PathBuf::from(&path)),
                    Some(file.path().unwrap())
                );

                self.set_finished(Ok(file.clone()));

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
                    gst::State::Paused => State::Paused,
                    gst::State::Playing => State::Recording,
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

fn new_recording_path(saving_location: &Path, video_format: VideoFormat) -> PathBuf {
    let file_name = glib::DateTime::now_local()
        .expect("You are somehow on year 9999")
        .format("Kooha-%F-%H-%M-%S")
        .expect("Invalid format string");

    let mut path = saving_location.join(file_name);
    path.set_extension(match video_format {
        VideoFormat::Webm => "webm",
        VideoFormat::Mkv => "mkv",
        VideoFormat::Mp4 => "mp4",
        VideoFormat::Gif => "gif",
    });

    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration() {
        let recording = Recording::new();
        assert_eq!(recording.duration(), gst::ClockTime::ZERO);
        assert_eq!(
            recording.property::<gst::ClockTime>("duration"),
            gst::ClockTime::ZERO
        );
        assert_eq!(
            recording.property::<u64>("duration"),
            gst::ClockTime::ZERO.into_glib()
        );

        recording
            .imp()
            .duration
            .set(gst::ClockTime::from_seconds(3));
        assert_eq!(recording.duration(), gst::ClockTime::from_seconds(3));
        assert_eq!(
            recording.property::<gst::ClockTime>("duration"),
            gst::ClockTime::from_seconds(3)
        );
        assert_eq!(
            recording.property::<u64>("duration"),
            gst::ClockTime::from_seconds(3).into_glib()
        );
    }
}
