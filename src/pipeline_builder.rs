use anyhow::{bail, Context, Ok, Result};
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
const GIF_FRAMERATE_OVERRIDE: u32 = 15;

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
        let queue = element_factory_make("queue")?;
        let filesink = element_factory_make_named("filesink", Some("filesink"))?;
        filesink.set_property(
            "location",
            self.file_path
                .to_str()
                .context("Could not convert file path to string")?,
        );

        pipeline.add_many(&[&encodebin, &queue, &filesink])?;
        gst::Element::link_many(&[&encodebin, &queue, &filesink])?;

        let framerate = match self.format {
            VideoFormat::Gif => GIF_FRAMERATE_OVERRIDE,
            _ => self.framerate,
        };

        tracing::debug!(
            file_path = ?self.file_path,
            format = ?self.format,
            framerate,
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

        [&self.speaker_source, &self.mic_source]
            .iter()
            .filter_map(|d| d.as_ref()) // Filter out None
            .try_for_each(|device_name| {
                let pulsesrc_bin = pulsesrc_bin(device_name)?;
                pipeline.add(&pulsesrc_bin)?;
                pulsesrc_bin.static_pad("src").unwrap().link(
                    &encodebin
                        .request_pad_simple("audio_%u")
                        .context("Failed to request audio_%u pad from encodebin")?,
                )?;
                Ok(())
            })?;

        if tracing::enabled!(tracing::Level::DEBUG) {
            let encodebin_elements = encodebin
                .downcast::<gst::Bin>()
                .unwrap()
                .iterate_recurse()
                .into_iter()
                .map(|element| {
                    let element = element.unwrap();
                    let name = element
                        .factory()
                        .map_or_else(|| element.name(), |f| f.name());
                    if name == "capsfilter" {
                        element.property::<gst::Caps>("caps").to_string()
                    } else {
                        name.to_string()
                    }
                })
                .collect::<Vec<_>>();
            tracing::debug!(?encodebin_elements);
        }

        Ok(pipeline)
    }
}

/// Create an encoding profile based on video format
fn create_profile(video_format: VideoFormat) -> gst_pbutils::EncodingContainerProfile {
    use profile::{Builder as ProfileBuilder, ElementPropertiesBuilder};

    // TODO Option for vaapi

    let thread_count = ideal_thread_count();

    let container_profile = match video_format {
        VideoFormat::Webm => {
            ProfileBuilder::new_simple("video/webm", "video/x-vp8", "audio/x-opus")
                .video_preset("vp8enc")
                .video_element_properties(
                    ElementPropertiesBuilder::new("vp8enc")
                        .field("max-quantizer", 17)
                        .field("cpu-used", 16)
                        .field("cq-level", 13)
                        .field("deadline", 1)
                        .field("static-threshold", 100)
                        .field_from_str("keyframe-mode", "disabled")
                        .field("buffer-size", 20000)
                        .field("threads", thread_count)
                        .build(),
                )
                .build()
        }
        VideoFormat::Mkv => ProfileBuilder::new(
            caps("video/x-matroska"),
            gst::Caps::builder("video/x-h264")
                .field("profile", "baseline")
                .build(),
            caps("audio/x-opus"),
        )
        .video_preset("x264enc")
        .video_element_properties(
            ElementPropertiesBuilder::new("x264enc")
                .field("qp-max", 17)
                .field_from_str("speed-preset", "superfast")
                .field("threads", thread_count)
                .build(),
        )
        .build(),
        VideoFormat::Mp4 => ProfileBuilder::new(
            caps("video/quicktime"),
            gst::Caps::builder("video/x-h264")
                .field("profile", "baseline")
                .build(),
            caps("audio/mpeg"),
        )
        .video_preset("x264enc")
        .video_element_properties(
            ElementPropertiesBuilder::new("x264enc")
                .field("qp-max", 17)
                .field_from_str("speed-preset", "superfast")
                .field("threads", thread_count)
                .build(),
        )
        .build(),
        VideoFormat::Gif => todo!("Unsupported video format"), // FIXME
    };

    tracing::debug!(suggested_file_extension = ?container_profile.file_extension());

    container_profile
}

/// Helper function to create a caps with just a name.
fn caps(name: &str) -> gst::Caps {
    gst::Caps::new_simple(name, &[])
}

/// Helper function for more helpful error messages when failing
/// to make an element.
fn element_factory_make(factory_name: &str) -> Result<gst::Element> {
    element_factory_make_named(factory_name, None)
}

/// Helper function for more helpful error messages when failing
/// to make an element.
fn element_factory_make_named(
    factory_name: &str,
    element_name: Option<&str>,
) -> Result<gst::Element> {
    gst::ElementFactory::make(factory_name, element_name)
        .with_context(|| format!("Failed to make element `{}`", factory_name))
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
    cmp::min(glib::num_processors(), MAX_THREAD_COUNT)
}

mod profile {
    use anyhow::{anyhow, Result};
    use gst_pbutils::prelude::*;
    use gtk::glib::{
        self,
        translate::{ToGlibPtr, UnsafeFrom},
    };

    use super::{caps, element_factory_make};

    pub struct ElementPropertiesBuilder {
        structure: gst::Structure,
    }

    impl ElementPropertiesBuilder {
        pub fn new(element_name: &str) -> Self {
            Self {
                structure: gst::Structure::new_empty(element_name),
            }
        }

