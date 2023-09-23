use anyhow::{anyhow, ensure, Context, Error, Result};
use gettextrs::gettext;
use gst::prelude::*;
use gtk::{
    gio::{self, prelude::*},
    glib::{self, clone, closure_local, subclass::prelude::*},
};
use once_cell::{sync::Lazy, unsync::OnceCell};

use std::{
    cell::{Cell, RefCell},
    error, fmt,
    os::unix::prelude::RawFd,
    rc::Rc,
    time::Duration,
};

use crate::{
    area_selector::AreaSelector,
    audio_device::{self, Class as AudioDeviceClass},
    cancelled::Cancelled,
    help::{ErrorExt, ResultExt},
    i18n::gettext_f,
    pipeline::PipelineBuilder,
    screencast_session::{CursorMode, PersistMode, ScreencastSession, SourceType, Stream},
    settings::{CaptureMode, Settings},
    timer::Timer,
    utils,
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
    use gst::bus::BusWatchGuard;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::Recording)]
    pub struct Recording {
        #[property(get)]
        pub(super) state: Cell<State>,
        #[property(get)]
        pub(super) duration: Cell<gst::ClockTime>,

        pub(super) file: OnceCell<gio::File>,

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
        glib::Object::builder().build()
    }

    pub async fn start(&self, parent: Option<&impl IsA<gtk::Window>>, settings: &Settings) {
        if !matches!(self.state(), State::Init) {
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
            if utils::is_experimental_mode() {
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
            profile,
            fd,
            streams.clone(),
        );

        // select area
        if settings.capture_mode() == CaptureMode::Selection {
            let data =
                AreaSelector::present(Some(&utils::app_instance().window()), fd, &streams).await?;
            pipeline_builder.select_area_data(data);
        }

        // setup timer
        let timer = Timer::new(
            settings.record_delay(),
            clone!(@weak self as obj => move |secs_left| {
                obj.set_state(State::Delayed {
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
        imp.pipeline.set(pipeline.clone()).unwrap();
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
                glib::ControlFlow::Continue
            }),
        )));
        pipeline
            .set_state(gst::State::Playing)
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

        self.set_state(State::Flushing);

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

        self.delete_file();
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

    fn file(&self) -> Result<&gio::File> {
        let imp = self.imp();
        self.imp().file.get_or_try_init(|| {
            let location = imp
                .pipeline
                .get()
                .ok_or_else(|| anyhow!("Pipeline not set"))?
                .by_name("filesink")
                .ok_or_else(|| anyhow!("Element filesink not found on pipeline"))?
                .property::<String>("location");
            Ok(gio::File::for_path(location))
        })
    }

    fn set_state(&self, state: State) {
        if state == self.state() {
            return;
        }

        self.imp().state.replace(state);
        self.notify_state();
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
        if let Ok(file) = self.file() {
            file.delete_async(
                glib::Priority::DEFAULT_IDLE,
                gio::Cancellable::NONE,
                |res| {
                    if let Err(err) = res {
                        tracing::warn!("Failed to delete recording file: {:?}", err);
                    }
                },
            );
        } else {
            tracing::error!("Failed to delete recording file: Failed to get file");
        }
    }

    fn update_duration(&self) {
        let clock_time = self
            .imp()
            .pipeline
            .get()
            .and_then(|pipeline| pipeline.query_position::<gst::ClockTime>())
            .unwrap_or(gst::ClockTime::ZERO);

        if clock_time == self.duration() {
            return;
        }

        self.imp().duration.set(clock_time);
        self.notify_duration();
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        use gst::MessageView;

        let imp = self.imp();

        match message.view() {
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
                        if let Some(ref path) = self
                            .file()
                            .ok()
                            .and_then(|f| f.path())
                            .and_then(|path| path.parent().map(|p| p.to_owned()))
                        {
                            gettext_f(
                                // Translators: Do NOT translate the contents between '{' and '}', this is a variable name.
                                "Failed to open “{path}” for writing",
                                &[("path", &path.display().to_string())],
                            )
                        } else {
                            gettext("Failed to open file for writing")
                        },
                    )
                } else {
                    error
                };

                self.set_finished(Err(error));
                self.delete_file();

                glib::ControlFlow::Break
            }
            MessageView::Eos(..) => {
                tracing::debug!("Eos signal received from record bus");

                if self.state() != State::Flushing {
                    tracing::error!("Received an Eos signal on a {:?} state", self.state());
                }

                if let Err(err) = self.pipeline().set_state(gst::State::Null) {
                    tracing::error!("Failed to stop pipeline on eos: {:?}", err);
                }

                self.close_session();

                if let Some(source_id) = imp.duration_source_id.take() {
                    source_id.remove();
                }

                self.set_finished(Ok(self.file().unwrap().clone()));

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
                    gst::State::Paused => State::Paused,
                    gst::State::Playing => State::Recording,
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
    let screencast_session = ScreencastSession::new()
        .await
        .context("Failed to create ScreencastSession")?;

    tracing::debug!(
        "ScreenCast portal version: {:?}",
        screencast_session.version()
    );
    tracing::debug!(
        "Available cursor modes: {:?}",
        screencast_session.available_cursor_modes()
    );
    tracing::debug!(
        "Available source types: {:?}",
        screencast_session.available_source_types()
    );

    // TODO handle Closed signal from service side
    let (streams, restore_token, fd) = screencast_session
        .begin(
            cursor_mode,
            source_type,
            is_multiple_sources,
            restore_token,
            persist_mode,
            parent_window,
        )
        .await
        .context("Failed to begin ScreencastSession")?;

    Ok((screencast_session, streams, restore_token, fd))
}
