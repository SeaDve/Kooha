use anyhow::{anyhow, ensure, Context, Result};
use gettextrs::gettext;
use gst::prelude::*;
use gst_pbutils::prelude::*;
use gtk::glib;

use std::fmt;

use crate::{
    element_properties::{ElementProperties, EncodingProfileExtManual},
    utils,
};

/// Returns all profiles including experimental ones.
pub fn all() -> Vec<Box<dyn Profile>> {
    builtins().into_iter().chain(experimental::all()).collect()
}

/// Returns all builtin profiles.
pub fn builtins() -> Vec<Box<dyn Profile>> {
    vec![
        Box::new(WebMProfile),
        Box::new(Mp4Profile),
        Box::new(MatroskaProfile),
        Box::new(GifProfile),
    ]
}

/// Returns `None` if the profile is not found, or a `bool`
/// whether the profile is experimental.
pub fn is_experimental(id: &str) -> Option<bool> {
    if experimental::all().into_iter().any(|p| p.id() == id) {
        return Some(true);
    }

    if get(id).is_some() {
        return Some(false);
    }

    None
}

/// Get profile by ID including experimental ones.
pub fn get(id: &str) -> Option<Box<dyn Profile>> {
    all().into_iter().find(|p| p.id() == id)
}

/// Returns the default profile.
pub fn default() -> Box<dyn Profile> {
    get("webm").unwrap()
}

pub trait Profile: fmt::Debug {
    fn id(&self) -> &str;

    fn name(&self) -> String;

    fn file_extension(&self) -> &str;

    fn framerate_override(&self) -> Option<u32>;

    fn supports_audio(&self) -> bool;

    fn attach(
        &self,
        pipeline: &gst::Pipeline,
        video_src: &gst::Element,
        audio_srcs: &[gst::Element],
        sink: &gst::Element,
    ) -> Result<()>;
}

#[derive(Debug)]
struct GifProfile;

impl Profile for GifProfile {
    fn id(&self) -> &str {
        "gif"
    }

    fn name(&self) -> String {
        gettext("GIF")
    }

    fn file_extension(&self) -> &str {
        "gif"
    }

    fn framerate_override(&self) -> Option<u32> {
        Some(15)
    }

    fn supports_audio(&self) -> bool {
        false
    }

    fn attach(
        &self,
        pipeline: &gst::Pipeline,
        video_src: &gst::Element,
        audio_srcs: &[gst::Element],
        sink: &gst::Element,
    ) -> Result<()> {
        if !audio_srcs.is_empty() {
            tracing::error!("Audio is not supported for Gif profile");
        }

        let gifenc = element_factory_make("gifenc")?;
        gifenc.set_property("repeat", -1);
        gifenc.set_property("speed", 30);

        pipeline.add(&gifenc)?;

        video_src
            .link(&gifenc)
            .context("Failed to link video src to gifenc")?;

        gifenc.link(sink).context("Failed to link gifenc to sink")?;

        Ok(())
    }
}

macro_rules! encodebin_profile {
    ($id:literal, $struct_name:ident, $name:expr, $file_extension:literal, $profile:expr) => {
        #[derive(Debug)]
        struct $struct_name;

        impl Profile for $struct_name {
            fn id(&self) -> &str {
                $id
            }

            fn name(&self) -> String {
                $name
            }

            fn file_extension(&self) -> &str {
                $file_extension
            }

            fn framerate_override(&self) -> Option<u32> {
                None
            }

            fn supports_audio(&self) -> bool {
                true
            }

            fn attach(
                &self,
                pipeline: &gst::Pipeline,
                video_src: &gst::Element,
                audio_srcs: &[gst::Element],
                sink: &gst::Element,
            ) -> Result<()> {
                let encodebin = element_factory_make("encodebin")?;
                encodebin.set_property("profile", $profile);

                pipeline.add(&encodebin)?;

                video_src.static_pad("src").unwrap().link(
                    &encodebin
                        .request_pad_simple("video_%u")
                        .context("Failed to request video_%u pad from encodebin")?,
                )?;

                for src in audio_srcs {
                    src.static_pad("src").unwrap().link(
                        &encodebin
                            .request_pad_simple("audio_%u")
                            .context("Failed to request audio_%u pad from encodebin")?,
                    )?;
                }

                encodebin
                    .link(sink)
                    .context("Failed to link encodebin to sink")?;

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

                Ok(())
            }
        }
    };
}

