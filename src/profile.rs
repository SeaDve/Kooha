use anyhow::{bail, Context, Result};
use gst::prelude::*;
use gtk::{
    gio,
    glib::{self, subclass::prelude::*},
};
use once_cell::sync::OnceCell as OnceLock;
use serde::Deserialize;

const DEFAULT_SUGGESTED_MAX_FRAMERATE: gst::Fraction = gst::Fraction::from_integer(60);
const MAX_THREAD_COUNT: u32 = 64;

#[derive(Debug, Deserialize)]
struct Profiles {
    supported: Vec<ProfileData>,
    experimental: Vec<ProfileData>,
}

#[derive(Debug, Deserialize)]
struct ProfileData {
    id: String,
    #[serde(default)]
    is_experimental: bool,
    name: String,
    #[serde(rename = "suggested-max-fps")]
    suggested_max_framerate: Option<f64>,
    #[serde(rename = "extension")]
    file_extension: String,
    #[serde(rename = "videoenc")]
    videoenc_bin_str: String,
    #[serde(rename = "audioenc")]
    audioenc_bin_str: Option<String>,
    #[serde(rename = "muxer")]
    muxer_bin_str: Option<String>,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Profile {
        pub(super) data: OnceLock<ProfileData>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Profile {
        const NAME: &'static str = "KoohaProfile";
        type Type = super::Profile;
    }

    impl ObjectImpl for Profile {}
}

glib::wrapper! {
    pub struct Profile(ObjectSubclass<imp::Profile>);
}

impl Profile {
    fn from_data(data: ProfileData) -> Self {
        let this = glib::Object::new::<Self>();
        this.imp().data.set(data).unwrap();
        this
    }

    fn data(&self) -> &ProfileData {
        self.imp().data.get().unwrap()
    }

    pub fn all() -> Result<&'static [Self]> {
        static ALL: OnceLock<Vec<Profile>> = OnceLock::new();

        ALL.get_or_try_init(|| {
            let bytes = gio::resources_lookup_data(
                "/io/github/seadve/Kooha/profiles.yml",
                gio::ResourceLookupFlags::NONE,
            )?;
            let profiles = serde_yaml::from_slice::<Profiles>(&bytes)?;

            let supported = profiles.supported.into_iter().map(|mut data| {
                data.is_experimental = false;
                Self::from_data(data)
            });
            let experimental = profiles.experimental.into_iter().map(|mut data| {
                data.is_experimental = true;
                Self::from_data(data)
            });
            Ok(supported.chain(experimental).collect())
        })
        .map(|v| v.as_slice())
    }

    pub fn from_id(id: &str) -> Result<&'static Self> {
        let profile = Self::all()?
            .iter()
            .find(|p| p.id() == id)
            .with_context(|| format!("Profile `{}` not found", id))?;
        Ok(profile)
    }

    pub fn id(&self) -> &str {
        &self.data().id
    }

    pub fn name(&self) -> &str {
        &self.data().name
    }

    pub fn file_extension(&self) -> &str {
        &self.data().file_extension
    }

    pub fn supports_audio(&self) -> bool {
        self.data().audioenc_bin_str.is_some()
    }

    pub fn suggested_max_framerate(&self) -> gst::Fraction {
        self.data().suggested_max_framerate.map_or_else(
            || DEFAULT_SUGGESTED_MAX_FRAMERATE,
            |raw| gst::Fraction::approximate_f64(raw).unwrap(),
        )
    }

    pub fn is_experimental(&self) -> bool {
        self.data().is_experimental
    }

    pub fn is_available(&self) -> bool {
        self.is_available_inner()
            .inspect_err(|err| {
                tracing::debug!("Profile `{}` is not available: {:?}", self.id(), err);
            })
            .is_ok()
    }

    fn is_available_inner(&self) -> Result<()> {
        parse_bin_test(&self.data().videoenc_bin_str).context("Failed to parse videoenc bin")?;

        if let Some(audioenc_bin_str) = &self.data().audioenc_bin_str {
            parse_bin_test(audioenc_bin_str).context("Failed to parse audioenc bin")?;
        }

        if let Some(muxer_bin_str) = &self.data().muxer_bin_str {
            parse_bin_test(muxer_bin_str).context("Failed to parse muxer bin")?;
        }

        Ok(())
    }

    pub fn attach(
        &self,
        pipeline: &gst::Pipeline,
        video_src: &gst::Element,
        audio_srcs: Option<&gst::Element>,
        sink: &gst::Element,
    ) -> Result<()> {
        let videoenc_bin = parse_bin("kooha-videoenc-bin", &self.data().videoenc_bin_str)?;
        debug_assert!(videoenc_bin.iterate_elements().into_iter().any(|element| {
            let factory = element.unwrap().factory().unwrap();
            factory.has_type(gst::ElementFactoryType::VIDEO_ENCODER)
        }));

        pipeline.add(&videoenc_bin)?;
        video_src.link(&videoenc_bin)?;

        match (&self.data().audioenc_bin_str, &self.data().muxer_bin_str) {
            (None, None) => {
                // Special case for gifenc

                if audio_srcs.is_some() {
                    tracing::error!("Audio srcs ignored: Profile does not support audio");
                }

                videoenc_bin.link(sink)?;
            }
            (audioenc_str, Some(muxer_bin_str)) => {
                let muxer_bin = parse_bin("kooha-muxer-bin", muxer_bin_str)?;
                let muxer = muxer_bin
                    .iterate_elements()
                    .find(|element| {
                        element
                            .factory()
                            .is_some_and(|f| f.has_type(gst::ElementFactoryType::MUXER))
                    })
                    .context("Can't find the muxer in muxer bin")?;

                pipeline.add(&muxer_bin)?;
                videoenc_bin.link_pads(None, &muxer, Some("video_%u"))?;
                muxer_bin.link(sink)?;

                if let Some(audio_srcs) = audio_srcs {
                    let audioenc_str = audioenc_str
                        .as_ref()
                        .context("Failed to handle audio srcs: Profile has no audio encoder")?;
                    let audioenc_bin = parse_bin("kooha-audioenc-bin", audioenc_str)?;
                    debug_assert!(audioenc_bin.iterate_elements().into_iter().any(|element| {
                        let factory = element.unwrap().factory().unwrap();
                        factory.has_type(gst::ElementFactoryType::AUDIO_ENCODER)
                    }));

                    pipeline.add(&audioenc_bin)?;
                    audio_srcs.link(&audioenc_bin)?;
                    audioenc_bin.link_pads(None, &muxer, Some("audio_%u"))?;
                }
            }
            (Some(_), None) => {
                bail!("Unexpected audioenc without muxer")
            }
        }

        Ok(())
    }
}

