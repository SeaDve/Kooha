use std::{env, sync::OnceLock};

pub fn enabled_features() -> &'static [Feature] {
    static ENABLED_FEATURES: OnceLock<Vec<Feature>> = OnceLock::new();

    ENABLED_FEATURES.get_or_init(|| {
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
    })
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
            "all" | "1" => Some(Self::All),
            "experimental-formats" => Some(Self::ExperimentalFormats),
            "multiple-video-sources" => Some(Self::MultipleVideoSources),
            "window-recording" => Some(Self::WindowRecording),
            _ => None,
        }
    }

    pub fn is_enabled(self) -> bool {
        let enabled_features = enabled_features();

        enabled_features.contains(&Self::All) || enabled_features.contains(&self)
    }
}