encodebin_profile!(
    "webm",
    WebMProfile,
    gettext("WebM"),
    "webm",
    new_encoding_profile(
        ElementProperties::builder("vp8enc")
            .field("max-quantizer", 17)
            .field("cpu-used", 16)
            .field("cq-level", 13)
            .field("deadline", 1)
            .field("static-threshold", 100)
            .field_from_str("keyframe-mode", "disabled")
            .field("buffer-size", 20000)
            .field("threads", utils::ideal_thread_count())
            .build(),
        Vec::new(),
        ElementProperties::builder("opusenc").build(),
        Vec::new(),
        ElementProperties::builder("webmmux").build(),
        Vec::new()
    )?
);

encodebin_profile!(
    "mp4",
    Mp4Profile,
    gettext("MP4"),
    "mp4",
    new_encoding_profile(
        ElementProperties::builder("x264enc")
            .field("qp-max", 17)
            .field_from_str("speed-preset", "superfast")
            .field("threads", utils::ideal_thread_count())
            .build(),
        vec![("profile", "baseline".to_send_value())],
        ElementProperties::builder("lamemp3enc").build(),
        Vec::new(),
        ElementProperties::builder("mp4mux").build(),
        Vec::new()
    )?
);

encodebin_profile!(
    "matroska",
    MatroskaProfile,
    gettext("Matroska"),
    "mkv",
    new_encoding_profile(
        ElementProperties::builder("x264enc")
            .field("qp-max", 17)
            .field_from_str("speed-preset", "superfast")
            .field("threads", utils::ideal_thread_count())
            .build(),
        vec![("profile", "baseline".to_send_value())],
        ElementProperties::builder("opusenc").build(),
        Vec::new(),
        ElementProperties::builder("matroskamux").build(),
        Vec::new()
    )?
);

mod experimental {
    use super::*;

    /// Get all experimental profiles
    pub fn all() -> Vec<Box<dyn Profile>> {
        vec![
            Box::new(WebMVp9Profile),
            Box::new(WebMAv1Profile),
            Box::new(VaapiVp8Profile),
            Box::new(VaapiVp9Profile),
            Box::new(VaapiH264Profile),
        ]
    }

    encodebin_profile!(
        "webm-vp9",
        WebMVp9Profile,
        gettext("WebM VP9"),
        "webm",
        new_encoding_profile(
            ElementProperties::builder("vp9enc")
                .field("max-quantizer", 17)
                .field("cpu-used", 16)
                .field("cq-level", 13)
                .field("deadline", 1)
                .field("static-threshold", 100)
                .field_from_str("keyframe-mode", "disabled")
                .field("buffer-size", 20000)
                .field("threads", utils::ideal_thread_count())
                .build(),
            Vec::new(),
            ElementProperties::builder("opusenc").build(),
            Vec::new(),
            ElementProperties::builder("webmmux").build(),
            Vec::new()
        )?
    );

    encodebin_profile!(
        "webm-av1",
        WebMAv1Profile,
        gettext("WebM AV1"),
        "webm",
        new_encoding_profile(
            ElementProperties::builder("av1enc")
                .field("max-quantizer", 17)
                .field("cpu-used", 5)
                .field_from_str("end-usage", "cq")
                .field("buf-sz", 20000)
                .field("threads", utils::ideal_thread_count())
                .build(),
            Vec::new(),
            ElementProperties::builder("opusenc").build(),
            Vec::new(),
            ElementProperties::builder("webmmux").build(),
            Vec::new()
        )?
    );

