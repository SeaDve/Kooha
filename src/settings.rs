use adw::prelude::*;
use gettextrs::gettext;
use gsettings_macro::gen_settings;
use gtk::{
    gio,
    glib::{self, clone},
};

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{
    config::APP_ID,
    profile::{self, Profile},
    utils,
};

#[gen_settings(file = "./data/io.github.seadve.Kooha.gschema.xml.in")]
#[gen_settings_skip(key_name = "saving-location")]
#[gen_settings_skip(key_name = "record-delay")]
#[gen_settings_skip(key_name = "profile-id")]
pub struct Settings;

impl Default for Settings {
    fn default() -> Self {
        Self::new(APP_ID)
    }
}

impl Settings {
    pub const NONE_PROFILE_ID: &'static str = "none";

    /// Opens a `FileChooserDialog` to select a folder and updates
    /// the settings with the selected folder.
    pub fn select_saving_location(&self, transient_for: Option<&impl IsA<gtk::Window>>) {
        let chooser = gtk::FileChooserDialog::builder()
            .modal(true)
            .action(gtk::FileChooserAction::SelectFolder)
            .title(&gettext("Select Recordings Folder"))
            .build();
        chooser.set_transient_for(transient_for);

        if let Err(err) =
            chooser.set_current_folder(Some(&gio::File::for_path(self.saving_location())))
        {
            tracing::warn!("Failed to set current folder: {:?}", err);
        }

        chooser.add_button(&gettext("_Cancel"), gtk::ResponseType::Cancel);
        chooser.add_button(&gettext("_Select"), gtk::ResponseType::Accept);
        chooser.set_default_response(gtk::ResponseType::Accept);

        chooser.present();

        let inner = &self.0;
        chooser.connect_response(clone!(@weak inner => move |chooser, response| {
            if response != gtk::ResponseType::Accept {
                chooser.close();
                return;
            }

            let Some(directory) = chooser.file().and_then(|file| file.path()) else {
                present_message(
                    &gettext("No folder selected"),
                    &gettext("Please choose a folder and try again."),
                    Some(chooser),
                );
                return;
            };

            if !is_accessible(&directory) {
                present_message(
                    // Translators: {} will be replaced with a path to the folder.
                    &gettext!("Cannot access “{}”", directory.display()),
                    &gettext("Please choose an accessible location and try again."),
                    Some(chooser),
                );
                return;
            }

            inner.set("saving-location", &directory).unwrap();
            chooser.close();
        }));
    }

    pub fn saving_location(&self) -> PathBuf {
        let stored_saving_location: PathBuf = self.0.get("saving-location");

        if !stored_saving_location.as_os_str().is_empty() {
            return stored_saving_location;
        }

        let saving_location =
            glib::user_special_dir(glib::UserDirectory::Videos).unwrap_or_else(glib::home_dir);

        let kooha_saving_location = saving_location.join("Kooha");

        if let Err(err) = fs::create_dir_all(&kooha_saving_location) {
            tracing::warn!(
                "Failed to create dir at `{}`: {:?}",
                kooha_saving_location.display(),
                err
            );
            return saving_location;
        }

        kooha_saving_location
    }

    pub fn connect_saving_location_changed(
        &self,
        f: impl Fn(&Self) + 'static,
    ) -> gio::glib::SignalHandlerId {
        self.0
            .connect_changed(Some("saving-location"), move |settings, _| {
                f(&Self(settings.clone()));
            })
    }

    pub fn record_delay(&self) -> Duration {
        Duration::from_secs(self.0.get::<u32>("record-delay") as u64)
    }

    pub fn create_record_delay_action(&self) -> gio::Action {
        self.0.create_action("record-delay")
    }

    pub fn bind_record_delay<'a>(
        &'a self,
        object: &'a impl IsA<gio::glib::Object>,
        property: &'a str,
    ) -> gio::BindingBuilder<'a> {
        self.0.bind("record-delay", object, property)
    }

    pub fn set_profile(&self, profile: Option<&dyn Profile>) {
        self.0
            .set_string(
                "profile-id",
                profile.map_or(Self::NONE_PROFILE_ID, |profile| profile.id()),
            )
            .unwrap();
    }

    pub fn profile(&self) -> Option<Box<dyn Profile>> {
        let profile_id = self.0.get::<String>("profile-id");

        if profile_id.is_empty() || profile_id == Self::NONE_PROFILE_ID {
            return None;
        }

        if let Some(profile) = profile::get(&profile_id) {
            if !profile.is_available() {
                return None;
            }

            return Some(profile);
        }

        tracing::warn!("Profile with id `{}` not found", profile_id);
        None
    }

    pub fn connect_profile_changed(
        &self,
        f: impl Fn(&Self) + 'static,
    ) -> gio::glib::SignalHandlerId {
        self.0
            .connect_changed(Some("profile-id"), move |settings, _| {
                f(&Self(settings.clone()));
            })
    }

    pub fn reset_profile(&self) {
        self.0.reset("profile-id");
    }
}

fn present_message(heading: &str, body: &str, transient_for: Option<&impl IsA<gtk::Window>>) {
    let dialog = adw::MessageDialog::builder()
        .heading(heading)
        .body(body)
        .default_response("ok")
        .modal(true)
        .build();
    dialog.add_response("ok", &gettext("Ok"));
    dialog.set_transient_for(transient_for);
    dialog.present();
}

fn is_accessible(path: &Path) -> bool {
    if !utils::is_flatpak() {
        return true;
    }

    let home_folder = glib::home_dir();

    path != home_folder && path.starts_with(&home_folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{env, process::Command, sync::Once};

    fn setup_schema() {
        static INIT: Once = Once::new();

        INIT.call_once(|| {
            let schema_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/data");

            let output = Command::new("glib-compile-schemas")
                .arg(schema_dir)
                .output()
                .unwrap();

            if !output.status.success() {
                panic!(
                    "Failed to compile GSchema for tests; stdout: {}; stderr: {}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            env::set_var("GSETTINGS_SCHEMA_DIR", schema_dir);
            env::set_var("GSETTINGS_BACKEND", "memory");
        });
    }

    #[test]
    fn default_profile() {
        setup_schema();

        assert!(Settings::default().profile().is_some());
        assert!(Settings::default().profile().unwrap().supports_audio());
    }
}
