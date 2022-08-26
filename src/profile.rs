use anyhow::{anyhow, ensure, Result};
use gst_pbutils::prelude::*;
use gtk::{glib, subclass::prelude::*};

use std::cell::RefCell;

use crate::{
    element_properties::{
        ElementFactoryPropertiesMap, ElementProperties, EncodingProfileExtManual,
    },
    utils,
};

pub enum BuiltinProfiles {
    WebM,
    Mp4,
    Matroska,
}

impl BuiltinProfiles {
    pub fn get(self) -> Profile {
        match self {
            Self::WebM => BUILTIN_PROFILES.with(|profiles| profiles[0].clone()),
            Self::Mp4 => BUILTIN_PROFILES.with(|profiles| profiles[1].clone()),
            Self::Matroska => BUILTIN_PROFILES.with(|profiles| profiles[2].clone()),
        }
    }
}

thread_local! {
    static BUILTIN_PROFILES: Vec<Profile> = vec![
        {
            let profile = Profile::new("WebM");
            profile.set_container_preset_name("webmmux");
            profile.set_video_preset_name("vp8enc");
            profile.set_video_element_properties(
                ElementProperties::builder()
                    .item(
                        ElementFactoryPropertiesMap::builder("vp8enc")
                            .field("max-quantizer", 17)
                            .field("cpu-used", 16)
                            .field("cq-level", 13)
                            .field("deadline", 1)
                            .field("static-threshold", 100)
                            .field_from_str("keyframe-mode", "disabled")
                            .field("buffer-size", 20000)
                            .field("threads", utils::ideal_thread_count())
                            .build(),
                    )
                    .build(),
            );
            profile.set_audio_preset_name("opusenc");
            profile
        },
        {
            // TODO support "profile" = baseline
            let profile = Profile::new("MP4");
            profile.set_container_preset_name("mp4mux");
            profile.set_video_preset_name("x264enc");
            profile.set_video_element_properties(
                ElementProperties::builder()
                    .item(
                        ElementFactoryPropertiesMap::builder("x264enc")
                            .field("qp-max", 17)
                            .field_from_str("speed-preset", "superfast")
                            .field("threads", utils::ideal_thread_count())
                            .build(),
                    )
                    .build(),
            );
            profile.set_audio_preset_name("lamemp3enc");
            profile
        },
        {
            let profile = Profile::new("Matroska");
            profile.set_container_preset_name("matroskamux");
            profile.set_video_preset_name("x264enc");
            profile.set_video_element_properties(
                ElementProperties::builder()
                    .item(
                        ElementFactoryPropertiesMap::builder("x264enc")
                            .field("qp-max", 17)
                            .field_from_str("speed-preset", "superfast")
                            .field("threads", utils::ideal_thread_count())
                            .build(),
                    )
                    .build(),
            );
            profile.set_audio_preset_name("opusenc");
            profile
        },
    ];
}

