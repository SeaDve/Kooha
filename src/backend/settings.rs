use crate::config;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};

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

    pub fn create_action(&self, action: &str) -> gio::Action {
        let self_ = &imp::KhaSettings::from_instance(self);

        self_.settings.create_action(action)
    }

    pub fn bind_property<P: IsA<glib::Object>>(
        &self,
        source_property: &str,
        object: &P,
        target_property: &str,
    ) {
        let self_ = &imp::KhaSettings::from_instance(self);

        self_
            .settings
            .bind(source_property, object, target_property)
            .flags(gio::SettingsBindFlags::DEFAULT)
            .build();
    }
}
