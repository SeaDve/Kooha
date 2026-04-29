use anyhow::{Context, Ok, Result, bail, ensure};
use gst::prelude::*;
use gtk::graphene::Rect;

use std::{os::unix::io::RawFd, path::PathBuf};

use crate::{
    area_selector::SelectAreaData,
    device::{self, DeviceClass},
    profile::Profile,
    screencast_portal::Stream,
    webcam,
};

const AUDIO_SAMPLE_RATE: i32 = 48_000;

#[derive(Debug)]
#[must_use]
pub struct PipelineBuilder {
    file_path: PathBuf,
    framerate: gst::Fraction,
    profile: Profile,
    fd: RawFd,
    streams: Vec<Stream>,
    record_desktop_audio: bool,
    record_microphone: bool,
    select_area_data: Option<SelectAreaData>,
    webcam_overlay: bool,
    webcam_device_path: Option<String>,
}

impl PipelineBuilder {
    pub fn new(
        file_path: PathBuf,
        framerate: gst::Fraction,
        profile: Profile,
        fd: RawFd,
        streams: Vec<Stream>,
    ) -> Self {
        Self {
            file_path,
            framerate,
            profile,
            fd,
            streams,
            record_desktop_audio: false,
            record_microphone: false,
            select_area_data: None,
            webcam_overlay: false,
            webcam_device_path: None,
        }
    }

    pub fn record_desktop_audio(&mut self, record_desktop_audio: bool) -> &mut Self {
        self.record_desktop_audio = record_desktop_audio;
        self
    }

    pub fn record_microphone(&mut self, record_microphone: bool) -> &mut Self {
        self.record_microphone = record_microphone;
        self
    }

    pub fn select_area_data(&mut self, data: SelectAreaData) -> &mut Self {
        self.select_area_data = Some(data);
        self
    }

    pub fn webcam_overlay(&mut self, enabled: bool, device_path: String) -> &mut Self {
        self.webcam_overlay = enabled;
        self.webcam_device_path = Some(device_path);
        self
    }

