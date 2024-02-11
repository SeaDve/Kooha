use std::{
    cell::{Cell, OnceCell, RefCell},
    error, fmt,
    os::unix::prelude::RawFd,
    rc::Rc,
    time::{Duration, Instant},
};

use anyhow::{ensure, Context, Error, Result};
use gettextrs::gettext;
use gst::prelude::*;
use gtk::{
    gio::{self, prelude::*},
    glib::{self, clone, closure_local, subclass::prelude::*},
};

use crate::{
    application::Application,
    area_selector::AreaSelector,
    audio_device::{self, AudioDeviceClass},
    cancelled::Cancelled,
    help::{ErrorExt, ResultExt},
    i18n::gettext_f,
    pipeline::{PipelineBuilder, FILESINK_ELEMENT_NAME},
    screencast_session::{
        CursorMode, PersistMode, ScreencastSession, SourceType, Stream, WindowIdentifier,
    },
    settings::{CaptureMode, Settings},
    timer::Timer,
    IS_EXPERIMENTAL_MODE,
};

const DEFAULT_DURATION_UPDATE_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub struct NoProfileError;

impl fmt::Display for NoProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&gettext("No active profile"))
    }
}

impl error::Error for NoProfileError {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "KoohaRecordingState")]
pub enum RecordingState {
    #[default]
    Init,
    Delayed {
        secs_left: u64,
    },
    Recording,
    Paused,
    Flushing {
        /// Progress in percentage with range 0..=100
        progress: u8,
    },
    Finished,
}

#[derive(Debug, Clone, glib::SharedBoxed)]
#[shared_boxed_type(name = "KoohaRecordingResult")]
struct BoxedResult(Rc<Result<gio::File>>);

mod imp {
    use glib::subclass::Signal;
    use gst::bus::BusWatchGuard;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::Recording)]
    pub struct Recording {
        #[property(get)]
        pub(super) state: Cell<RecordingState>,
        #[property(get)]
        pub(super) duration: Cell<gst::ClockTime>,

        pub(super) file: OnceCell<gio::File>,

        pub(super) starting_time: Cell<Option<Instant>>,
        pub(super) expected_duration: Cell<Option<Duration>>,

        pub(super) timer: RefCell<Option<Timer>>,
        pub(super) session: RefCell<Option<ScreencastSession>>,
        pub(super) duration_source_id: RefCell<Option<glib::SourceId>>,
        pub(super) pipeline: OnceCell<gst::Pipeline>,
        pub(super) bus_watch_guard: RefCell<Option<BusWatchGuard>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Recording {
        const NAME: &'static str = "KoohaRecording";
        type Type = super::Recording;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Recording {
        fn dispose(&self) {
            if let Some(timer) = self.timer.take() {
                timer.cancel();
            }

            if let Some(pipeline) = self.pipeline.get() {
                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::warn!("Failed to stop pipeline on dispose: {:?}", err);
                }
            }

            self.obj().close_session();

            if let Some(source_id) = self.duration_source_id.take() {
                source_id.remove();
            }
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("finished")
                    .param_types([BoxedResult::static_type()])
                    .build()]
            });

            SIGNALS.as_ref()
        }
    }
}

glib::wrapper! {
     pub struct Recording(ObjectSubclass<imp::Recording>);
}

impl Recording {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub async fn start(&self, parent: Option<&impl IsA<gtk::Window>>, settings: &Settings) {
        if !matches!(self.state(), RecordingState::Init) {
            tracing::error!("Trying to start recording on a non-init state");
            return;
        }

        if let Err(err) = self.start_inner(parent, settings).await {
            self.close_session();
            self.set_finished(Err(err));
        }
    }

