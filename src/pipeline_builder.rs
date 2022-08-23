use anyhow::{bail, Context, Result};
use gst::prelude::*;
use gst_pbutils::prelude::*;
use gtk::{
    glib,
    graphene::{Rect, Size},
};

use std::{
    cmp,
    os::unix::io::RawFd,
    path::{Path, PathBuf},
};

use crate::{screencast_session::Stream, settings::VideoFormat};

const MAX_THREAD_COUNT: u32 = 64;
const DEFAULT_GIF_FRAMERATE: u32 = 15;

#[derive(Debug)]
struct SelectAreaContext {
    pub coords: Rect,
    pub screen_size: Size,
}

#[derive(Debug)]
#[must_use]
pub struct PipelineBuilder {
    file_path: PathBuf,
    framerate: u32,
    format: VideoFormat,
    fd: RawFd,
    streams: Vec<Stream>,
    speaker_source: Option<String>,
    mic_source: Option<String>,
    select_area_context: Option<SelectAreaContext>,
}

impl PipelineBuilder {
    pub fn new(
        file_path: &Path,
        framerate: u32,
        format: VideoFormat,
        fd: RawFd,
        streams: Vec<Stream>,
    ) -> Self {
        Self {
            file_path: file_path.to_path_buf(),
            framerate,
            format,
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
        let pipeline = gst::Pipeline::new(None);

        let encodebin = element_factory_make("encodebin")?;
        encodebin.set_property("profile", &create_profile(self.format));
        encodebin.set_property("avoid-reencoding", true);
        let queue = element_factory_make("queue")?;
        let filesink = element_factory_make("filesink")?;
        filesink.set_property(
            "location",
            self.file_path
                .to_str()
                .context("Could not convert file path to string")?,
        );

        pipeline.add_many(&[&encodebin, &queue, &filesink])?;
        gst::Element::link_many(&[&encodebin, &queue, &filesink])?;

        let framerate = match self.format {
            VideoFormat::Gif => DEFAULT_GIF_FRAMERATE,
            _ => self.framerate,
        };

        tracing::debug!(stream_len = ?self.streams.len());

        let videosrc_bin = match self.streams.len() {
            0 => bail!("Found no streams"),
            1 => single_stream_pipewiresrc_bin(
                self.fd,
                self.streams.get(0).unwrap(),
                framerate,
                self.select_area_context.as_ref(),
            )?,
            _ => {
                if self.select_area_context.is_some() {
                    bail!("Select area is not supported for multiple streams");
                }

                multi_stream_pipewiresrc_bin(self.fd, &self.streams, framerate)?
            }
        };

        pipeline.add(&videosrc_bin)?;
        videosrc_bin.static_pad("src").unwrap().link(
            &encodebin
                .request_pad_simple("video_%u")
                .context("Failed to request video_%u pad from encodebin")?,
        )?;

        let mut audiosrc_bins = Vec::new();
        if let Some(ref device_name) = self.speaker_source {
            let pulsesrc_bin = pulsesrc_bin(device_name)?;
            audiosrc_bins.push(pulsesrc_bin);
        }
        if let Some(ref device_name) = self.mic_source {
            let pulsesrc_bin = pulsesrc_bin(device_name)?;
            audiosrc_bins.push(pulsesrc_bin);
        }

        for bin in audiosrc_bins {
            pipeline.add(&bin)?;
            bin.static_pad("src").unwrap().link(
                &encodebin
                    .request_pad_simple("audio_%u")
                    .context("Failed to request audio_%u pad from encodebin")?,
            )?;
        }

        Ok(pipeline)
    }
}

fn create_profile(video_format: VideoFormat) -> gst_pbutils::EncodingContainerProfile {
    // FIXME broken gif and mp4

    if video_format == VideoFormat::Gif {
        let caps = gst::Caps::builder("image/gif").build();
        let video_profile = gst_pbutils::EncodingVideoProfile::builder(&caps)
            .presence(0)
            .build();
        return gst_pbutils::EncodingContainerProfile::builder(&caps)
            .presence(0)
            .add_profile(&video_profile)
            .build();
    }

    // TODO option to force vaapi
    // TODO modify element_properties
    let video_profile = {
        let caps = match video_format {
            VideoFormat::Webm => gst::Caps::builder("video/x-vp8").build(),
            VideoFormat::Mkv => gst::Caps::builder("video/x-vp8").build(),
            VideoFormat::Mp4 => gst::Caps::builder("video/x-h264")
                .field("alignment", "au")
                .field("stream-format", "avc")
                .build(),
            VideoFormat::Gif => unreachable!(),
        };
        gst_pbutils::EncodingVideoProfile::builder(&caps)
            .variable_framerate(false)
            .presence(0)
            .build()
    };

    let audio_profile = {
        let caps = match video_format {
            VideoFormat::Webm => gst::Caps::builder("audio/x-opus").build(),
            VideoFormat::Mkv => gst::Caps::builder("audio/x-opus").build(),
            VideoFormat::Mp4 => gst::Caps::builder("audio/mpeg")
                .field("mpegversion", 1)
                .field("layer", 3)
                .build(),
            VideoFormat::Gif => unreachable!(),
        };
        gst_pbutils::EncodingAudioProfile::builder(&caps)
            .presence(0)
            .build()
    };

    let container_profile = {
        let caps = match video_format {
            VideoFormat::Webm => gst::Caps::builder("video/webm").build(),
            VideoFormat::Mkv => gst::Caps::builder("video/x-matroska").build(),
            VideoFormat::Mp4 => gst::Caps::builder("video/quicktime")
                .field("variant", "iso")
                .build(),
            VideoFormat::Gif => unreachable!(),
        };
        gst_pbutils::EncodingContainerProfile::builder(&caps)
            .add_profile(&video_profile)
            .add_profile(&audio_profile)
            .build()
    };

    tracing::debug!(suggested_file_extension = ?container_profile.file_extension());

    container_profile
}

fn element_factory_make(element_name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(element_name, None)
        .with_context(|| format!("Failed to make element `{}`", element_name))
}

fn pipewiresrc_with_default(fd: RawFd, path: &str) -> Result<gst::Element> {
    let src = element_factory_make("pipewiresrc")?;
    src.set_property("fd", &fd);
    src.set_property("path", path);
    src.set_property("do-timestamp", true);
    src.set_property("keepalive-time", 1000);
    src.set_property("resend-last", true);
    Ok(src)
}

fn videoconvert_with_default() -> Result<gst::Element> {
    let conv = element_factory_make("videoconvert")?;
    conv.set_property("chroma-mode", gst_video::VideoChromaMode::None);
    conv.set_property("dither", gst_video::VideoDitherMethod::None);
    conv.set_property("matrix-mode", gst_video::VideoMatrixMode::OutputOnly);
    conv.set_property("n-threads", ideal_thread_count());
    Ok(conv)
}

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

