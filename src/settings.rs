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
};

use crate::{config::APP_ID, utils};

#[gen_settings(file = "./data/io.github.seadve.Kooha.gschema.xml.in")]
#[gen_settings_skip(key_name = "saving-location")]
pub struct Settings;

impl Default for Settings {
    fn default() -> Self {
        Self::new(APP_ID)
    }
}

impl Settings {
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

            let directory = if let Some(directory) = chooser.file().and_then(|file| file.path()) {
                directory
            } else {
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
            tracing::info!("Saving location set to {}", directory.display());
            chooser.close();
        }));
    }

    pub fn saving_location(&self) -> PathBuf {
        let saving_location: PathBuf = self.0.get("saving-location");

        if !saving_location.as_os_str().is_empty() {
            return saving_location;
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