    /// Builds the pipeline.
    ///
    /// Without webcam overlay:
    ///                   (If has select_area_data)
    ///                        |             |
    ///                        v             v
    /// pipewiresrc-bin -> videoscale -> videocrop -> queue -> |
    ///                                                        | -> profile.attach -> filesink
    ///                               pulsesrc-bin -> queue -> |
    ///
    /// With webcam overlay:
    /// pipewiresrc-bin -> ... -> queue -> |                        
    ///                                     | -> compositor -> queue -> |
    /// v4l2src (webcam) -> circle -> |   |                            | -> profile.attach -> filesink
    ///                               pulsesrc-bin -> queue -> |
    pub fn build(&self) -> Result<gst::Pipeline> {
        tracing::debug!(
            file_path = %self.file_path.display(),
            framerate = ?self.framerate,
            profile = ?self.profile.id(),
            fd = self.fd,
            stream_len = self.streams.len(),
            streams = ?self.streams,
            record_desktop_audio = ?self.record_desktop_audio,
            record_microphone = ?self.record_microphone,
            select_area_data = ?self.select_area_data,
            webcam_overlay = self.webcam_overlay,
            webcam_device_path = ?self.webcam_device_path,
        );

        let pipeline = gst::Pipeline::new();

        let videosrc_bin = make_videosrc_bin(self.fd, &self.streams, self.framerate)
            .context("Failed to create videosrc bin")?;
        let videoenc_queue = gst::ElementFactory::make("queue")
            .name("kooha-videoenc-queue")
            .build()?;
        let filesink = gst::ElementFactory::make("filesink")
            .property(
                "location",
                self.file_path
                    .to_str()
                    .context("Could not convert file path to string")?,
            )
            .build()?;
        pipeline.add_many([videosrc_bin.upcast_ref(), &videoenc_queue, &filesink])?;

        let screen_video_output = if let Some(ref data) = self.select_area_data {
            let videoscale = gst::ElementFactory::make("videoscale").build()?;
            let videocrop = make_videocrop(data)?;
            pipeline.add_many([&videoscale, &videocrop])?;

            // x264enc requires even resolution.
            let (stream_width, stream_height) = data.stream_size;
            let videoscale_caps = gst::Caps::builder("video/x-raw")
                .field("width", round_to_even(stream_width))
                .field("height", round_to_even(stream_height))
                .build();

            videosrc_bin.link(&videoscale)?;
            videoscale.link_filtered(&videocrop, &videoscale_caps)?;
            // Return the element that outputs screen video (videocrop)
            videocrop
        } else {
            videosrc_bin.link(&videoenc_queue)?;
            videoenc_queue.clone()
        };

        // If webcam overlay is enabled, use compositor to combine screen + webcam
        let final_video_output = if self.webcam_overlay {
            let device_path = self
                .webcam_device_path
                .as_ref()
                .context("Webcam overlay enabled but no device path set")?;

            let webcam_size = 120; // Size of the webcam overlay in pixels
            let webcam_bin = webcam::make_webcam_bin(device_path, webcam_size)
                .context("Failed to create webcam bin")?;

            // Compositor to overlay webcam on screen
            let compositor = gst::ElementFactory::make("compositor")
                .name("kooha-overlay-compositor")
                .property("background-alpha", 0u8)
                .build()?;
            let compositor_queue = gst::ElementFactory::make("queue")
                .name("kooha-compositor-queue")
                .build()?;

            pipeline.add_many([webcam_bin.upcast_ref(), &compositor, &compositor_queue])?;

            // Connect screen video to compositor sink_0
            let screen_sink_pad = compositor
                .request_pad_simple("sink_0")
                .context("Failed to request sink_0 pad from compositor")?;
            screen_video_output
                .static_pad("src")
                .unwrap()
                .link(&screen_sink_pad)?;

            // Connect webcam to compositor sink_1, positioned bottom-left with margin
            let webcam_sink_pad = compositor
                .request_pad_simple("sink_1")
                .context("Failed to request sink_1 pad from compositor")?;
            // Bottom-left position with 20px margin
            webcam_sink_pad.set_property("xpos", 20i32);
            webcam_sink_pad.set_property("ypos", -1i32); // -1 means bottom-aligned (auto)
            webcam_sink_pad.set_property("zorder", 1i32);
            webcam_bin
                .static_pad("src")
                .unwrap()
                .link(&webcam_sink_pad)?;

            compositor.link(&compositor_queue)?;
            compositor_queue.link(&videoenc_queue)?;

            compositor_queue
        } else {
            screen_video_output
        };

        // Link the final video output to the encoder queue (if not already linked)
        if !self.webcam_overlay {
            // Already linked above
        }

        let audioenc_queue = if self.record_desktop_audio || self.record_microphone {
            debug_assert!(self.profile.supports_audio());

            let pulsesrcs = [
                self.record_desktop_audio
                    .then(|| make_pulsesrc(DeviceClass::Sink, "kooha-desktop-audio-src")),
                self.record_microphone
                    .then(|| make_pulsesrc(DeviceClass::Source, "kooha-microphone-src")),
            ];
            let audiosrc_bin = make_audiosrc_bin(
                &pulsesrcs
                    .into_iter()
                    .flatten()
                    .collect::<Result<Vec<_>>>()?,
            )
            .context("Failed to create audiosrc bin")?;
            let audioenc_queue = gst::ElementFactory::make("queue")
                .name("kooha-audioenc-queue")
                .build()?;

            pipeline.add_many([audiosrc_bin.upcast_ref(), &audioenc_queue])?;
            audiosrc_bin.link(&audioenc_queue)?;

            Some(audioenc_queue)
        } else {
            None
        };

        // Link final video output to encoder queue
        if !self.webcam_overlay {
            // screen_video_output is already linked to videoenc_queue
        }

        self.profile
            .attach(
                &pipeline,
                &videoenc_queue,
                audioenc_queue.as_ref(),
                &filesink,
            )
            .with_context(|| {
                format!(
                    "Failed to attach profile `{}` to pipeline",
                    self.profile.id()
                )
            })?;

        Ok(pipeline)
    }
}

fn make_pipewiresrc(fd: RawFd, path: &str) -> Result<gst::Element> {
    let src = gst::ElementFactory::make("pipewiresrc")
        .property("fd", fd)
        .property("path", path)
        .property("do-timestamp", true)
        .property("provide-clock", false)
        .property("keepalive-time", 1000)
        .property("resend-last", true)
        .build()?;

    Ok(src)
}

fn make_videoflip() -> Result<gst::Element> {
    let videoflip = gst::ElementFactory::make("videoflip")
        .property_from_str("video-direction", "auto")
        .build()?;

    Ok(videoflip)
}

