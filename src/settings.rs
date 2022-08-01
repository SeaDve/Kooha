use gsettings_macro::gen_settings;
use gtk::{
    gio::{self, prelude::*},
    glib,
};

use std::path::{Path, PathBuf};

use crate::config::APP_ID;

#[gen_settings(file = "./data/io.github.seadve.Kooha.gschema.xml.in")]
#[gen_settings_skip(key_name = "saving-location")]
pub struct Settings;

impl Default for Settings {
    fn default() -> Self {
        Self::new(APP_ID)
    }
}

impl Settings {
    pub fn set_saving_location(&self, directory: &Path) {
        self.0
            .set_string("saving-location", directory.to_str().unwrap())
            .unwrap();
    }

    pub fn saving_location(&self) -> PathBuf {
        let saving_location = self.0.string("saving-location").to_string();

        if saving_location == "default" {
            glib::user_special_dir(glib::UserDirectory::Videos).unwrap_or_else(glib::home_dir)
        } else {
            PathBuf::from(saving_location)
        }
    }
}