    // x264enc requires even resolution.
    let crop = element_factory_make("videocrop")?;
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
fn multi_stream_pipewiresrc_bin(fd: i32, streams: &[Stream], framerate: u32) -> Result<gst::Bin> {
    let bin = gst::Bin::new(None);

    let compositor = element_factory_make("compositor")?;
    let videoconvert = videoconvert_with_default()?;
    let queue = element_factory_make("queue")?;

    bin.add_many(&[&compositor, &videoconvert, &queue])?;
    gst::Element::link_many(&[&compositor, &videoconvert, &queue])?;

    let videorate_filter = gst::Caps::builder("video/x-raw")
        .field("framerate", gst::Fraction::new(framerate as i32, 1))
        .build();

    let mut last_pos = 0;
    for stream in streams {
        // TODO maybe put another videoconvert here

        let pipewiresrc = pipewiresrc_with_default(fd, &stream.node_id().to_string())?;
        let videorate = element_factory_make("videorate")?;
        let videorate_capsfilter = element_factory_make("capsfilter")?;
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
    let videorate = element_factory_make("videorate")?;
    let queue = element_factory_make("queue")?;

    bin.add_many(&[&pipewiresrc, &videoconvert, &videorate, &queue])?;
    gst::Element::link_many(&[&pipewiresrc, &videoconvert, &videorate])?;

    let videorate_filter = gst::Caps::builder("video/x-raw")
        .field("framerate", gst::Fraction::new(framerate as i32, 1))
        .build();

    if let Some(context) = select_area_context {
        let (stream_width, stream_height) = stream.size().context("Stream has no size")?;

        let videoscale = element_factory_make("videoscale")?;
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

    let pulsesrc = element_factory_make("pulsesrc")?;
    pulsesrc.set_property("device", device_name);
    let audioconvert = element_factory_make("audioconvert")?;
    let queue = element_factory_make("queue")?;

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

fn ideal_thread_count() -> u32 {
    let num_processors = glib::num_processors();
    cmp::min(num_processors, MAX_THREAD_COUNT)
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
