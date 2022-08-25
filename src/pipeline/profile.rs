use gst_pbutils::prelude::*;

use super::{
    caps,
    element_properties::{ElementProperties, ElementPropertiesEncodingProfileExt},
};

pub struct Builder {
    container_caps: gst::Caps,
    container_preset_name: Option<String>,
    container_element_properties: Option<ElementProperties>,

    video_caps: gst::Caps,
    video_preset_name: Option<String>,
    video_element_properties: Option<ElementProperties>,

    audio_caps: gst::Caps,
    audio_preset_name: Option<String>,
    audio_element_properties: Option<ElementProperties>,
}

#[allow(dead_code)]
impl Builder {
    pub fn new(container_caps: gst::Caps, video_caps: gst::Caps, audio_caps: gst::Caps) -> Self {
        Self {
            container_caps,
            container_preset_name: None,
            container_element_properties: None,
            video_caps,
            video_preset_name: None,
            video_element_properties: None,
            audio_caps,
            audio_preset_name: None,
            audio_element_properties: None,
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

    pub fn container_element_properties(mut self, element_properties: ElementProperties) -> Self {
        self.container_element_properties = Some(element_properties);
        self
    }

    pub fn video_element_properties(mut self, element_properties: ElementProperties) -> Self {
        self.video_element_properties = Some(element_properties);
        self
    }

    pub fn audio_element_properties(mut self, element_properties: ElementProperties) -> Self {
        self.audio_element_properties = Some(element_properties);
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

            if let Some(element_properties) = self.video_element_properties {
                profile.set_element_properties(element_properties);
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

            if let Some(element_properties) = self.audio_element_properties {
                profile.set_element_properties(element_properties);
            }

            profile
        };

        let container_profile = {
            let mut builder = gst_pbutils::EncodingContainerProfile::builder(&self.container_caps)
                .add_profile(&video_profile)
                .add_profile(&audio_profile)
                .presence(0);

            if let Some(ref preset_name) = self.container_preset_name {
                builder = builder.preset_name(preset_name);
            }

            let profile = builder.build();

            if let Some(element_properties) = self.container_element_properties {
                profile.set_element_properties(element_properties);
            }

            profile
        };

        container_profile
    }
}
