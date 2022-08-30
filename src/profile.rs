use anyhow::{anyhow, ensure, Result};
use gst_pbutils::prelude::*;
use gtk::{glib, subclass::prelude::*};

use std::cell::RefCell;

use crate::element_factory_profile::{ElementFactoryProfile, EncodingProfileExtManual};

mod imp {
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct Profile {
        pub(super) name: RefCell<String>,
        pub(super) muxer_profile: RefCell<Option<ElementFactoryProfile>>,
        pub(super) video_encoder_profile: RefCell<Option<ElementFactoryProfile>>,
        pub(super) audio_encoder_profile: RefCell<Option<ElementFactoryProfile>>,

        pub(super) muxer_factory: RefCell<Option<gst::ElementFactory>>,
        pub(super) video_encoder_factory: RefCell<Option<gst::ElementFactory>>,
        pub(super) audio_encoder_factory: RefCell<Option<gst::ElementFactory>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Profile {
        const NAME: &'static str = "KoohaProfile";
        type Type = super::Profile;
    }

    impl ObjectImpl for Profile {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecString::builder("name")
                        .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                        .build(),
                    glib::ParamSpecBoxed::builder(
                        "muxer-profile",
                        ElementFactoryProfile::static_type(),
                    )
                    .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                    .build(),
                    glib::ParamSpecBoxed::builder(
                        "video-encoder-profile",
                        ElementFactoryProfile::static_type(),
                    )
                    .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                    .build(),
                    glib::ParamSpecBoxed::builder(
                        "audio-encoder-profile",
                        ElementFactoryProfile::static_type(),
                    )
                    .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                    .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "name" => {
                    let name = value.get().unwrap();
                    obj.set_name(name);
                }
                "muxer-profile" => {
                    let muxer_profile = value.get().unwrap();
                    obj.set_muxer_profile(muxer_profile);
                }
                "video-encoder-profile" => {
                    let video_encoder_profile = value.get().unwrap();
                    obj.set_video_encoder_profile(video_encoder_profile);
                }
                "audio-encoder-profile" => {
                    let audio_encoder_profile = value.get().unwrap();
                    obj.set_audio_encoder_profile(audio_encoder_profile);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "name" => obj.name().to_value(),
                "muxer-profile" => obj.muxer_profile().to_value(),
                "video-encoder-profile" => obj.video_encoder_profile().to_value(),
                "audio-encoder-profile" => obj.audio_encoder_profile().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
     pub struct Profile(ObjectSubclass<imp::Profile>);
}

impl Profile {
    pub fn new(
        name: &str,
        muxer_profile: &ElementFactoryProfile,
        video_encoder_profile: &ElementFactoryProfile,
        audio_encoder_profile: &ElementFactoryProfile,
    ) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("muxer-profile", muxer_profile)
            .property("video-encoder-profile", video_encoder_profile)
            .property("audio-encoder-profile", audio_encoder_profile)
            .build()
            .expect("Failed to create Profile.")
    }

    pub fn new_empty(name: &str) -> Self {
        glib::Object::builder()
            .property("name", name)
            .build()
            .expect("Failed to create Profile.")
    }

    pub fn set_name(&self, name: &str) {
        if name == self.name() {
            return;
        }

        self.imp().name.replace(name.to_string());
        self.notify("name");
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    pub fn set_muxer_profile(&self, profile: ElementFactoryProfile) {
        if Some(&profile) == self.muxer_profile().as_ref() {
            return;
        }

        let imp = self.imp();
        imp.muxer_profile.replace(Some(profile));
        imp.muxer_factory.replace(None);
        self.notify("muxer-profile");
    }

    pub fn muxer_profile(&self) -> Option<ElementFactoryProfile> {
        self.imp().muxer_profile.borrow().clone()
    }

    pub fn set_video_encoder_profile(&self, profile: ElementFactoryProfile) {
        if Some(&profile) == self.video_encoder_profile().as_ref() {
            return;
        }

        let imp = self.imp();
        imp.video_encoder_profile.replace(Some(profile));
        imp.video_encoder_factory.replace(None);
        self.notify("video-encoder-profile");
    }

    pub fn video_encoder_profile(&self) -> Option<ElementFactoryProfile> {
        self.imp().video_encoder_profile.borrow().clone()
    }

    pub fn set_audio_encoder_profile(&self, profile: ElementFactoryProfile) {
        if Some(&profile) == self.audio_encoder_profile().as_ref() {
            return;
        }

        let imp = self.imp();
        imp.audio_encoder_profile.replace(Some(profile));
        imp.audio_encoder_factory.replace(None);
        self.notify("audio-encoder-profile");
    }

    pub fn audio_encoder_profile(&self) -> Option<ElementFactoryProfile> {
        self.imp().audio_encoder_profile.borrow().clone()
    }

    pub fn muxer_factory(&self) -> Result<gst::ElementFactory> {
        if let Some(ref factory) = *self.imp().muxer_factory.borrow() {
            return Ok(factory.clone());
        }

        let factory = find_element_factory(
            self.muxer_profile()
                .ok_or_else(|| anyhow!("Profile `{}` has no muxer profile", self.name()))?
                .factory_name(),
        )?;
        self.imp().muxer_factory.replace(Some(factory.clone()));
        Ok(factory)
    }

    pub fn video_encoder_factory(&self) -> Result<gst::ElementFactory> {
        if let Some(ref factory) = *self.imp().video_encoder_factory.borrow() {
            return Ok(factory.clone());
        }

        let factory = find_element_factory(
            self.video_encoder_profile()
                .ok_or_else(|| anyhow!("Profile `{}` has no video encoder profile", self.name()))?
                .factory_name(),
        )?;
        self.imp()
            .video_encoder_factory
            .replace(Some(factory.clone()));
        Ok(factory)
    }

    pub fn audio_encoder_factory(&self) -> Result<gst::ElementFactory> {
        if let Some(ref factory) = *self.imp().audio_encoder_factory.borrow() {
            return Ok(factory.clone());
        }

        let factory = find_element_factory(
            self.audio_encoder_profile()
                .ok_or_else(|| anyhow!("Profile `{}` has no audio encoder profile", self.name()))?
                .factory_name(),
        )?;
        self.imp()
            .audio_encoder_factory
            .replace(Some(factory.clone()));
        Ok(factory)
    }

    pub fn to_encoding_profile(&self) -> Result<gst_pbutils::EncodingContainerProfile> {
        let muxer_factory = self.muxer_factory()?;
        let container_format_caps = profile_format_from_factory(&muxer_factory)?;

        // Video Encoder
        let video_encoder_factory = self.video_encoder_factory()?;
        let video_format_caps = profile_format_from_factory(&video_encoder_factory)?;
        ensure!(
            muxer_factory.can_sink_any_caps(&video_format_caps),
            "`{}` src is incompatible on `{}` sink",
            video_encoder_factory.name(),
            muxer_factory.name()
        );
        let video_profile = gst_pbutils::EncodingVideoProfile::builder(&video_format_caps)
            .preset_name(&video_encoder_factory.name())
            .presence(0)
            .build();
        video_profile.set_element_properties(
            self.video_encoder_profile()
                .ok_or_else(|| anyhow!("Profile `{}` has no video encoder profile", self.name()))?
                .into_element_properties(),
        );

        // Audio Encoder
        let audio_encoder_factory = self.audio_encoder_factory()?;
        let audio_format_caps = profile_format_from_factory(&audio_encoder_factory)?;
        ensure!(
            muxer_factory.can_sink_any_caps(&audio_format_caps),
            "`{}` src is incompatible on `{}` sink",
            audio_encoder_factory.name(),
            muxer_factory.name()
        );
        let audio_profile = gst_pbutils::EncodingAudioProfile::builder(&audio_format_caps)
            .preset_name(&audio_encoder_factory.name())
            .presence(0)
            .build();
        audio_profile.set_element_properties(
            self.audio_encoder_profile()
                .ok_or_else(|| anyhow!("Profile `{}` has no audio encoder profile", self.name()))?
                .into_element_properties(),
        );

        // Muxer
        let container_profile =
            gst_pbutils::EncodingContainerProfile::builder(&container_format_caps)
                .add_profile(&video_profile)
                .add_profile(&audio_profile)
                .presence(0)
                .build();
        container_profile.set_element_properties(
            self.muxer_profile()
                .ok_or_else(|| anyhow!("Profile `{}` has no muxer profile", self.name()))?
                .into_element_properties(),
        );

        Ok(container_profile)
    }

    pub fn deep_clone(&self) -> Self {
        glib::Object::with_values(
            Self::static_type(),
            &self
                .list_properties()
                .iter()
                .map(|pspec| {
                    let property_name = pspec.name();
                    (property_name, self.property_value(property_name))
                })
                .collect::<Vec<_>>(),
        )
        .expect("Failed to create Profile.")
        .downcast()
        .unwrap()
    }
}

fn find_element_factory(factory_name: &str) -> Result<gst::ElementFactory> {
    gst::ElementFactory::find(factory_name)
        .ok_or_else(|| anyhow!("`{}` factory not found", factory_name))
}

fn profile_format_from_factory(factory: &gst::ElementFactory) -> Result<gst::Caps> {
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
                let mut caps = gst::Caps::new_empty();
                caps.get_mut()
                    .unwrap()
                    .append_structure(structure.to_owned());
                return Ok(caps);
            }
        }
    }

