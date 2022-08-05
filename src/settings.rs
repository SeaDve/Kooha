use adw::prelude::*;
use gettextrs::gettext;
use gsettings_macro::gen_settings;
use gtk::{gio, glib};

use std::path::{Path, PathBuf};

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
    pub async fn select_saving_location(&self, transient_for: Option<&impl IsA<gtk::Window>>) {
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

        let response = chooser.run_future().await;

        if response != gtk::ResponseType::Accept {
            chooser.close();
            return;
        }

        if let Some(ref directory) = chooser.file().and_then(|file| file.path()) {
            if is_accessible(directory) {
                self.0.set("saving-location", directory).unwrap();
                tracing::info!("Saving location set to {}", directory.display());
            } else {
                let err_dialog = adw::MessageDialog::builder()
                    .heading(&gettext!("Cannot access “{}”", directory.display()))
                    .body(&gettext(
                        "Please choose an accessible location and try again.",
                    ))
                    .default_response("ok")
                    .modal(true)
                    .build();
                err_dialog.add_response("ok", &gettext("Ok"));
                err_dialog.set_transient_for(transient_for);
                err_dialog.present();
            }
        } else {
            tracing::info!("No saving location selected");
        }

        chooser.close();
    }

    pub fn saving_location(&self) -> PathBuf {
        let saving_location: PathBuf = self.0.get("saving-location");

        if saving_location.as_os_str().is_empty() {
            glib::user_special_dir(glib::UserDirectory::Videos).unwrap_or_else(glib::home_dir)
        } else {
            saving_location
        }
    }
}

fn is_accessible(path: &Path) -> bool {
    if !utils::is_flatpak() {
        return true;
    }

    let home_folder = glib::home_dir();

    path != home_folder && path.starts_with(&home_folder)
}
