use anyhow::{bail, Context, Ok, Result};
use gst::prelude::*;
use gtk::{
    glib,
    graphene::{Rect, Size},
};

use std::{
    ffi::OsStr,
    os::unix::io::RawFd,
    path::{Path, PathBuf},
};

use crate::{profile::Profile, screencast_session::Stream, utils};

// TODO
// * Do we need restrictions?
// * Can we drop filter elements (videorate, videoconvert, videoscale, audioconvert) and let encodebin handle it?
// * Can we set frame rate directly on profile format?
// * Add tests

#[derive(Debug)]
struct SelectAreaContext {
    pub coords: Rect,
    pub screen_size: Size,
}

#[derive(Debug)]
#[must_use]
pub struct PipelineBuilder {
    saving_location: PathBuf,
    framerate: u32,
    profile: Box<dyn Profile>,
    fd: RawFd,
    streams: Vec<Stream>,
    speaker_source: Option<String>,
    mic_source: Option<String>,
    select_area_context: Option<SelectAreaContext>,
}

impl PipelineBuilder {
    pub fn new(
        saving_location: &Path,
        framerate: u32,
        profile: Box<dyn Profile>,
        fd: RawFd,
        streams: Vec<Stream>,
    ) -> Self {
        Self {
            saving_location: saving_location.to_path_buf(),
            framerate,
            profile,
            fd,
            streams,
            speaker_source: None,
            mic_source: None,
            select_area_context: None,
        }
    }

    pub fn speaker_source(&mut self, speaker_source: String) -> &mut Self {
        self.speaker_source = Some(speaker_source);
        self
    }

    pub fn mic_source(&mut self, mic_source: String) -> &mut Self {
        self.mic_source = Some(mic_source);
        self
    }

    pub fn select_area_context(&mut self, coords: Rect, screen_size: Size) -> &mut Self {
        self.select_area_context = Some(SelectAreaContext {
            coords,
            screen_size,
        });
        self
    }

    pub fn build(&self) -> Result<gst::Pipeline> {
        let file_path = new_recording_path(&self.saving_location, self.profile.file_extension());

        let queue = utils::make_element("queue")?;
        let filesink = utils::make_named_element("filesink", "filesink")?;
        filesink.set_property(
            "location",
            file_path
                .to_str()
                .context("Could not convert file path to string")?,
        );

        let pipeline = gst::Pipeline::new(None);
        pipeline.add_many(&[&queue, &filesink])?;
        gst::Element::link_many(&[&queue, &filesink])?;

        tracing::debug!(
            file_path = %file_path.display(),
            framerate = self.framerate,
            profile = ?self.profile,
            stream_len = self.streams.len(),
            streams = ?self.streams,
            speaker_source = ?self.speaker_source,
            mic_source = ?self.mic_source,
        );

        let videosrc_bin = match self.streams.len() {
            0 => bail!("Found no streams"),
            1 => single_stream_pipewiresrc_bin(
                self.fd,
                self.streams.get(0).unwrap(),
                self.framerate,
                self.select_area_context.as_ref(),
            )?,
            _ => {
                if self.select_area_context.is_some() {
                    bail!("Select area is not supported for multiple streams");
                }

                multi_stream_pipewiresrc_bin(self.fd, &self.streams, self.framerate)?
            }
        };

        pipeline.add(&videosrc_bin)?;

        let audio_srcs = if self.profile.supports_audio() {
            let mut audio_srcs = Vec::new();
            for audio_device_name in [&self.speaker_source, &self.mic_source]
                .into_iter()
                .flatten()
            {
                let audio_src_bin = pulsesrc_bin(audio_device_name)?;
                pipeline.add(&audio_src_bin)?;
                audio_srcs.push(audio_src_bin.upcast());
            }
            audio_srcs
        } else {
            if self.speaker_source.is_some() || self.mic_source.is_some() {
                tracing::warn!("Audio is not supported by the selected profile, but audio sources are provided");
            }

            Vec::new()
        };

        self.profile
            .attach(&pipeline, videosrc_bin.upcast_ref(), &audio_srcs, &queue)?;

        Ok(pipeline)
    }
}

fn pipewiresrc_with_default(fd: RawFd, path: &str) -> Result<gst::Element> {
    let src = utils::make_element("pipewiresrc")?;
    src.set_property("fd", &fd);
    src.set_property("path", path);
    src.set_property("do-timestamp", true);
    src.set_property("keepalive-time", 1000);
    src.set_property("resend-last", true);
    Ok(src)
}

fn videoconvert_with_default() -> Result<gst::Element> {
    let conv = utils::make_element("videoconvert")?;
    conv.set_property("chroma-mode", gst_video::VideoChromaMode::None);
    conv.set_property("dither", gst_video::VideoDitherMethod::None);
    conv.set_property("matrix-mode", gst_video::VideoMatrixMode::OutputOnly);
    conv.set_property("n-threads", utils::ideal_thread_count());
    Ok(conv)
}

/// Create a videocrop element that computes the crop from the given coordinates
/// and size.
fn videocrop_compute(
    stream_width: i32,
    stream_height: i32,
    context: &SelectAreaContext,
) -> Result<gst::Element> {
    let actual_screen = context.screen_size;

    let scale_factor = stream_width as f32 / actual_screen.width();
    let coords = context.coords.scale(scale_factor, scale_factor);

    let top_crop = coords.y();
    let left_crop = coords.x();
    let right_crop = stream_width as f32 - (coords.width() + coords.x());
    let bottom_crop = stream_height as f32 - (coords.height() + coords.y());

    tracing::debug!(top_crop, left_crop, right_crop, bottom_crop);

    // x264enc requires even resolution.
    let crop = utils::make_element("videocrop")?;
    crop.set_property("top", round_to_even_f32(top_crop));
    crop.set_property("left", round_to_even_f32(left_crop));
    crop.set_property("right", round_to_even_f32(right_crop));
    crop.set_property("bottom", round_to_even_f32(bottom_crop));
    Ok(crop)
}