    async fn start_inner(
        &self,
        parent: Option<&impl IsA<gtk::Window>>,
        settings: &Settings,
    ) -> Result<()> {
        let imp = self.imp();
        let profile = settings.profile().context(NoProfileError)?;
        let profile_supports_audio = profile.supports_audio();

        // setup screencast session
        let restore_token = settings.screencast_restore_token();
        settings.set_screencast_restore_token("");
        let (screencast_session, streams, restore_token, fd) = new_screencast_session(
            if settings.show_pointer() {
                CursorMode::EMBEDDED
            } else {
                CursorMode::HIDDEN
            },
            if *IS_EXPERIMENTAL_MODE {
                SourceType::MONITOR | SourceType::WINDOW
            } else {
                SourceType::MONITOR
            },
            true,
            Some(&restore_token),
            PersistMode::ExplicitlyRevoked,
            parent,
        )
        .await
        .with_help(
            || {
                gettext_f(
                    // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
                    "Check out {link} for help.",
                    &[("link", r#"<a href="https://github.com/SeaDve/Kooha#-it-doesnt-work">It Doesn't Work page</a>"#)],
                )
            },
            || gettext("Failed to start recording"),
        )?;
        imp.session.replace(Some(screencast_session));
        settings.set_screencast_restore_token(&restore_token.unwrap_or_default());

        let mut pipeline_builder = PipelineBuilder::new(
            &settings.saving_location(),
            settings.video_framerate(),
            profile.clone(),
            fd,
            streams.clone(),
        );

        // select area
        if settings.capture_mode() == CaptureMode::Selection {
            let data =
                AreaSelector::present(Some(&Application::get().window()), fd, &streams).await?;
            pipeline_builder.select_area_data(data);
        }

        // setup timer
        let timer = Timer::new(
            settings.record_delay(),
            clone!(@weak self as obj => move |secs_left| {
                obj.set_state(RecordingState::Delayed {
                    secs_left
                });
            }),
        );
        imp.timer.replace(Some(Timer::clone(&timer)));
        timer.await?;

        // setup audio sources
        if profile_supports_audio {
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
        }

        // build pipeline
        let pipeline = pipeline_builder.build().with_help(
            || gettext("A GStreamer plugin may not be installed."),
            || gettext("Failed to start recording"),
        )?;
        pipeline.debug_to_dot_file_with_ts(gst::DebugGraphDetails::VERBOSE, "kooha-pipeline");
        imp.pipeline.set(pipeline.clone()).unwrap();

        let location = pipeline
            .by_name(FILESINK_ELEMENT_NAME)
            .context("Element filesink not found on pipeline")?
            .property::<String>("location");
        imp.file.set(gio::File::for_path(location)).unwrap();
        let bus_watch_guard = pipeline
            .bus()
            .unwrap()
            .add_watch_local(
                clone!(@weak self as obj => @default-return glib::ControlFlow::Break, move |_, message|  {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();
        imp.bus_watch_guard.replace(Some(bus_watch_guard));
        imp.duration_source_id.replace(Some(glib::timeout_add_local(
            DEFAULT_DURATION_UPDATE_INTERVAL,
            clone!(@weak self as obj => @default-return glib::ControlFlow::Break, move || {
                obj.update_duration();
                obj.update_flushing_progress();
                glib::ControlFlow::Continue
            }),
        )));

        let state_change = pipeline
            .set_state(gst::State::Playing)
            .context("Failed to initialize pipeline state to playing")
            .with_help(
                || gettext("Make sure that the saving location exists and is accessible."),
                || gettext("Failed to start recording"),
            )?;
        self.update_duration();

        tracing::debug!(?state_change, "Pipeline state change");
        if state_change != gst::StateChangeSuccess::Async {
            imp.starting_time.set(Some(Instant::now()));
        }

        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        ensure!(
            matches!(self.state(), RecordingState::Recording),
            "Recording can only be paused from recording state"
        );

        self.pipeline()
            .set_state(gst::State::Paused)
            .context("Failed to set pipeline state to paused")?;

        Ok(())
    }

    pub fn resume(&self) -> Result<()> {
        ensure!(
            matches!(self.state(), RecordingState::Paused),
            "Recording can only be resumed from paused state"
        );

        self.pipeline()
            .set_state(gst::State::Playing)
            .context("Failed to set pipeline state to playing from paused")?;

        Ok(())
    }

    pub fn stop(&self) {
        let imp = self.imp();

        let state = self.state();
        if matches!(
            state,
            RecordingState::Init | RecordingState::Flushing { .. } | RecordingState::Finished
        ) {
            tracing::error!("Trying to stop recording on a `{:?}` state", state);
            return;
        }

        self.set_state(RecordingState::Flushing { progress: 0 });
        imp.expected_duration.set(
            imp.starting_time
                .get()
                .map(|starting_time| starting_time.elapsed()),
        );
        self.update_flushing_progress();

        tracing::debug!("Sending eos event to pipeline");
        // FIXME Maybe it is needed to verify if we received the same
        // eos event by checking its seqnum in the bus?
        self.pipeline().send_event(gst::event::Eos::new());
    }

    pub fn cancel(&self) {
        let imp = self.imp();

        tracing::debug!("Cancelling recording");

        if let Some(timer) = imp.timer.take() {
            timer.cancel();
        }

        if let Some(pipeline) = imp.pipeline.get() {
            if let Err(err) = pipeline.set_state(gst::State::Null) {
                tracing::warn!("Failed to stop pipeline on cancel: {:?}", err);
            }
        }

        let _ = imp.bus_watch_guard.take();

        self.close_session();

        if let Some(source_id) = imp.duration_source_id.take() {
            source_id.remove();
        }

        // HACK we need to return before calling this to avoid a `BorrowMutError` when
        // `Window` tried to take the `recording` on finished callback while `recording`
        // is borrowed to call `cancel`.
        glib::idle_add_local_once(clone!(@weak self as obj => move || {
            obj.set_finished(Err(Error::from(Cancelled::new("recording"))));
        }));
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

    fn set_state(&self, state: RecordingState) {
        tracing::debug!("Recording state changed to {:?}", state);

        if state == self.state() {
            return;
        }

        self.imp().state.replace(state);
        self.notify_state();
    }

    fn file(&self) -> &gio::File {
        self.imp()
            .file
            .get()
            .expect("file not set, make sure to start recording first")
    }

    fn pipeline(&self) -> &gst::Pipeline {
        self.imp()
            .pipeline
            .get()
            .expect("pipeline not set, make sure to start recording first")
    }

    fn set_finished(&self, res: Result<gio::File>) {
        self.set_state(RecordingState::Finished);

        let result = BoxedResult(Rc::new(res));
        self.emit_by_name::<()>("finished", &[&result]);
    }

    /// Closes session on the background
    fn close_session(&self) {
        if let Some(session) = self.imp().session.take() {
            glib::spawn_future_local(async move {
                if let Err(err) = session.close().await {
                    tracing::warn!("Failed to close screencast session: {:?}", err);
                }
            });
        }
    }

    fn update_duration(&self) {
        let imp = self.imp();

        let clock_time = imp
            .pipeline
            .get()
            .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
            .unwrap_or(gst::ClockTime::ZERO);

        if clock_time == self.duration() {
            return;
        }

        imp.duration.set(clock_time);
        self.notify_duration();
    }

    fn update_flushing_progress(&self) {
        let imp = self.imp();

        let RecordingState::Flushing {
            progress: prev_progress,
        } = self.state()
        else {
            return;
        };

        let progress = imp.expected_duration.get().map_or(0, |expected_duration| {
            let progress_percent =
                self.duration().nseconds() as f64 / expected_duration.as_nanos() as f64 * 100.0;
            (progress_percent.round() as u8).clamp(0, 100)
        });

        if prev_progress == progress {
            return;
        }

        self.set_state(RecordingState::Flushing { progress });
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        use gst::MessageView;

        let imp = self.imp();

        match message.view() {
            MessageView::AsyncDone(ad) => {
                tracing::debug!(
                    state = ?self.state(),
                    running_time = ?ad.running_time(),
                    "Received async done message on bus"
                );

                if imp.starting_time.get().is_none() {
                    imp.starting_time.set(Some(Instant::now()));
                }

                glib::ControlFlow::Continue
            }
            MessageView::Error(e) => {
                tracing::debug!(state = ?self.state(), "Received error at bus");

                if let Err(err) = self.pipeline().set_state(gst::State::Null) {
                    tracing::warn!("Failed to stop pipeline on error: {:?}", err);
                }

                self.close_session();

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                // TODO print error quarks for all glib::Error

                let error = Error::from(e.error())
                    .context(e.debug().unwrap_or_else(|| "<no debug>".into()))
                    .context(gettext("An error occurred while recording"));

                let error = if e.error().matches(gst::ResourceError::OpenWrite) {
                    error.help(
                        gettext("Make sure that the saving location exists and is accessible."),
                        gettext_f(
                            // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
                            "Failed to open “{path}” for writing",
                            &[("path", &self.file().uri())],
                        ),
                    )
                } else {
                    error
                };

                self.set_finished(Err(error));

                glib::ControlFlow::Break
            }
            MessageView::Eos(..) => {
                tracing::debug!("Eos signal received from record bus");

                debug_assert!(
                    matches!(self.state(), RecordingState::Flushing { .. }),
                    "received eos signal on {:?}",
                    self.state()
                );

                self.set_state(RecordingState::Flushing { progress: 100 });

                if let Err(err) = self.pipeline().set_state(gst::State::Null) {
                    tracing::error!("Failed to stop pipeline on eos: {:?}", err);
                }

                self.close_session();

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                self.set_finished(Ok(self.file().clone()));

                glib::ControlFlow::Break
            }
            MessageView::StateChanged(sc) => {
                let new_state = sc.current();

                if message.src()
                    != imp
                        .pipeline
                        .get()
                        .map(|pipeline| pipeline.upcast_ref::<gst::Object>())
                {
                    tracing::trace!(
                        "`{}` changed state from `{:?}` -> `{:?}`",
                        message
                            .src()
                            .map_or_else(|| "<unknown source>".into(), |e| e.name()),
                        sc.old(),
                        new_state,
                    );
                    return glib::ControlFlow::Continue;
                }

                tracing::debug!(
                    "Pipeline changed state from `{:?}` -> `{:?}`",
                    sc.old(),
                    new_state,
                );

                let state = match new_state {
                    gst::State::Paused => RecordingState::Paused,
                    gst::State::Playing => RecordingState::Recording,
                    _ => return glib::ControlFlow::Continue,
                };
                self.set_state(state);

                glib::ControlFlow::Continue
            }
            MessageView::Warning(w) => {
                tracing::warn!("Received warning message on bus: {:?}", w);
                glib::ControlFlow::Continue
            }
            MessageView::Info(i) => {
                tracing::debug!("Received info message on bus: {:?}", i);
                glib::ControlFlow::Continue
            }
            other => {
                tracing::trace!("Received other message on bus: {:?}", other);
                glib::ControlFlow::Continue
            }
        }
    }
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}

async fn new_screencast_session(
    cursor_mode: CursorMode,
    source_type: SourceType,
    is_multiple_sources: bool,
    restore_token: Option<&str>,
    persist_mode: PersistMode,
    parent_window: Option<&impl IsA<gtk::Window>>,
) -> Result<(ScreencastSession, Vec<Stream>, Option<String>, RawFd)> {
    let session = ScreencastSession::new()
        .await
        .context("Failed to create session")?;

    tracing::debug!(
        version = ?session.version(),
        available_cursor_modes = ?session.available_cursor_modes(),
        available_source_types = ?session.available_source_types(),
        "Created screencast proxy"
    );

    tracing::debug!(
        ?cursor_mode,
        ?source_type,
        is_multiple_sources,
        restore_token,
        ?persist_mode,
        "Selecting sources"
    );

    session
        .select_sources(
            source_type,
            is_multiple_sources,
            cursor_mode,
            restore_token,
            persist_mode,
        )
        .await
        .context("Failed to select sources")?;

    let window_identifier = if let Some(window) = parent_window {
        WindowIdentifier::new(window.as_ref()).await
    } else {
        WindowIdentifier::None
    };

    tracing::debug!(%window_identifier, "Starting session");

    let (streams, restore_token) = session
        .start(window_identifier)
        .await
        .context("Failed to start session")?;

    let fd = session
        .open_pipe_wire_remote()
        .await
        .context("Failed to open pipewire remote")?;

    Ok((session, streams, restore_token, fd))
}