/// Create a videocrop element that computes the crop from the given coordinates
/// and size.
fn make_videocrop(data: &SelectAreaData) -> Result<gst::Element> {
    let SelectAreaData {
        selection,
        paintable_rect,
        stream_size,
    } = data;

    let (stream_width, stream_height) = stream_size;
    let scale_factor_h = *stream_width as f32 / paintable_rect.width();
    let scale_factor_v = *stream_height as f32 / paintable_rect.height();

    if scale_factor_h != scale_factor_v {
        tracing::warn!(
            scale_factor_h,
            scale_factor_v,
            "Scale factors of horizontal and vertical are unequal"
        );
    }

    // Both paintable and selection position are relative to the widget coordinates.
    // To get the absolute position and so correct crop values, subtract the paintable
    // rect's position from the selection rect.
    let old_selection_rect = selection.rect();
    let selection_rect_scaled = Rect::new(
        old_selection_rect.x() - paintable_rect.x(),
        old_selection_rect.y() - paintable_rect.y(),
        old_selection_rect.width(),
        old_selection_rect.height(),
    )
    .scale(scale_factor_h, scale_factor_v);

    let raw_top_crop = selection_rect_scaled.y();
    let raw_left_crop = selection_rect_scaled.x();
    let raw_right_crop =
        *stream_width as f32 - (selection_rect_scaled.width() + selection_rect_scaled.x());
    let raw_bottom_crop =
        *stream_height as f32 - (selection_rect_scaled.height() + selection_rect_scaled.y());

    tracing::debug!(raw_top_crop, raw_left_crop, raw_right_crop, raw_bottom_crop);

    let top_crop = round_to_even_f32(raw_top_crop).clamp(0, *stream_height);
    let left_crop = round_to_even_f32(raw_left_crop).clamp(0, *stream_width);
    let right_crop = round_to_even_f32(raw_right_crop).clamp(0, *stream_width);
    let bottom_crop = round_to_even_f32(raw_bottom_crop).clamp(0, *stream_height);

    tracing::debug!(top_crop, left_crop, right_crop, bottom_crop);

    // x264enc requires even resolution.
    let crop = gst::ElementFactory::make("videocrop")
        .property("top", top_crop)
        .property("left", left_crop)
        .property("right", right_crop)
        .property("bottom", bottom_crop)
        .build()?;
    Ok(crop)
}

/// Creates a bin with a src pad for multiple pipewire streams.
///
/// Single stream:
///
/// pipewiresrc -> videoflip -> videorate
///
/// Multiple streams:
///
/// pipewiresrc1 -> videoflip -> |
///                              |
/// pipewiresrc2 -> videoflip -> | -> compositor -> videorate
///                              |
/// pipewiresrcn -> videoflip -> |
pub fn make_videosrc_bin(
    fd: RawFd,
    streams: &[Stream],
    framerate: gst::Fraction,
) -> Result<gst::Bin> {
    // TODO Create a bin that hotswaps compositor depending whether gl is supported or not.

    let bin = gst::Bin::builder().name("kooha-pipewiresrc-bin").build();

    let videorate = gst::ElementFactory::make("videorate")
        .property("skip-to-first", true)
        .build()?;
    let videorate_capsfilter = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("framerate", framerate)
                .build(),
        )
        .build()?;
    bin.add_many([&videorate, &videorate_capsfilter])?;
    videorate.link(&videorate_capsfilter)?;

    match streams {
        [] => bail!("No streams provided"),
        [stream] => {
            let pipewiresrc = make_pipewiresrc(fd, &stream.node_id().to_string())?;
            let videoflip = make_videoflip()?;
            bin.add_many([&pipewiresrc, &videoflip])?;
            gst::Element::link_many([&pipewiresrc, &videoflip, &videorate])?;
        }
        streams => {
            let compositor = gst::ElementFactory::make("compositor").build()?;
            bin.add(&compositor)?;
            compositor.link(&videorate)?;

            let mut last_pos = 0;
            for stream in streams {
                let pipewiresrc = make_pipewiresrc(fd, &stream.node_id().to_string())?;
                let videoflip = make_videoflip()?;
                bin.add_many([&pipewiresrc, &videoflip])?;
                pipewiresrc.link(&videoflip)?;

                let compositor_sink_pad = compositor
                    .request_pad_simple("sink_%u")
                    .context("Failed to request sink_%u pad from compositor")?;
                compositor_sink_pad.set_property("xpos", last_pos);
                videoflip
                    .static_pad("src")
                    .unwrap()
                    .link(&compositor_sink_pad)?;

                let (stream_width, _) = stream.size().unwrap();
                last_pos += stream_width;
            }
        }
    }

    let src_pad = videorate_capsfilter.static_pad("src").unwrap();
    bin.add_pad(&gst::GhostPad::with_target(&src_pad)?)?;

    Ok(bin)
}

