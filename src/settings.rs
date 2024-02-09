use adw::prelude::*;
use anyhow::{Context, Result};
use gettextrs::gettext;
use gsettings_macro::gen_settings;
use gtk::{gio, glib};

use std::{fs, path::PathBuf, time::Duration};

use crate::{
    config::APP_ID,
    profile::{self, Profile},
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

    /// Opens a `FileDialog` to select a folder and updates
    /// the settings with the selected folder.
    pub async fn select_saving_location(
        &self,
        parent: Option<&impl IsA<gtk::Window>>,
    ) -> Result<()> {
        let dialog = gtk::FileDialog::builder()
            .modal(true)
            .title(gettext("Select Recordings Folder"))
            .initial_folder(&gio::File::for_path(self.saving_location()))
            .build();

        match dialog.select_folder_future(parent).await {
            Ok(folder) => {
                let path = folder.path().context("Folder does not have a path")?;
                self.0.set("saving-location", path).unwrap();
            }
            Err(err) => {
                if !err.matches(gtk::DialogError::Dismissed) {
                    return Err(err.into());
                }
            }
        }

        Ok(())
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

        profile::get(&profile_id).filter(|profile| profile.is_available())
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