    Err(anyhow!(
        "Failed to find profile format for factory `{}`",
        factory_name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_simple_profile(
        muxer_factory_name: &str,
        video_encoder_factory_name: &str,
        audio_encoder_factory_name: &str,
    ) -> Profile {
        Profile::new(
            "",
            &ElementFactoryProfile::new(muxer_factory_name),
            &ElementFactoryProfile::new(video_encoder_factory_name),
            &ElementFactoryProfile::new(audio_encoder_factory_name),
        )
    }

    #[test]
    fn incompatibles() {
        let a = new_simple_profile("webmmux", "x264enc", "opusenc");
        assert_eq!(
            a.to_encoding_profile().unwrap_err().to_string(),
            "`x264enc` src is incompatible on `webmmux` sink"
        );

        let b = new_simple_profile("webmmux", "vp8enc", "lamemp3enc");
        assert_eq!(
            b.to_encoding_profile().unwrap_err().to_string(),
            "`lamemp3enc` src is incompatible on `webmmux` sink"
        );
    }

    #[test]
    fn test_profile_format_from_factory_name() {
        assert!(
            profile_format_from_factory(&find_element_factory("vp8enc").unwrap())
                .unwrap()
                .can_intersect(&gst::Caps::builder("video/x-vp8").build()),
        );
        assert!(
            profile_format_from_factory(&find_element_factory("opusenc").unwrap())
                .unwrap()
                .can_intersect(&gst::Caps::builder("audio/x-opus").build())
        );
        assert!(
            profile_format_from_factory(&find_element_factory("matroskamux").unwrap())
                .unwrap()
                .can_intersect(&gst::Caps::builder("video/x-matroska").build()),
        );
        assert!(
            !profile_format_from_factory(&find_element_factory("matroskamux").unwrap())
                .unwrap()
                .can_intersect(&gst::Caps::builder("video/x-vp8").build()),
        );
        assert_eq!(
            profile_format_from_factory(&find_element_factory("audioconvert").unwrap())
                .unwrap_err()
                .to_string(),
            "Factory `audioconvert` must be an encoder or muxer to be used in a profile"
        );
    }
}
