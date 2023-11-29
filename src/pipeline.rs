use std::{os::fd::RawFd, path::Path, time::Duration};

use anyhow::{ensure, Context, Result};
use gst::prelude::*;
use gst_pbutils::prelude::*;
use gtk::{
    gdk,
    glib::{self, clone, closure_local},
    graphene::Rect,
    subclass::prelude::*,
};

use crate::{
    audio_device::{self, Class as AudioDeviceClass},
    screencast_session::Stream,
    utils,
};

const DURATION_UPDATE_INTERVAL: Duration = Duration::from_millis(200);
const PREVIEW_FRAME_RATE: i32 = 60;

const COMPOSITOR_NAME: &str = "compositor";
const VIDEO_TEE_NAME: &str = "video-tee";
const PAINTABLE_SINK_NAME: &str = "paintablesink";

const DESKTOP_AUDIO_LEVEL_NAME: &str = "desktop-audio-level";
const DESKTOP_AUDIO_TEE: &str = "desktop-audio-tee";

const MICROPHONE_LEVEL_NAME: &str = "microphone-level";
const MICROPHONE_TEE: &str = "microphone-tee";

pub struct CropData {
    /// Full rect where the selection is made.
    pub full_rect: Rect,
    /// Selection made from the full rect.
    pub selection_rect: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "KoohaStreamSize", nullable)]
pub struct StreamSize {
    width: i32,
    height: i32,
}

impl StreamSize {
    pub fn new(width: i32, height: i32) -> Self {
        Self { width, height }
    }

    pub fn width(self) -> i32 {
        self.width
    }

    pub fn height(self) -> i32 {
        self.height
    }
}

#[derive(Debug, Clone, Copy, glib::Boxed)]
#[boxed_type(name = "KoohaPeaks")]
pub struct Peaks {
    left: f64,
    right: f64,
}

impl Peaks {
    pub fn new(left: f64, right: f64) -> Self {
        Self { left, right }
    }

    pub fn left(&self) -> f64 {
        self.left
    }

    pub fn right(&self) -> f64 {
        self.right
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "KoohaRecordingState")]
pub enum RecordingState {
    #[default]
    Idle,
    Started {
        duration: gst::ClockTime,
    },
}

impl RecordingState {
    pub fn started(duration: gst::ClockTime) -> Self {
        Self::Started { duration }
    }

    pub fn is_started(self) -> bool {
        matches!(self, Self::Started { .. })
    }

    pub fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }
}

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::{once_cell::sync::Lazy, subclass::Signal};
    use gst::bus::BusWatchGuard;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::Pipeline)]
    pub struct Pipeline {
        #[property(get)]
        pub(super) stream_size: Cell<Option<StreamSize>>,
        #[property(get)]
        pub(super) recording_state: Cell<RecordingState>,

        pub(super) inner: gst::Pipeline,
        pub(super) bus_watch_guard: RefCell<Option<BusWatchGuard>>,

        pub(super) video_elements: RefCell<Vec<gst::Element>>,
        pub(super) desktop_audio_elements: RefCell<Vec<gst::Element>>,
        pub(super) microphone_elements: RefCell<Vec<gst::Element>>,
        pub(super) recording_elements: RefCell<Vec<gst::Element>>,

        pub(super) duration_source_id: RefCell<Option<glib::SourceId>>,
        pub(super) caps_notify_source_id: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Pipeline {
        const NAME: &'static str = "KoohaPipeline";
        type Type = super::Pipeline;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Pipeline {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if let Err(err) = obj.setup_elements() {
                tracing::error!("Failed to setup pipeline: {:?}", err);
            }
        }

        fn dispose(&self) {
            if let Err(err) = self.inner.set_state(gst::State::Null) {
                tracing::error!("Failed to set state to Null {:?}", err);
            }

            if let Some(source_id) = self.duration_source_id.take() {
                source_id.remove();
            }

            if let Some(source_id) = self.caps_notify_source_id.take() {
                source_id.remove();
            }

            let _ = self.bus_watch_guard.take();
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("desktop-audio-peak")
                        .param_types([Peaks::static_type()])
                        .build(),
                    Signal::builder("microphone-peak")
                        .param_types([Peaks::static_type()])
                        .build(),
                ]
            });

            SIGNALS.as_ref()
        }
    }
}

glib::wrapper! {
    pub struct Pipeline(ObjectSubclass<imp::Pipeline>);
}