/// Creates a bin with a src pad for multiple pipewire streams.
///
/// pipewiresrc1 -> videorate -> |
///                              | -> compositor -> videoconvert -> queue
/// pipewiresrc2 -> videorate -> |
fn multi_stream_pipewiresrc_bin(fd: RawFd, streams: &[Stream], framerate: u32) -> Result<gst::Bin> {
    let bin = gst::Bin::new(None);

    let compositor = utils::make_element("compositor")?;
    let videoconvert = videoconvert_with_default()?;
    let queue = utils::make_element("queue")?;

    bin.add_many(&[&compositor, &videoconvert, &queue])?;
    gst::Element::link_many(&[&compositor, &videoconvert, &queue])?;

    let videorate_filter = gst::Caps::builder("video/x-raw")
        .field("framerate", gst::Fraction::new(framerate as i32, 1))
        .build();

    let mut last_pos = 0;
    for stream in streams {
        let pipewiresrc = pipewiresrc_with_default(fd, &stream.node_id().to_string())?;
        let videorate = utils::make_element("videorate")?;
        let videorate_capsfilter = utils::make_element("capsfilter")?;
        videorate_capsfilter.set_property("caps", &videorate_filter);

        bin.add_many(&[&pipewiresrc, &videorate, &videorate_capsfilter])?;
        gst::Element::link_many(&[&pipewiresrc, &videorate, &videorate_capsfilter])?;

        let compositor_sink_pad = compositor
            .request_pad_simple("sink_%u")
            .context("Failed to request sink_%u pad from compositor")?;
        compositor_sink_pad.set_property("xpos", last_pos);
        videorate_capsfilter
            .static_pad("src")
            .unwrap()
            .link(&compositor_sink_pad)?;

        let stream_width = stream.size().unwrap().0;
        last_pos += stream_width;
    }

    let queue_pad = queue.static_pad("src").unwrap();
    bin.add_pad(&gst::GhostPad::with_target(Some("src"), &queue_pad)?)?;

    Ok(bin)
}

/// Creates a bin with a src pad for a single pipewire stream.
///
/// No selection:
/// pipewiresrc -> videconvert -> videorate -> queue
///
/// Has selection:
/// pipewiresrc -> videconvert -> videorate -> videoscale -> videocrop -> queue
fn single_stream_pipewiresrc_bin(
    fd: RawFd,
    stream: &Stream,
    framerate: u32,
    select_area_context: Option<&SelectAreaContext>,
) -> Result<gst::Bin> {
    let bin = gst::Bin::new(None);

    let pipewiresrc = pipewiresrc_with_default(fd, &stream.node_id().to_string())?;
    let videoconvert = videoconvert_with_default()?;
    let videorate = utils::make_element("videorate")?;
    let queue = utils::make_element("queue")?;

    bin.add_many(&[&pipewiresrc, &videoconvert, &videorate, &queue])?;
    gst::Element::link_many(&[&pipewiresrc, &videoconvert, &videorate])?;

    let videorate_filter = gst::Caps::builder("video/x-raw")
        .field("framerate", gst::Fraction::new(framerate as i32, 1))
        .build();

    if let Some(context) = select_area_context {
        let (stream_width, stream_height) = stream.size().context("Stream has no size")?;

        let videoscale = utils::make_element("videoscale")?;
        let videocrop = videocrop_compute(stream_width, stream_height, context)?;

        // x264enc requires even resolution.
        let videoscale_filter = gst::Caps::builder("video/x-raw")
            .field("width", round_to_even(stream_width))
            .field("height", round_to_even(stream_height))
            .build();

        bin.add_many(&[&videoscale, &videocrop])?;
        videorate.link_filtered(&videoscale, &videorate_filter)?;
        videoscale.link_filtered(&videocrop, &videoscale_filter)?;
        gst::Element::link_many(&[&videocrop, &queue])?;
    } else {
        videorate.link_filtered(&queue, &videorate_filter)?;
    }

    let queue_pad = queue.static_pad("src").unwrap();
    bin.add_pad(&gst::GhostPad::with_target(Some("src"), &queue_pad)?)?;

    Ok(bin)
}

/// Creates a bin with a src pad for a pulse audio device
///
/// pulsesrc -> audioconvert -> queue
fn pulsesrc_bin(device_name: &str) -> Result<gst::Bin> {
    let bin = gst::Bin::new(None);

    let pulsesrc = utils::make_element("pulsesrc")?;
    pulsesrc.set_property("device", device_name);
    let audioconvert = utils::make_element("audioconvert")?;
    let queue = utils::make_element("queue")?;

    bin.add_many(&[&pulsesrc, &audioconvert, &queue])?;
    gst::Element::link_many(&[&pulsesrc, &audioconvert, &queue])?;

    let queue_pad = queue.static_pad("src").unwrap();
    bin.add_pad(&gst::GhostPad::with_target(Some("src"), &queue_pad)?)?;

    Ok(bin)
}

fn round_to_even(number: i32) -> i32 {
    number / 2 * 2
}

fn round_to_even_f32(number: f32) -> i32 {
    number as i32 / 2 * 2
}

fn new_recording_path(saving_location: &Path, extension: impl AsRef<OsStr>) -> PathBuf {
    let file_name = glib::DateTime::now_local()
        .expect("You are somehow on year 9999")
        .format("Kooha-%F-%H-%M-%S")
        .expect("Invalid format string");

    let mut path = saving_location.join(file_name);
    path.set_extension(extension);

    path
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