mod imp {
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct Profile {
        pub(super) name: RefCell<String>,
        pub(super) container_preset_name: RefCell<String>,
        pub(super) container_element_properties: RefCell<ElementProperties>,
        pub(super) video_preset_name: RefCell<String>,
        pub(super) video_element_properties: RefCell<ElementProperties>,
        pub(super) audio_preset_name: RefCell<String>,
        pub(super) audio_element_properties: RefCell<ElementProperties>,
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
                        .flags(
                            glib::ParamFlags::READWRITE
                                | glib::ParamFlags::EXPLICIT_NOTIFY
                                | glib::ParamFlags::CONSTRUCT,
                        )
                        .build(),
                    glib::ParamSpecString::builder("container-preset-name")
                        .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                        .build(),
                    glib::ParamSpecBoxed::builder(
                        "container-element-properties",
                        ElementProperties::static_type(),
                    )
                    .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                    .build(),
                    glib::ParamSpecString::builder("video-preset-name")
                        .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                        .build(),
                    glib::ParamSpecBoxed::builder(
                        "video-element-properties",
                        ElementProperties::static_type(),
                    )
                    .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                    .build(),
                    glib::ParamSpecString::builder("audio-preset-name")
                        .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                        .build(),
                    glib::ParamSpecBoxed::builder(
                        "audio-element-properties",
                        ElementProperties::static_type(),
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
                "container-preset-name" => {
                    let container_preset_name = value.get().unwrap();
                    obj.set_container_preset_name(container_preset_name);
                }
                "container-element-properties" => {
                    let container_element_properties = value.get().unwrap();
                    obj.set_container_element_properties(container_element_properties);
                }
                "video-preset-name" => {
                    let video_preset_name = value.get().unwrap();
                    obj.set_video_preset_name(video_preset_name);
                }
                "video-element-properties" => {
                    let video_element_properties = value.get().unwrap();
                    obj.set_video_element_properties(video_element_properties);
                }
                "audio-preset-name" => {
                    let audio_preset_name = value.get().unwrap();
                    obj.set_audio_preset_name(audio_preset_name);
                }
                "audio-element-properties" => {
                    let audio_element_properties = value.get().unwrap();
                    obj.set_audio_element_properties(audio_element_properties);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "name" => obj.name().to_value(),
                "container-preset-name" => obj.container_preset_name().to_value(),
                "container-element-properties" => obj.container_element_properties().to_value(),
                "video-preset-name" => obj.video_preset_name().to_value(),
                "video-element-properties" => obj.video_element_properties().to_value(),
                "audio-preset-name" => obj.audio_preset_name().to_value(),
                "audio-element-properties" => obj.audio_element_properties().to_value(),
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

    pub fn set_container_preset_name(&self, name: &str) {
        if name == self.container_preset_name() {
            return;
        }

        self.imp().container_preset_name.replace(name.to_string());
        self.notify("container-preset-name");
    }

    pub fn container_preset_name(&self) -> String {
        self.imp().container_preset_name.borrow().clone()
    }

    pub fn set_container_element_properties(&self, properties: ElementProperties) {
        if properties == self.container_element_properties() {
            return;
        }

        self.imp().container_element_properties.replace(properties);
        self.notify("container-element-properties");
    }

    pub fn container_element_properties(&self) -> ElementProperties {
        self.imp().container_element_properties.borrow().clone()
    }

    pub fn set_video_preset_name(&self, name: &str) {
        if name == self.video_preset_name() {
            return;
        }

        self.imp().video_preset_name.replace(name.to_string());
        self.notify("video-preset-name");
    }

    pub fn video_preset_name(&self) -> String {
        self.imp().video_preset_name.borrow().clone()
    }

    pub fn set_video_element_properties(&self, properties: ElementProperties) {
        if properties == self.video_element_properties() {
            return;
        }

        self.imp().video_element_properties.replace(properties);
        self.notify("video-element-properties");
    }

    pub fn video_element_properties(&self) -> ElementProperties {
        self.imp().video_element_properties.borrow().clone()
    }

    pub fn set_audio_preset_name(&self, name: &str) {
        if name == self.audio_preset_name() {
            return;
        }

        self.imp().audio_preset_name.replace(name.to_string());
        self.notify("audio-preset-name");
    }

    pub fn audio_preset_name(&self) -> String {
        self.imp().audio_preset_name.borrow().clone()
    }

    pub fn set_audio_element_properties(&self, properties: ElementProperties) {
        if properties == self.audio_element_properties() {
            return;
        }

        self.imp().audio_element_properties.replace(properties);
        self.notify("audio-element-properties");
    }

    pub fn audio_element_properties(&self) -> ElementProperties {
        self.imp().audio_element_properties.borrow().clone()
    }

    pub fn to_encoding_profile(&self) -> Result<gst_pbutils::EncodingContainerProfile> {
        let container_preset_name = self.container_preset_name();
        let container_element_factory = find_element_factory(&container_preset_name)?;
        let container_format_caps = profile_format_from_factory(&container_element_factory)?;

        // Video Encoder
        let video_preset_name = self.video_preset_name();
        let video_element_factory = find_element_factory(&video_preset_name)?;
        let video_format_caps = profile_format_from_factory(&video_element_factory)?;
        ensure!(
            container_element_factory.can_sink_any_caps(&video_format_caps),
            "`{}` src is incompatible on `{}` sink",
            video_preset_name,
            container_preset_name
        );
        let video_profile = gst_pbutils::EncodingVideoProfile::builder(&video_format_caps)
            .preset_name(&self.video_preset_name())
            .presence(0)
            .build();
        video_profile.set_element_properties(self.video_element_properties());

        // Audio Encoder
        let audio_preset_name = self.audio_preset_name();
        let audio_element_factory = find_element_factory(&audio_preset_name)?;
        let audio_format_caps = profile_format_from_factory(&audio_element_factory)?;
        ensure!(
            container_element_factory.can_sink_any_caps(&audio_format_caps),
            "`{}` src is incompatible on `{}` sink",
            audio_preset_name,
            container_preset_name
        );
        let audio_profile = gst_pbutils::EncodingAudioProfile::builder(&audio_format_caps)
            .preset_name(&self.audio_preset_name())
            .presence(0)
            .build();
        audio_profile.set_element_properties(self.audio_element_properties());

        // Muxer
        let container_profile =
            gst_pbutils::EncodingContainerProfile::builder(&container_format_caps)
                .add_profile(&video_profile)
                .add_profile(&audio_profile)
                .presence(0)
                .build();
        container_profile.set_element_properties(self.container_element_properties());

        Ok(container_profile)
    }
}

fn find_element_factory(factory_name: &str) -> Result<gst::ElementFactory> {
    gst::ElementFactory::find(factory_name)
        .ok_or_else(|| anyhow!("Failed to find factory `{}`", factory_name))
}

fn profile_format_from_factory(factory: &gst::ElementFactory) -> Result<gst::Caps> {
    let factory_name = factory.name();

    ensure!(
        factory.has_type(gst::ElementFactoryType::ENCODER | gst::ElementFactoryType::MUXER),
        "Factory`{}` must be an encoder or muxer to be used in a profile",
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
        container_preset_name: &str,
        video_preset_name: &str,
        audio_preset_name: &str,
    ) -> Profile {
        let profile = Profile::new("");
        profile.set_container_preset_name(container_preset_name);
        profile.set_video_preset_name(video_preset_name);
        profile.set_audio_preset_name(audio_preset_name);
        profile
    }

    #[test]
    fn builtins() {
        assert_eq!(BuiltinProfiles::WebM.get().name(), "WebM");
        assert_eq!(BuiltinProfiles::Mp4.get().name(), "MP4");
        assert_eq!(BuiltinProfiles::Matroska.get().name(), "Matroska");

        BUILTIN_PROFILES.with(|profiles| {
            profiles
                .iter()
                .for_each(|profile| assert!(profile.to_encoding_profile().is_ok()));
        });
    }

    #[test]
    fn incompatibles() {
        let a = new_simple_profile("webmmux", "x264enc", "opusenc"); // webmmux does not support x264enc
        assert!(a
            .to_encoding_profile()
            .err()
            .unwrap()
            .to_string()
            .contains("`x264enc` src is incompatible on `webmmux` sink"));

        let b = new_simple_profile("webmmux", "vp8enc", "lamemp3enc"); // webmmux does not support lamemp3enc
        assert!(b
            .to_encoding_profile()
            .err()
            .unwrap()
            .to_string()
            .contains("`lamemp3enc` src is incompatible on `webmmux` sink"));
    }

    #[test]
    fn test_profile_format_from_factory_name() {
        assert_eq!(
            profile_format_from_factory(&find_element_factory("vp8enc").unwrap()).unwrap(),
            gst::Caps::builder("video/x-vp8").build(),
        );
        assert_eq!(
            profile_format_from_factory(&find_element_factory("opusenc").unwrap()).unwrap(),
            gst::Caps::builder("audio/x-opus").build(),
        );
        assert_eq!(
            profile_format_from_factory(&find_element_factory("matroskamux").unwrap()).unwrap(),
            gst::Caps::builder("video/x-matroska").build(),
        );
        assert!(
            profile_format_from_factory(&find_element_factory("audioconvert").unwrap())
                .err()
                .unwrap()
                .to_string()
                .contains("must be an encoder or muxer"),
        );
    }
}