fn parse_bin_test(description: &str) -> Result<(), glib::Error> {
    // Empty names are ignored in implementation details of `gst::parse::bin_from_description_with_name_full`
    parse_bin_inner("", description, false)?;

    Ok(())
}

fn parse_bin(name: &str, description: &str) -> Result<gst::Bin, glib::Error> {
    parse_bin_inner(name, description, true)
}

fn parse_bin_inner(
    name: &str,
    description: &str,
    add_ghost_pads: bool,
) -> Result<gst::Bin, glib::Error> {
    let ideal_n_threads = glib::num_processors().min(MAX_THREAD_COUNT);
    let formatted_description = description.replace("${N_THREADS}", &ideal_n_threads.to_string());
    let bin = gst::parse::bin_from_description_with_name_full(
        &formatted_description,
        add_ghost_pads,
        name,
        None,
        gst::ParseFlags::FATAL_ERRORS,
    )?
    .downcast()
    .unwrap();
    Ok(bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{collections::HashSet, sync::Once};

    use crate::config::RESOURCES_FILE;

    fn init_gresources() {
        static INIT: Once = Once::new();

        INIT.call_once(|| {
            let res = gio::Resource::load(RESOURCES_FILE).unwrap();
            gio::resources_register(&res);
        });
    }

    #[test]
    fn profiles_fields_validity() {
        init_gresources();

        let mut unique = HashSet::new();

        for profile in Profile::all().unwrap() {
            assert!(!profile.id().is_empty());

            assert!(!profile.name().is_empty());
            assert!(!profile.file_extension().is_empty());
            assert_ne!(
                profile.suggested_max_framerate(),
                gst::Fraction::from_integer(0)
            );

            assert!(
                unique.insert(profile.id().to_string()),
                "Duplicate id `{}`",
                profile.id()
            );
        }
    }

    #[test]
    fn profiles_validity() {
        init_gresources();
        gst::init().unwrap();
        gstgif::plugin_register_static().unwrap();

        for profile in Profile::all().unwrap() {
            // These profiles are not supported by the CI runner.
            if matches!(profile.id(), "vaapi-vp8" | "vaapi-vp9" | "va-h264") {
                continue;
            }

            // FIXME Remove this. This is needed as x264enc is somehow not found.
            if matches!(profile.id(), "mp4" | "matroska-h264") {
                continue;
            }

            let pipeline = gst::Pipeline::new();

            let dummy_video_src = gst::ElementFactory::make("fakesrc").build().unwrap();
            let dummy_sink = gst::ElementFactory::make("fakesink").build().unwrap();
            pipeline.add_many([&dummy_video_src, &dummy_sink]).unwrap();

            let dummy_audio_src = if profile.supports_audio() {
                let dummy_audio_src = gst::ElementFactory::make("fakesrc").build().unwrap();
                pipeline.add(&dummy_audio_src).unwrap();
                Some(dummy_audio_src)
            } else {
                None
            };

            if let Err(err) = profile.attach(
                &pipeline,
                &dummy_video_src,
                dummy_audio_src.as_ref(),
                &dummy_sink,
            ) {
                panic!("can't attach profile `{}`: {:?}", profile.id(), err);
            }

            assert!(pipeline
                .find_unlinked_pad(gst::PadDirection::Sink)
                .is_none());
            assert!(pipeline.find_unlinked_pad(gst::PadDirection::Src).is_none());

            assert!(profile.is_available());
        }
    }
}
