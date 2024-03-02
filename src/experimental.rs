use std::env;

use once_cell::sync::Lazy;

static ENABLED_FEATURES: Lazy<Vec<Feature>> = Lazy::new(|| {
    env::var("KOOHA_EXPERIMENTAL")
        .map(|val| {
            val.split(',')
                .filter_map(|raw_feature_str| {
                    let feature_str = raw_feature_str.trim().to_lowercase();
                    let feature = Feature::from_str(&feature_str);
                    if feature.is_none() {
                        tracing::warn!("Unknown `{}` experimental feature", feature_str);
                    }
                    feature
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
});

pub fn enabled_features() -> &'static [Feature] {
    ENABLED_FEATURES.as_ref()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    All,
    ExperimentalFormats,
    MultipleVideoSources,
    WindowRecording,
}

impl Feature {
    fn from_str(string: &str) -> Option<Self> {
        match string {
            "all" => Some(Self::All),
            "experimental-formats" => Some(Self::ExperimentalFormats),
            "multiple-video-sources" => Some(Self::MultipleVideoSources),
            "window-recording" => Some(Self::WindowRecording),
            _ => None,
        }
    }

    pub fn is_enabled(self) -> bool {
        ENABLED_FEATURES.contains(&Self::All) || ENABLED_FEATURES.contains(&self)
    }
}