    encodebin_profile!(
        "vaapi-vp8",
        VaapiVp8Profile,
        gettext("WebM VAAPI VP8"),
        "mkv",
        new_encoding_profile(
            ElementProperties::builder("vaapivp8enc").build(),
            Vec::new(),
            ElementProperties::builder("opusenc").build(),
            Vec::new(),
            ElementProperties::builder("webmmux").build(),
            Vec::new()
        )?
    );

    encodebin_profile!(
        "vaapi-vp9",
        VaapiVp9Profile,
        gettext("WebM VAAPI VP9"),
        "mkv",
        new_encoding_profile(
            ElementProperties::builder("vaapivp9enc").build(),
            Vec::new(),
            ElementProperties::builder("opusenc").build(),
            Vec::new(),
            ElementProperties::builder("webmmux").build(),
            Vec::new()
        )?
    );

    encodebin_profile!(
        "vaapi-h264",
        VaapiH264Profile,
        gettext("WebM VAAPI H264"),
        "mkv",
        new_encoding_profile(
            ElementProperties::builder("vaapih264enc").build(),
            Vec::new(),
            ElementProperties::builder("lamemp3enc").build(),
            Vec::new(),
            ElementProperties::builder("mp4mux").build(),
            Vec::new()
        )?
    );
}

fn element_factory_make(factory_name: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(factory_name, None)
        .with_context(|| format!("Failed to make element `{}`", factory_name))
}

fn find_element_factory(factory_name: &str) -> Result<gst::ElementFactory> {
    gst::ElementFactory::find(factory_name)
        .ok_or_else(|| anyhow!("`{}` factory not found", factory_name))
}

fn profile_format_from_factory(
    factory: &gst::ElementFactory,
    values: Vec<(&str, glib::SendValue)>,
) -> Result<gst::Caps> {
    let factory_name = factory.name();

    ensure!(
        factory.has_type(gst::ElementFactoryType::ENCODER | gst::ElementFactoryType::MUXER),
        "Factory `{}` must be an encoder or muxer to be used in a profile",
        factory_name
    );

    for template in factory.static_pad_templates() {
        if template.direction() == gst::PadDirection::Src {
            let template_caps = template.caps();
            if let Some(structure) = template_caps.structure(0) {
                let mut structure = structure.to_owned();

                for (f, v) in values {
                    structure.set_value(f, v);
                }

                let mut caps = gst::Caps::new_empty();
                caps.get_mut().unwrap().append_structure(structure);
                return Ok(caps);
            }
        }
    }

    Err(anyhow!(
        "Failed to find profile format for factory `{}`",
        factory_name
    ))
}