/// Creates a new audio src element with the given name.
///
/// If the class is already a source, it will return the device name as is,
/// otherwise, if it is a sink, it will append `.monitor` to the device name.
fn make_pulsesrc(class: DeviceClass, element_name: &str) -> Result<gst::Element> {
    let device = device::find_default(class)?;

    let pulsesrc = gst::ElementFactory::make("pulsesrc")
        .name(element_name)
        .property("provide-clock", false)
        .property("do-timestamp", true)
        .build()?;

    match class {
        DeviceClass::Sink => {
            let pulsesink = device.create_element(None)?;
            let device_name = pulsesink
                .property::<Option<String>>("device")
                .context("No device name")?;
            ensure!(!device_name.is_empty(), "Empty device name");

            let monitor_name = format!("{}.monitor", device_name);
            pulsesrc.set_property("device", &monitor_name);

            tracing::debug!("Found desktop audio with name `{}`", monitor_name);
        }
        DeviceClass::Source => {
            device.reconfigure_element(&pulsesrc)?;

            let device_name = pulsesrc
                .property::<Option<String>>("device")
                .context("No device name")?;
            ensure!(!device_name.is_empty(), "Empty device name");

            tracing::debug!("Found microphone with name `{}`", device_name);
        }
    }

    Ok(pulsesrc)
}

/// Creates a bin with a src pad for a pulse audio device
///
/// pulsesrc1 -> audiorate -> |
///                           |
/// pulsesrc2 -> audiorate -> | -> audiomixer
///                           |
/// pulsesrcn -> audiorate -> |
fn make_audiosrc_bin<'a>(
    pulsesrcs: impl IntoIterator<Item = &'a gst::Element>,
) -> Result<gst::Bin> {
    let bin = gst::Bin::builder().name("kooha-pulsesrc-bin").build();

    let caps = gst::Caps::builder_full()
        .structure(
            gst::Structure::builder("audio/x-raw")
                .field("rate", AUDIO_SAMPLE_RATE)
                .field("channels", 2)
                .build(),
        )
        .structure(
            gst::Structure::builder("audio/x-raw")
                .field("rate", AUDIO_SAMPLE_RATE)
                .field("channels", 1)
                .build(),
        )
        .build();

    let audiomixer = gst::ElementFactory::make("audiomixer")
        .property("latency", gst::ClockTime::from_seconds(2))
        .build()?;
    let audiomixer_capsfilter = gst::ElementFactory::make("capsfilter")
        .property("caps", &caps)
        .build()?;
    bin.add_many([&audiomixer, &audiomixer_capsfilter])?;
    audiomixer.link(&audiomixer_capsfilter)?;

    let src_pad = audiomixer_capsfilter.static_pad("src").unwrap();
    bin.add_pad(&gst::GhostPad::with_target(&src_pad)?)?;

    for pulsesrc in pulsesrcs {
        let audiorate = gst::ElementFactory::make("audiorate")
            .property("skip-to-first", true)
            .build()?;

        bin.add_many([pulsesrc, &audiorate])?;
        pulsesrc.link_filtered(&audiorate, &caps)?;
        audiorate.link_pads(None, &audiomixer, Some("sink_%u"))?;
    }

    Ok(bin)
}

fn round_to_even(number: i32) -> i32 {
    number / 2 * 2
}

fn round_to_even_f32(number: f32) -> i32 {
    (number / 2.0).round() as i32 * 2
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! assert_even {
        ($number:expr) => {
            assert_eq!($number % 2, 0)
        };
    }

    #[test]
    fn odd_round_to_even() {
        assert_even!(round_to_even(5));
        assert_even!(round_to_even(101));
    }

    #[test]
    fn odd_round_to_even_f32() {
        assert_even!(round_to_even_f32(3.0));
        assert_even!(round_to_even_f32(99.0));
    }

    #[test]
    fn even_round_to_even() {
        assert_even!(round_to_even(50));
        assert_even!(round_to_even(4));
    }

    #[test]
    fn even_round_to_even_f32() {
        assert_even!(round_to_even_f32(300.0));
        assert_even!(round_to_even_f32(6.0));
    }

    #[test]
    fn float_round_to_even_f32() {
        assert_even!(round_to_even_f32(5.3));
        assert_even!(round_to_even_f32(2.9));
    }
}
