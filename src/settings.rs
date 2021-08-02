use chrono::Local;
use gtk::{
    gio,
    glib::{self, signal::SignalHandlerId},
    prelude::*,
};

use std::path::{Path, PathBuf};

use crate::config::APP_ID;

fn settings() -> gio::Settings {
    gio::Settings::new(APP_ID)
}

pub fn create_action(action: &str) -> gio::Action {
    settings().create_action(action)
}

pub fn bind<P: IsA<glib::Object>>(key: &str, object: &P, property: &str) {
    settings()
        .bind(key, object, property)
        .flags(gio::SettingsBindFlags::DEFAULT)
        .build();
}

pub fn connect_changed_notify<F: Fn(&gio::Settings, &str) + 'static>(
    detail: Option<&str>,
    f: F,
) -> SignalHandlerId {
    settings().connect_changed(detail, f)
}

pub fn set_saving_location(directory: &Path) {
    settings()
        .set_string("saving-location", directory.to_str().unwrap())
        .unwrap();
}

pub fn saving_location() -> PathBuf {
    let saving_location = settings().string("saving-location").to_string();

    if saving_location == "default" {
        glib::user_special_dir(glib::UserDirectory::Videos)
    } else {
        PathBuf::from(saving_location)
    }
}

pub fn file_path() -> PathBuf {
    let file_name = Local::now().format("Kooha %m-%d-%Y %H:%M:%S").to_string();

    let mut path = saving_location();
    path.push(file_name);
    path.set_extension(video_format());
    path
}

pub fn video_format() -> String {
    settings().string("video-format").to_string()
}

pub fn is_record_speaker() -> bool {
    settings().boolean("record-speaker")
}

pub fn is_record_mic() -> bool {
    settings().boolean("record-mic")
}

pub fn is_show_pointer() -> bool {
    settings().boolean("show-pointer")
}

pub fn is_selection_mode() -> bool {
    let capture_mode = settings().string("capture-mode");
    capture_mode == "selection"
}

pub fn video_framerate() -> u32 {
    settings().uint("video-framerate")
}

pub fn record_delay() -> u32 {
    settings().string("record-delay").parse().unwrap()
}