impl Pipeline {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn connect_desktop_audio_peak<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Peaks) + 'static,
    {
        self.connect_closure(
            "desktop-audio-peak",
            false,
            closure_local!(|obj: &Self, peaks: &Peaks| {
                f(obj, peaks);
            }),
        )
    }

    pub fn connect_microphone_peak<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &Peaks) + 'static,
    {
        self.connect_closure(
            "microphone-peak",
            false,
            closure_local!(|obj: &Self, peaks: &Peaks| {
                f(obj, peaks);
            }),
        )
    }

    pub fn paintable(&self) -> gdk::Paintable {
        self.imp()
            .inner
            .by_name(PAINTABLE_SINK_NAME)
            .unwrap()
            .property("paintable")
    }

    pub fn start_recording(&self, dir: &Path, crop_data: Option<CropData>) -> Result<()> {
        let imp = self.imp();

        ensure!(imp.recording_state.get().is_idle(), "Already recording");

        assert!(imp.recording_elements.borrow().is_empty());

        let video_profile =
            gst_pbutils::EncodingVideoProfile::builder(&gst::Caps::builder("video/x-vp8").build())
                .preset("Profile Realtime")
                .variable_framerate(true)
                .build();
        let audio_profile = gst_pbutils::EncodingAudioProfile::builder(
            &gst::Caps::builder("audio/x-vorbis").build(),
        )
        .build();
        let profile = gst_pbutils::EncodingContainerProfile::builder(
            &gst::Caps::builder("video/webm").build(),
        )
        .name("WebM audio/video")
        .description("Standard WebM/VP8/Vorbis")
        .add_profile(video_profile)
        .add_profile(audio_profile)
        .build();

        let recording_path = {
            let file_name = glib::DateTime::now_local()
                .context("Failed to get current time")?
                .format("Kooha-%F-%H-%M-%S")
                .unwrap();
            let mut path = dir.join(file_name);
            path.set_extension("webm");
            path
        };

        let encodebin = gst::ElementFactory::make("encodebin")
            .property("profile", profile)
            .build()?;
        let filesink = gst::ElementFactory::make("filesink")
            .property("async", false) // FIXME ?
            .property(
                "location",
                recording_path
                    .to_str()
                    .context("Cannot convert path to str")?,
            )
            .build()?;

        let elements = vec![encodebin.clone(), filesink.clone()];
        imp.inner.add_many(&elements)?;
        encodebin.link(&filesink)?;

        let video_tee = imp.inner.by_name(VIDEO_TEE_NAME).unwrap();
        let video_tee_src_pad = video_tee
            .request_pad_simple("src_%u")
            .context("Failed to request src_%u pad from video tee")?;
        let encodebin_sink_pad = encodebin
            .request_pad_simple("video_%u")
            .context("Failed to request video_%u pad from encodebin")?;

        if let Some(crop_data) = crop_data {
            let stream_size = imp.stream_size.get().context("Stream size was not set")?;
            let videoscale = gst::ElementFactory::make("videoscale").build()?;
            let videocrop = videocrop_compute(&crop_data, stream_size)?;

            // x264enc requires even resolution.
            let videoscale_filter = gst::Caps::builder("video/x-raw")
                .field("width", round_to_even(stream_size.width()))
                .field("height", round_to_even(stream_size.height()))
                .build();

            let elements = vec![videoscale.clone(), videocrop.clone()];
            imp.inner.add_many(&elements)?;

            video_tee_src_pad.link(&videoscale.static_pad("sink").unwrap())?;

            videoscale.link_filtered(&videocrop, &videoscale_filter)?;

            videocrop
                .static_pad("src")
                .unwrap()
                .link(&encodebin_sink_pad)?;

            for element in &elements {
                element.sync_state_with_parent()?;
            }
        } else {
            video_tee_src_pad.link(&encodebin_sink_pad)?;
        }

        if let Some(desktop_audio_tee) = imp.inner.by_name(DESKTOP_AUDIO_TEE) {
            let desktop_audio_tee_src_pad = desktop_audio_tee
                .request_pad_simple("src_%u")
                .context("Failed to request src_%u pad from desktop audio tee")?;
            let encodebin_sink_pad = encodebin
                .request_pad_simple("audio_%u")
                .context("Failed to request audio_%u pad from encodebin")?;
            desktop_audio_tee_src_pad.link(&encodebin_sink_pad)?;
        }

        if let Some(microphone_tee) = imp.inner.by_name(MICROPHONE_TEE) {
            let microphone_tee_src_pad = microphone_tee
                .request_pad_simple("src_%u")
                .context("Failed to request src_%u pad from microphone tee")?;
            let encodebin_sink_pad = encodebin
                .request_pad_simple("audio_%u")
                .context("Failed to request audio_%u pad from encodebin")?;
            microphone_tee_src_pad.link(&encodebin_sink_pad)?;
        }

        for element in &elements {
            element.sync_state_with_parent()?;
        }

        imp.recording_elements.replace(elements);

        imp.duration_source_id.replace(Some(glib::timeout_add_local(
            DURATION_UPDATE_INTERVAL,
            clone!(@weak self as obj => @default-panic, move || {
                let position = obj
                    .imp()
                    .inner
                    .query_position::<gst::ClockTime>()
                    .unwrap_or(gst::ClockTime::ZERO);
                obj.set_recording_state(RecordingState::started(position));
                glib::ControlFlow::Continue
            }),
        )));

        self.set_recording_state(RecordingState::started(gst::ClockTime::ZERO));

        tracing::debug!("Started recording");

        Ok(())
    }

    pub fn stop_recording(&self) -> Result<()> {
        let imp = self.imp();

        let recording_elements = imp.recording_elements.take();

        ensure!(imp.recording_state.get().is_started(), "Not recording");

        assert!(!recording_elements.is_empty());

        for element in recording_elements {
            element.set_state(gst::State::Null)?;
            imp.inner.remove(&element)?;
        }

        imp.duration_source_id.take().unwrap().remove();

        self.set_recording_state(RecordingState::Idle);

        tracing::debug!("Stopped recording");

        Ok(())
    }

    pub fn set_streams(&self, streams: &[Stream], fd: RawFd) -> Result<()> {
        let imp = self.imp();

        for element in imp.video_elements.take() {
            element.set_state(gst::State::Null)?;
            imp.inner.remove(&element)?;
        }

        let compositor = imp.inner.by_name(COMPOSITOR_NAME).unwrap();

        for pad in compositor.sink_pads() {
            compositor.release_request_pad(&pad);
        }

        let videorate_caps = gst::Caps::builder("video/x-raw")
            .field("framerate", gst::Fraction::new(PREVIEW_FRAME_RATE, 1))
            .build();

        let mut last_pos = 0;
        for stream in streams {
            let pipewiresrc = gst::ElementFactory::make("pipewiresrc")
                .property("fd", fd)
                .property("path", stream.node_id().to_string())
                .property("do-timestamp", true)
                .property("keepalive-time", 1000)
                .property("resend-last", true)
                .build()?;
            let videorate = gst::ElementFactory::make("videorate").build()?;
            let videorate_capsfilter = gst::ElementFactory::make("capsfilter")
                .property("caps", &videorate_caps)
                .build()?;

            let elements = [pipewiresrc, videorate, videorate_capsfilter.clone()];
            imp.inner.add_many(&elements)?;
            gst::Element::link_many(&elements)?;
            imp.video_elements.borrow_mut().extend(elements);

            let compositor_sink_pad = compositor
                .request_pad_simple("sink_%u")
                .context("Failed to request sink_%u pad from compositor")?;
            compositor_sink_pad.set_property("xpos", last_pos);
            videorate_capsfilter
                .static_pad("src")
                .unwrap()
                .link(&compositor_sink_pad)?;

            let (stream_width, _) = stream.size().context("stream is missing size")?;
            last_pos += stream_width;
        }

        for element in imp.video_elements.borrow().iter() {
            element.sync_state_with_parent()?;
        }

        tracing::debug!("Loaded {} streams", streams.len());

        match imp.inner.set_state(gst::State::Playing)? {
            gst::StateChangeSuccess::Success | gst::StateChangeSuccess::NoPreroll => {
                self.update_stream_size();
            }
            gst::StateChangeSuccess::Async => {}
        }

        Ok(())
    }

    pub async fn load_desktop_audio(&self) -> Result<()> {
        let imp = self.imp();

        if !imp.desktop_audio_elements.borrow().is_empty() {
            return Ok(());
        }

        let device_name = audio_device::find_default_name(AudioDeviceClass::Sink)
            .await
            .context("No desktop audio source found")?;

        let pulsesrc = gst::ElementFactory::make("pulsesrc")
            .property("device", &device_name)
            .build()?;
        let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
        let level = gst::ElementFactory::make("level")
            .name(DESKTOP_AUDIO_LEVEL_NAME)
            .property("interval", gst::ClockTime::from_mseconds(80))
            .property("peak-ttl", gst::ClockTime::from_mseconds(80))
            .build()?;
        let tee = gst::ElementFactory::make("tee")
            .name(DESKTOP_AUDIO_TEE)
            .build()?;
        let fakesink = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()?;

        let elements = vec![pulsesrc, audioconvert, level, tee, fakesink];
        imp.inner.add_many(&elements)?;
        gst::Element::link_many(&elements)?;

        for element in &elements {
            element.sync_state_with_parent()?;
        }

        imp.desktop_audio_elements.replace(elements);

        tracing::debug!("Loaded desktop audio from {}", device_name);

        Ok(())
    }

    pub fn unload_desktop_audio(&self) -> Result<()> {
        let imp = self.imp();

        for element in imp.desktop_audio_elements.take() {
            element.set_state(gst::State::Null)?;
            imp.inner.remove(&element)?;
        }

        tracing::debug!("Unloaded desktop audio");

        Ok(())
    }

    pub async fn load_microphone(&self) -> Result<()> {
        let imp = self.imp();

        if !imp.microphone_elements.borrow().is_empty() {
            return Ok(());
        }

        let device_name = audio_device::find_default_name(AudioDeviceClass::Source)
            .await
            .context("No desktop audio source found")?;

        let pulsesrc = gst::ElementFactory::make("pulsesrc")
            .property("device", &device_name)
            .build()?;
        let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
        let level = gst::ElementFactory::make("level")
            .name(MICROPHONE_LEVEL_NAME)
            .property("interval", gst::ClockTime::from_mseconds(80))
            .property("peak-ttl", gst::ClockTime::from_mseconds(80))
            .build()?;
        let tee = gst::ElementFactory::make("tee")
            .name(MICROPHONE_TEE)
            .build()?;
        let fakesink = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()?;

        let elements = vec![pulsesrc, audioconvert, level, tee, fakesink];
        imp.inner.add_many(&elements)?;
        gst::Element::link_many(&elements)?;

        for element in &elements {
            element.sync_state_with_parent()?;
        }

        imp.microphone_elements.replace(elements);

        tracing::debug!("Loaded microphone from {}", device_name);

        Ok(())
    }

    pub fn unload_microphone(&self) -> Result<()> {
        let imp = self.imp();

        for element in imp.microphone_elements.take() {
            element.set_state(gst::State::Null)?;
            imp.inner.remove(&element)?;
        }

        tracing::debug!("Unloaded microphone");

        Ok(())
    }

    fn set_recording_state(&self, recording_state: RecordingState) {
        let imp = self.imp();

        if recording_state == imp.recording_state.get() {
            return;
        }

        imp.recording_state.set(recording_state);
        self.notify_recording_state();
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        let imp = self.imp();

        match message.view() {
            gst::MessageView::Element(e) => {
                tracing::trace!(?message, "Element message from bus");

                if let Some(src) = e.src() {
                    if let Some(structure) = e.structure() {
                        if structure.has_name("level") {
                            let peaks = structure.get::<&glib::ValueArray>("rms").unwrap();
                            let left_peak = peaks.nth(0).unwrap().get::<f64>().unwrap();
                            let right_peak = peaks.nth(1).unwrap().get::<f64>().unwrap();

                            let normalized_left_peak = 10_f64.powf(left_peak / 20.0);
                            let normalized_right_peak = 10_f64.powf(right_peak / 20.0);

                            match src.name().as_str() {
                                DESKTOP_AUDIO_LEVEL_NAME => {
                                    self.emit_by_name::<()>(
                                        "desktop-audio-peak",
                                        &[&Peaks::new(normalized_left_peak, normalized_right_peak)],
                                    );
                                }
                                MICROPHONE_LEVEL_NAME => {
                                    self.emit_by_name::<()>(
                                        "microphone-peak",
                                        &[&Peaks::new(normalized_left_peak, normalized_right_peak)],
                                    );
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                }

                glib::ControlFlow::Continue
            }
            gst::MessageView::StateChanged(sc) => {
                if message
                    .src()
                    .is_some_and(|src| src == imp.inner.upcast_ref::<gst::Object>())
                {
                    tracing::debug!(
                        "Pipeline changed state from `{:?}` -> `{:?}`",
                        sc.old(),
                        sc.current(),
                    );
                }

                glib::ControlFlow::Continue
            }
            gst::MessageView::Error(e) => {
                tracing::error!(src = ?e.src(), error = ?e.error(), debug = ?e.debug(), "Error from bus");

                glib::ControlFlow::Break
            }
            gst::MessageView::Warning(w) => {
                tracing::warn!("Warning from bus: {:?}", w);

                glib::ControlFlow::Continue
            }
            gst::MessageView::Info(i) => {
                tracing::debug!("Info from bus: {:?}", i);

                glib::ControlFlow::Continue
            }
            _ => {
                tracing::trace!(?message, "Message from bus");

                glib::ControlFlow::Continue
            }
        }
    }

    fn update_stream_size(&self) {
        let imp = self.imp();

        let compositor = imp.inner.by_name(COMPOSITOR_NAME).unwrap();
        let stream_size = compositor.static_pad("src").unwrap().caps().map(|caps| {
            let caps_struct = caps.structure(0).unwrap();
            let stream_width = caps_struct.get::<i32>("width").unwrap();
            let stream_height = caps_struct.get::<i32>("height").unwrap();
            StreamSize::new(stream_width, stream_height)
        });

        imp.stream_size.set(stream_size);
        self.notify_stream_size();
    }

    fn setup_elements(&self) -> Result<()> {
        let imp = self.imp();

        let compositor = gst::ElementFactory::make("compositor")
            .name(COMPOSITOR_NAME)
            .build()?;
        let convert = gst::ElementFactory::make("videoconvert")
            .property("chroma-mode", gst_video::VideoChromaMode::None)
            .property("dither", gst_video::VideoDitherMethod::None)
            .property("matrix-mode", gst_video::VideoMatrixMode::OutputOnly)
            .property("n-threads", utils::ideal_thread_count())
            .build()?;
        let tee = gst::ElementFactory::make("tee")
            .name(VIDEO_TEE_NAME)
            .build()?;
        let sink = gst::ElementFactory::make("gtk4paintablesink")
            .name(PAINTABLE_SINK_NAME)
            .build()?;

        imp.inner.add_many([&compositor, &convert, &tee, &sink])?;
        gst::Element::link_many([&compositor, &convert, &tee])?;

        let tee_src_pad = tee
            .request_pad_simple("src_%u")
            .context("Failed to request sink_%u pad from compositor")?;
        tee_src_pad.link(&sink.static_pad("sink").unwrap())?;

        let bus_watch_guard = imp.inner.bus().unwrap().add_watch_local(
            clone!(@weak self as obj => @default-panic, move |_, message| {
                obj.handle_bus_message(message)
            }),
        )?;
        imp.bus_watch_guard.replace(Some(bus_watch_guard));

        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        compositor
            .static_pad("src")
            .unwrap()
            .connect_caps_notify(move |_| {
                tx.send(()).unwrap();
            });
        let source_id = rx.attach(
            None,
            clone!(@weak self as obj => @default-panic, move |_| {
                obj.update_stream_size();
                glib::ControlFlow::Continue
            }),
        );
        imp.caps_notify_source_id.replace(Some(source_id));

        Ok(())
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a videocrop element that computes the crop from the given coordinates
/// and size.
fn videocrop_compute(data: &CropData, stream_size: StreamSize) -> Result<gst::Element> {
    let scale_h = stream_size.width() as f32 / data.full_rect.width();
    let scale_v = stream_size.height() as f32 / data.full_rect.height();

    if scale_h != scale_v {
        tracing::warn!(
            scale_h,
            scale_v,
            "Scale factors of horizontal and vertical are unequal"
        );
    }

    // Both selection and full rect position are relative to the widget coordinates.
    // To get the absolute position and so correct crop values, subtract the full
    // rect's position from the selection rect.
    let x = (data.selection_rect.x() - data.full_rect.x()) * scale_h;
    let y = (data.selection_rect.y() - data.full_rect.y()) * scale_v;
    let width = data.selection_rect.width() * scale_h;
    let height = data.selection_rect.height() * scale_v;

    tracing::trace!(x, y, width, height);

    // x264enc requires even resolution.
    let top_crop = round_to_even_f32(y);
    let left_crop = round_to_even_f32(x);
    let right_crop = round_to_even_f32(stream_size.width() as f32 - (x + width));
    let bottom_crop = round_to_even_f32(stream_size.height() as f32 - (y + height));

    tracing::trace!(top_crop, left_crop, right_crop, bottom_crop);

    let crop = gst::ElementFactory::make("videocrop")
        .property("top", top_crop.clamp(0, stream_size.height()))
        .property("left", left_crop.clamp(0, stream_size.width()))
        .property("right", right_crop.clamp(0, stream_size.width()))
        .property("bottom", bottom_crop.clamp(0, stream_size.height()))
        .build()?;
    Ok(crop)
}

fn round_to_even(number: i32) -> i32 {
    number / 2 * 2
}

fn round_to_even_f32(number: f32) -> i32 {
    (number / 2.0).round() as i32 * 2
}