        pub fn field<V: ToSendValue + Sync>(mut self, name: &str, value: V) -> Self {
            self.structure.set(name, value);
            self
        }

        /// Parse the value into the type of the element's property.
        ///
        /// The element is based on the given name on `Self::new` and
        /// the element's property is based on the recently given name.
        pub fn field_from_str(self, name: &str, string: &str) -> Self {
            self.try_field_from_str(name, string).unwrap()
        }

        pub fn try_field_from_str(mut self, name: &str, string: &str) -> Result<Self> {
            let element = element_factory_make(self.structure.name())?;
            let pspec = element.find_property(name).ok_or_else(|| {
                anyhow!(
                    "Property `{}` not found on type `{}`",
                    name,
                    element.type_()
                )
            })?;
            let value = unsafe {
                glib::SendValue::unsafe_from(
                    glib::Value::deserialize_with_pspec(string, &pspec)?.into_raw(),
                )
            };

            self.structure.set_value(name, value);
            Ok(self)
        }

        pub fn build(self) -> gst::Structure {
            self.structure
        }
    }

    pub struct Builder {
        container_caps: gst::Caps,
        container_preset_name: Option<String>,
        container_element_properties: Vec<gst::Structure>,

        video_caps: gst::Caps,
        video_preset_name: Option<String>,
        video_element_properties: Vec<gst::Structure>,

        audio_caps: gst::Caps,
        audio_preset_name: Option<String>,
        audio_element_properties: Vec<gst::Structure>,
    }

    #[allow(dead_code)]
    impl Builder {
        pub fn new(
            container_caps: gst::Caps,
            video_caps: gst::Caps,
            audio_caps: gst::Caps,
        ) -> Self {
            Self {
                container_caps,
                container_preset_name: None,
                container_element_properties: Vec::new(),
                video_caps,
                video_preset_name: None,
                video_element_properties: Vec::new(),
                audio_caps,
                audio_preset_name: None,
                audio_element_properties: Vec::new(),
            }
        }

        pub fn new_simple(
            container_caps_name: &str,
            video_caps_name: &str,
            audio_caps_name: &str,
        ) -> Self {
            Self::new(
                caps(container_caps_name),
                caps(video_caps_name),
                caps(audio_caps_name),
            )
        }

        pub fn container_preset(mut self, preset_name: &str) -> Self {
            self.container_preset_name = Some(preset_name.to_string());
            self
        }

        pub fn video_preset(mut self, preset_name: &str) -> Self {
            self.video_preset_name = Some(preset_name.to_string());
            self
        }

        pub fn audio_preset(mut self, preset_name: &str) -> Self {
            self.audio_preset_name = Some(preset_name.to_string());
            self
        }

        /// Appends to the container element properties.
        pub fn container_element_properties(mut self, element_properties: gst::Structure) -> Self {
            self.container_element_properties.push(element_properties);
            self
        }

        /// Appends to the video element properties.
        pub fn video_element_properties(mut self, element_properties: gst::Structure) -> Self {
            self.video_element_properties.push(element_properties);
            self
        }

        /// Appends to the audio element properties.
        pub fn audio_element_properties(mut self, element_properties: gst::Structure) -> Self {
            self.audio_element_properties.push(element_properties);
            self
        }

        pub fn build(self) -> gst_pbutils::EncodingContainerProfile {
            let video_profile = {
                let mut builder =
                    gst_pbutils::EncodingVideoProfile::builder(&self.video_caps).presence(0);

                if let Some(ref preset_name) = self.video_preset_name {
                    builder = builder.preset_name(preset_name);
                }

                let profile = builder.build();

                if !self.video_element_properties.is_empty() {
                    profile.set_element_properties(&self.video_element_properties);
                }

                profile
            };

            let audio_profile = {
                let mut builder =
                    gst_pbutils::EncodingAudioProfile::builder(&self.audio_caps).presence(0);

                if let Some(ref preset_name) = self.audio_preset_name {
                    builder = builder.preset_name(preset_name);
                }

                let profile = builder.build();

                if !self.audio_element_properties.is_empty() {
                    profile.set_element_properties(&self.audio_element_properties);
                }

                profile
            };

            let container_profile = {
                let mut builder =
                    gst_pbutils::EncodingContainerProfile::builder(&self.container_caps)
                        .add_profile(&video_profile)
                        .add_profile(&audio_profile)
                        .presence(0);

                if let Some(ref preset_name) = self.container_preset_name {
                    builder = builder.preset_name(preset_name);
                }

                let profile = builder.build();

                if !self.container_element_properties.is_empty() {
                    profile.set_element_properties(&self.container_element_properties);
                }

                profile
            };

            container_profile
        }
    }

    trait EncodingProfileExt {
        fn set_element_properties(&self, element_properties: &[gst::Structure]);
    }

    impl<T: IsA<gst_pbutils::EncodingProfile>> EncodingProfileExt for T {
        fn set_element_properties(&self, element_properties: &[gst::Structure]) {
            let actual_element_properties = gst::Structure::builder("element-properties-map")
                .field(
                    "map",
                    element_properties
                        .iter()
                        .map(|ep| ep.to_send_value())
                        .collect::<gst::List>(),
                )
                .build();

            unsafe {
                gst_pbutils::ffi::gst_encoding_profile_set_element_properties(
                    self.as_ref().to_glib_none().0,
                    actual_element_properties.to_glib_full(),
                );
            }
        }
    }
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
