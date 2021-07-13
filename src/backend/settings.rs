use gtk::{gio, glib, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;

    use crate::config::APP_ID;

    #[derive(Debug)]
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
                settings: gio::Settings::new(APP_ID),
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
        glib::Object::new::<Self>(&[]).expect("Failed to create KhaSettings")
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

    pub fn record_delay(&self) -> u32 {
        let imp = self.private();
        imp.settings.string("record-delay").parse::<u32>().unwrap()
    }

    pub fn set_saving_location(&self, directory: &str) {
        let imp = self.private();
        imp.settings
            .set_string("saving-location", directory)
            .unwrap();
    }

    pub fn saving_location(&self) -> String {
        let imp = self.private();
        let current_saving_location = imp.settings.string("saving-location");

        if current_saving_location == "default" {
            glib::user_special_dir(glib::UserDirectory::Videos)
                .as_path()
                .display()
                .to_string()
        } else {
            current_saving_location.to_string()
        }
    }
}
