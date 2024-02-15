use anyhow::{bail, Context, Ok, Result};
use gst::prelude::*;
use gtk::graphene::Rect;
use num_rational::Rational32;

use std::{
    os::unix::io::RawFd,
    path::{Path, PathBuf},
};

use crate::{area_selector::SelectAreaData, profile::Profile, screencast_session::Stream};

const AUDIO_SAMPLE_RATE: i32 = 48_000;
const AUDIO_N_CHANNELS: i32 = 1;

pub type Framerate = Rational32;

#[derive(Debug)]
#[must_use]
pub struct PipelineBuilder {
    file_path: PathBuf,
    framerate: Framerate,
    profile: Profile,
    fd: RawFd,
    streams: Vec<Stream>,
    speaker_source: Option<String>,
    mic_source: Option<String>,
    select_area_data: Option<SelectAreaData>,
}

impl PipelineBuilder {
    pub fn new(
        file_path: &Path,
        framerate: Framerate,
        profile: Profile,
        fd: RawFd,
        streams: Vec<Stream>,
    ) -> Self {
        Self {
            file_path: file_path.to_path_buf(),
            framerate,
            profile,
            fd,
            streams,
            speaker_source: None,
            mic_source: None,
            select_area_data: None,
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

    pub fn select_area_data(&mut self, data: SelectAreaData) -> &mut Self {
        self.select_area_data = Some(data);
        self
    }

    pub fn build(&self) -> Result<gst::Pipeline> {
        tracing::debug!(
            file_path = %self.file_path.display(),
            framerate = ?self.framerate,
            profile = ?self.profile.id(),
            stream_len = self.streams.len(),
            streams = ?self.streams,
            speaker_source = ?self.speaker_source,
            mic_source = ?self.mic_source,
            select_area_data = ?self.select_area_data,
        );

        let videosrc_bin = make_pipewiresrc_bin(
            self.fd,
            &self.streams,
            self.framerate,
            self.select_area_data.as_ref(),
        )
        .context("Failed to create videosrc bin")?;

        let videoenc_queue = gst::ElementFactory::make("queue").build()?;
        let filesink = gst::ElementFactory::make("filesink")
            .property(
                "location",
                self.file_path
                    .to_str()
                    .context("Could not convert file path to string")?,
            )
            .build()?;

        let pipeline = gst::Pipeline::new();

        pipeline.add_many([videosrc_bin.upcast_ref(), &videoenc_queue, &filesink])?;
        videosrc_bin.link(&videoenc_queue)?;

        let has_audio_source = self.speaker_source.is_some() || self.mic_source.is_some();
        let audioenc_queue = if self.profile.supports_audio() && has_audio_source {
            let audiosrc_bin = make_pulsesrc_bin(
                [&self.speaker_source, &self.mic_source]
                    .into_iter()
                    .filter_map(|s| s.as_deref()),
            )
            .context("Failed to create audiosrc bin")?;
            let audioenc_queue = gst::ElementFactory::make("queue").build()?;

            pipeline.add_many([audiosrc_bin.upcast_ref(), &audioenc_queue])?;
            audiosrc_bin.link(&audioenc_queue)?;

            Some(audioenc_queue)
        } else {
            if has_audio_source {
                tracing::warn!(
                    "Selected profile does not support audio, but audio sources are provided. Ignoring audio sources"
                );
            }

            None
        };

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
        .property("keepalive-time", 1000)
        .property("resend-last", true)
        .build()?;

    Ok(src)
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
///                           (If has select area data)
///                                 |            |
///                                 V            V
/// pipewiresrc -> videorate -> videoscale -> videocrop
///
/// Multiple streams:
///                                                (If has select area data)
/// pipewiresrc1 -> videorate -> |                       |            |
///                              |                       V            V
/// pipewiresrc2 -> videorate -> | -> compositor -> videoscale -> videocrop
///                              |
/// pipewiresrcn -> videorate -> |
pub fn make_pipewiresrc_bin(
    fd: RawFd,
    streams: &[Stream],
    framerate: Framerate,
    select_area_data: Option<&SelectAreaData>,
) -> Result<gst::Bin> {
    let bin = gst::Bin::builder().name("kooha-pipewiresrc-bin").build();

    let videorate_caps = gst::Caps::builder("video/x-raw")
        .field("framerate", gst::Fraction::from(framerate))
        .build();

    let src_element = match streams {
        [] => bail!("No streams provided"),
        [stream] => {
            let pipewiresrc = make_pipewiresrc(fd, &stream.node_id().to_string())?;
            let videorate = gst::ElementFactory::make("videorate")
                .property("skip-to-first", true)
                .build()?;
            let videorate_capsfilter = gst::ElementFactory::make("capsfilter")
                .property("caps", &videorate_caps)
                .build()?;

            bin.add_many([&pipewiresrc, &videorate, &videorate_capsfilter])?;
            gst::Element::link_many([&pipewiresrc, &videorate, &videorate_capsfilter])?;

            videorate_capsfilter
        }
        streams => {
            let compositor = gst::ElementFactory::make("compositor").build()?;
            bin.add(&compositor)?;

            let mut last_pos = 0;
            for stream in streams {
                let pipewiresrc = make_pipewiresrc(fd, &stream.node_id().to_string())?;
                let videorate = gst::ElementFactory::make("videorate")
                    .property("skip-to-first", true)
                    .build()?;
                let videorate_capsfilter = gst::ElementFactory::make("capsfilter")
                    .property("caps", &videorate_caps)
                    .build()?;

                bin.add_many([&pipewiresrc, &videorate, &videorate_capsfilter])?;
                gst::Element::link_many([&pipewiresrc, &videorate, &videorate_capsfilter])?;

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

            compositor
        }
    };

    if let Some(data) = select_area_data {
        let videoscale = gst::ElementFactory::make("videoscale").build()?;
        let videocrop = make_videocrop(data)?;

        // x264enc requires even resolution.
        let (stream_width, stream_height) = data.stream_size;
        let videoscale_caps = gst::Caps::builder("video/x-raw")
            .field("width", round_to_even(stream_width))
            .field("height", round_to_even(stream_height))
            .build();

        bin.add_many([&videoscale, &videocrop])?;
        src_element.link(&videoscale)?;
        videoscale.link_filtered(&videocrop, &videoscale_caps)?;

        let src_pad = videocrop.static_pad("src").unwrap();
        bin.add_pad(
            &gst::GhostPad::builder_with_target(&src_pad)?
                .name("src")
                .build(),
        )?;
    } else {
        let src_pad = src_element.static_pad("src").unwrap();
        bin.add_pad(
            &gst::GhostPad::builder_with_target(&src_pad)?
                .name("src")
                .build(),
        )?;
    }

    Ok(bin)
}

/// Creates a bin with a src pad for a pulse audio device
///
/// pulsesrc1 -> audiorate -> |
///                           |
/// pulsesrc2 -> audiorate -> | -> audiomixer
///                           |
/// pulsesrcn -> audiorate -> |
fn make_pulsesrc_bin<'a>(device_names: impl IntoIterator<Item = &'a str>) -> Result<gst::Bin> {
    let bin = gst::Bin::builder().name("kooha-pulsesrc-bin").build();

    let audiomixer = gst::ElementFactory::make("audiomixer").build()?;
    bin.add(&audiomixer)?;

    let src_pad = audiomixer.static_pad("src").unwrap();
    bin.add_pad(
        &gst::GhostPad::builder_with_target(&src_pad)?
            .name("src")
            .build(),
    )?;

    let pulsesrc_caps = gst::Caps::builder("audio/x-raw")
        .field("rate", AUDIO_SAMPLE_RATE)
        .field("channels", AUDIO_N_CHANNELS)
        .build();
    for device_name in device_names {
        let pulsesrc = gst::ElementFactory::make("pulsesrc")
            .property("device", device_name)
            .property("provide-clock", false)
            .property("do-timestamp", true)
            .build()?;
        let audiorate = gst::ElementFactory::make("audiorate")
            .property("skip-to-first", true)
            .build()?;

        bin.add_many([&pulsesrc, &audiorate])?;
        pulsesrc.link_filtered(&audiorate, &pulsesrc_caps)?;
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
