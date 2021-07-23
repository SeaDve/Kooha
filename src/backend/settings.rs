use chrono::Local;
use gtk::{
    gio,
    glib::{self, signal::SignalHandlerId},
    prelude::*,
    subclass::prelude::*,
};

use std::path::{Path, PathBuf};

use crate::config::APP_ID;

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Settings {
        pub settings: gio::Settings,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Settings {
        const NAME: &'static str = "Settings";
        type Type = super::Settings;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                settings: gio::Settings::new(APP_ID),
            }
        }
    }

    impl ObjectImpl for Settings {}
}

glib::wrapper! {
    pub struct Settings(ObjectSubclass<imp::Settings>);
}

impl Settings {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create Settings")
    }

    fn private(&self) -> &imp::Settings {
        &imp::Settings::from_instance(self)
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

    pub fn connect_changed_notify<F: Fn(&gio::Settings, &str) + 'static>(
        &self,
        detail: Option<&str>,
        f: F,
    ) -> SignalHandlerId {
        let imp = self.private();
        imp.settings.connect_changed(detail, f)
    }

    pub fn set_saving_location(&self, directory: &Path) {
        let imp = self.private();
        imp.settings
            .set_string("saving-location", directory.to_str().unwrap())
            .unwrap();
    }

    pub fn saving_location(&self) -> PathBuf {
        let imp = self.private();
        let saving_location = imp.settings.string("saving-location").to_string();

        if saving_location == "default" {
            glib::user_special_dir(glib::UserDirectory::Videos)
        } else {
            PathBuf::from(saving_location)
        }
    }

    pub fn file_path(&self) -> PathBuf {
        let file_name = Local::now().format("Kooha %m-%d-%Y %H:%M:%S").to_string();

        let mut path = self.saving_location();
        path.set_file_name(file_name);
        path.set_extension(self.video_format());
        path
    }

    pub fn video_format(&self) -> String {
        let imp = self.private();
        imp.settings.string("video-format").to_string()
    }

    pub fn is_record_speaker(&self) -> bool {
        let imp = self.private();
        imp.settings.boolean("record-speaker")
    }

    pub fn is_record_mic(&self) -> bool {
        let imp = self.private();
        imp.settings.boolean("record-mic")
    }

    pub fn is_show_pointer(&self) -> bool {
        let imp = self.private();
        imp.settings.boolean("show-pointer")
    }

    pub fn is_selection_mode(&self) -> bool {
        let imp = self.private();
        let capture_mode = imp.settings.string("capture-mode");
        capture_mode == "selection"
    }

    pub fn video_framerate(&self) -> u32 {
        let imp = self.private();
        imp.settings.uint("video-framerate")
    }

    pub fn record_delay(&self) -> u32 {
        let imp = self.private();
        imp.settings.string("record-delay").parse().unwrap()
    }
}
