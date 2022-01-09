use chrono::Local;
use gtk::{
    gio,
    glib::{self, SignalHandlerId},
    prelude::*,
    subclass::prelude::*,
};

use std::path::{Path, PathBuf};

use crate::config::APP_ID;

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Settings(pub gio::Settings);

    #[glib::object_subclass]
    impl ObjectSubclass for Settings {
        const NAME: &'static str = "KoohaSettings";
        type Type = super::Settings;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self(gio::Settings::new(APP_ID))
        }
    }

    impl ObjectImpl for Settings {}
}

glib::wrapper! {
    pub struct Settings(ObjectSubclass<imp::Settings>);
}

impl Settings {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create Settings.")
    }

    fn inner(&self) -> &gio::Settings {
        let imp = imp::Settings::from_instance(self);
        &imp.0
    }

    pub fn create_action(&self, action: &str) -> gio::Action {
        self.inner().create_action(action)
    }

    pub fn bind_key<P: IsA<glib::Object>>(&self, key: &str, object: &P, property: &str) {
        self.inner()
            .bind(key, object, property)
            .flags(gio::SettingsBindFlags::DEFAULT)
            .build();
    }

    pub fn connect_changed_notify<F: Fn(&gio::Settings, &str) + 'static>(
        &self,
        detail: Option<&str>,
        f: F,
    ) -> SignalHandlerId {
        self.inner().connect_changed(detail, f)
    }

    pub fn set_saving_location(&self, directory: &Path) {
        self.inner()
            .set_string("saving-location", directory.to_str().unwrap())
            .unwrap();
    }

    pub fn saving_location(&self) -> PathBuf {
        let saving_location = self.inner().string("saving-location").to_string();

        if saving_location == "default" {
            glib::user_special_dir(glib::UserDirectory::Videos)
        } else {
            PathBuf::from(saving_location)
        }
    }

    pub fn file_path(&self) -> PathBuf {
        let file_name = Local::now().format("Kooha-%F-%H-%M-%S").to_string();

        let mut path = self.saving_location();
        path.push(file_name);
        path.set_extension(self.video_format());
        path
    }

    pub fn video_format(&self) -> String {
        self.inner().string("video-format").to_string()
    }

    pub fn is_record_speaker(&self) -> bool {
        self.inner().boolean("record-speaker")
    }

    pub fn is_record_mic(&self) -> bool {
        self.inner().boolean("record-mic")
    }

    pub fn is_show_pointer(&self) -> bool {
        self.inner().boolean("show-pointer")
    }

    pub fn is_selection_mode(&self) -> bool {
        let capture_mode = self.inner().string("capture-mode");
        capture_mode == "selection"
    }

    pub fn video_framerate(&self) -> u32 {
        self.inner().uint("video-framerate")
    }

    pub fn record_delay(&self) -> u32 {
        self.inner().uint("record-delay")
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
