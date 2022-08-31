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
        pub(super) file_extension: RefCell<Option<String>>,
        pub(super) muxer_profile: RefCell<Option<ElementFactoryProfile>>,
        pub(super) video_encoder_profile: RefCell<Option<ElementFactoryProfile>>,
        pub(super) audio_encoder_profile: RefCell<Option<ElementFactoryProfile>>,
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
                    glib::ParamSpecString::builder("file-extension")
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
                "file-extension" => {
                    let file_extension = value.get().unwrap();
                    obj.set_file_extension(file_extension);
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
                "file-extension" => obj.file_extension().to_value(),
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
    pub fn new(name: &str) -> Self {
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

    pub fn set_file_extension(&self, file_extension: &str) {
        if Some(file_extension) == self.file_extension().as_deref() {
            return;
        }

        self.imp()
            .file_extension
            .replace(Some(file_extension.to_string()));
        self.notify("file-extension");
    }

    pub fn file_extension(&self) -> Option<String> {
        self.imp().file_extension.borrow().clone()
    }

    pub fn set_muxer_profile(&self, profile: ElementFactoryProfile) {
        if Some(&profile) == self.muxer_profile().as_ref() {
            return;
        }

        let imp = self.imp();
        imp.muxer_profile.replace(Some(profile));
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
        self.notify("audio-encoder-profile");
    }

    pub fn audio_encoder_profile(&self) -> Option<ElementFactoryProfile> {
        self.imp().audio_encoder_profile.borrow().clone()
    }

    pub fn to_encoding_profile(&self) -> Result<gst_pbutils::EncodingContainerProfile> {
        let muxer_profile = self
            .muxer_profile()
            .ok_or_else(|| anyhow!("Profile `{}` has no muxer profile", self.name()))?;
        let muxer_factory = muxer_profile.factory()?;

        // Video Encoder
        let video_encoder_profile = self
            .video_encoder_profile()
            .ok_or_else(|| anyhow!("Profile `{}` has no video encoder profile", self.name()))?;
        let video_profile_format = video_encoder_profile.format()?;
        ensure!(
            muxer_factory.can_sink_any_caps(video_profile_format),
            "`{}` src is incompatible on `{}` sink",
            video_encoder_profile.factory_name(),
            muxer_profile.factory_name()
        );
        let gst_video_profile = gst_pbutils::EncodingVideoProfile::builder(video_profile_format)
            .preset_name(video_encoder_profile.factory_name())
            .presence(0)
            .build();
        gst_video_profile.set_element_properties(video_encoder_profile.element_properties());

        // Audio Encoder
        let audio_encoder_profile = self
            .audio_encoder_profile()
            .ok_or_else(|| anyhow!("Profile `{}` has no audio encoder profile", self.name()))?;
        let audio_profile_format = audio_encoder_profile.format()?;
        ensure!(
            muxer_factory.can_sink_any_caps(audio_profile_format),
            "`{}` src is incompatible on `{}` sink",
            audio_encoder_profile.factory_name(),
            muxer_profile.factory_name()
        );
        let gst_audio_profile = gst_pbutils::EncodingAudioProfile::builder(audio_profile_format)
            .preset_name(audio_encoder_profile.factory_name())
            .presence(0)
            .build();
        gst_audio_profile.set_element_properties(audio_encoder_profile.element_properties());

        // Muxer
        let gst_container_profile =
            gst_pbutils::EncodingContainerProfile::builder(muxer_profile.format()?)
                .add_profile(&gst_video_profile)
                .add_profile(&gst_audio_profile)
                .presence(0)
                .build();
        gst_container_profile.set_element_properties(muxer_profile.element_properties());

        Ok(gst_container_profile)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn new_simple_profile(
        muxer_factory_name: &str,
        video_encoder_factory_name: &str,
        audio_encoder_factory_name: &str,
    ) -> Profile {
        let p = Profile::new("");
        p.set_muxer_profile(ElementFactoryProfile::new(muxer_factory_name));
        p.set_video_encoder_profile(ElementFactoryProfile::new(video_encoder_factory_name));
        p.set_audio_encoder_profile(ElementFactoryProfile::new(audio_encoder_factory_name));
        p
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
}