fn new_encoding_profile(
    video_encoder_element_properties: ElementProperties,
    video_encoder_caps_fields: Vec<(&str, glib::SendValue)>,
    audio_encoder_element_properties: ElementProperties,
    audio_encoder_caps_fields: Vec<(&str, glib::SendValue)>,
    muxer_element_properties: ElementProperties,
    muxer_caps_fields: Vec<(&str, glib::SendValue)>,
) -> Result<gst_pbutils::EncodingContainerProfile> {
    let muxer_factory_name = muxer_element_properties.factory_name();
    let muxer_factory = find_element_factory(muxer_factory_name)?;
    ensure!(
        muxer_factory.has_type(gst::ElementFactoryType::MUXER),
        "`{}` is not a muxer",
        muxer_factory_name
    );

    let video_encoder_factory_name = video_encoder_element_properties.factory_name();
    let video_encoder_factory = find_element_factory(video_encoder_factory_name)?;
    ensure!(
        video_encoder_factory.has_type(gst::ElementFactoryType::VIDEO_ENCODER),
        "`{}` is not a video encoder",
        video_encoder_factory_name
    );
    let video_encoder_format =
        profile_format_from_factory(&video_encoder_factory, video_encoder_caps_fields)?;
    ensure!(
        muxer_factory.can_sink_any_caps(&video_encoder_format),
        "`{}` src is incompatible on `{}` sink",
        video_encoder_factory_name,
        muxer_factory_name
    );
    let video_profile = gst_pbutils::EncodingVideoProfile::builder(&video_encoder_format)
        .presence(0)
        .build();
    video_profile.set_element_properties(video_encoder_element_properties);

    let audio_encoder_factory_name = audio_encoder_element_properties.factory_name();
    let audio_encoder_factory = find_element_factory(audio_encoder_factory_name)?;
    ensure!(
        audio_encoder_factory.has_type(gst::ElementFactoryType::AUDIO_ENCODER),
        "`{}` is not an audio encoder",
        audio_encoder_factory_name
    );
    let audio_encoder_format =
        profile_format_from_factory(&audio_encoder_factory, audio_encoder_caps_fields)?;
    ensure!(
        muxer_factory.can_sink_any_caps(&audio_encoder_format),
        "`{}` src is incompatible on `{}` sink",
        audio_encoder_factory_name,
        muxer_factory_name
    );
    let audio_profile = gst_pbutils::EncodingAudioProfile::builder(&audio_encoder_format)
        .presence(0)
        .build();
    audio_profile.set_element_properties(audio_encoder_element_properties);

    let muxer_format = profile_format_from_factory(&muxer_factory, muxer_caps_fields)?;
    let container_profile = gst_pbutils::EncodingContainerProfile::builder(&muxer_format)
        .add_profile(&video_profile)
        .add_profile(&audio_profile)
        .presence(0)
        .build();
    container_profile.set_element_properties(muxer_element_properties);

    Ok(container_profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    #[test]
    fn unique_ids() {
        let mut unique = HashSet::new();

        for profile in all() {
            assert!(
                unique.insert(profile.id().to_string()),
                "Duplicate id `{}`",
                profile.id()
            );
        }
    }

    #[test]
    fn builtin_profiles() {
        gst::init().unwrap();
        gstgif::plugin_register_static().unwrap();

        assert!(default().supports_audio());

        for profile in all() {
            let pipeline = gst::Pipeline::new(None);
            let dummy_video_src = gst::ElementFactory::make("fakesrc", None).unwrap();
            let dummy_audio_src = gst::ElementFactory::make("fakesrc", None).unwrap();
            let dummy_sink = gst::ElementFactory::make("fakesink", None).unwrap();
            pipeline
                .add_many(&[&dummy_video_src, &dummy_audio_src, &dummy_sink])
                .unwrap();

            assert!(!profile.name().is_empty());
            assert!(!profile.file_extension().is_empty());

            if let Err(err) =
                profile.attach(&pipeline, &dummy_video_src, &[dummy_audio_src], &dummy_sink)
            {
                panic!("{:?}", err);
            }
        }
    }

    #[test]
    fn is_experimental_test() {
        for profile in builtins() {
            assert!(!is_experimental(profile.id()).unwrap());
        }

        for profile in experimental::all() {
            assert!(is_experimental(profile.id()).unwrap());
        }
    }

    #[test]
    fn incompatibles() {
        fn new_simple_encoding_profile(
            video_encoder_factory_name: &str,
            audio_encoder_factory_name: &str,
            muxer_factory_name: &str,
        ) -> Result<gst_pbutils::EncodingContainerProfile> {
            new_encoding_profile(
                ElementProperties::builder(video_encoder_factory_name).build(),
                Vec::new(),
                ElementProperties::builder(audio_encoder_factory_name).build(),
                Vec::new(),
                ElementProperties::builder(muxer_factory_name).build(),
                Vec::new(),
            )
        }

        let a = new_simple_encoding_profile("x264enc", "opusenc", "webmmux");
        assert_eq!(
            a.unwrap_err().to_string(),
            "`x264enc` src is incompatible on `webmmux` sink"
        );

        let b = new_simple_encoding_profile("vp8enc", "lamemp3enc", "webmmux");
        assert_eq!(
            b.unwrap_err().to_string(),
            "`lamemp3enc` src is incompatible on `webmmux` sink"
        );
    }
}
