use gtk::{gio, glib, prelude::*, subclass::prelude::*};

use crate::config;

mod imp {
    use super::*;

    pub struct KhaSettings {
        pub settings: gio::Settings,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaSettings {
        const NAME: &'static str = "KhaSettings";
        type Type = super::KhaSettings;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                settings: gio::Settings::new(config::APP_ID),
            }
        }
    }

    impl ObjectImpl for KhaSettings {}
}

glib::wrapper! {
    pub struct KhaSettings(ObjectSubclass<imp::KhaSettings>);
}

impl KhaSettings {
    pub fn new() -> Self {
        let obj: Self =
            glib::Object::new::<Self>(&[]).expect("Failed to initialize Settings object");
        obj
    }

    fn private(&self) -> &imp::KhaSettings {
        &imp::KhaSettings::from_instance(self)
    }

    pub fn create_action(&self, action: &str) -> gio::Action {
        let imp = self.private();

        imp.settings.create_action(action)
    }

    pub fn bind_property<P: IsA<glib::Object>>(
        &self,
        source_property: &str,
        object: &P,
        target_property: &str,
    ) {
        let imp = self.private();

        imp.settings
            .bind(source_property, object, target_property)
            .flags(gio::SettingsBindFlags::DEFAULT)
            .build();
    }

    pub fn record_delay(&self) -> i32 {
        let imp = self.private();
        imp.settings.string("record-delay").parse::<i32>().unwrap()
    }
}
