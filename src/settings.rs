use gettextrs::gettext;
use gsettings_macro::gen_settings;
use gtk::{gio, glib, prelude::*};

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
            log::warn!("Failed to set current folder: {:?}", err);
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
                let directory_str = directory.to_str().unwrap();
                self.0.set_string("saving-location", directory_str).unwrap();
                log::info!("Saving location set to {}", directory_str);
            } else {
                let error_dialog = gtk::MessageDialog::builder()
                    .text(&gettext!("Cannot access “{}”", directory.to_str().unwrap()))
                    .secondary_text(&gettext(
                        "Please choose an accessible location and try again.",
                    ))
                    .buttons(gtk::ButtonsType::Ok)
                    .message_type(gtk::MessageType::Error)
                    .modal(true)
                    .build();
                error_dialog.set_transient_for(transient_for);
                error_dialog.connect_response(|error_dialog, _| error_dialog.close());
                error_dialog.present();
            }
        } else {
            log::info!("No saving location selected");
        }

        chooser.close();
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

fn is_accessible(path: &Path) -> bool {
    if !ashpd::is_sandboxed() {
        return true;
    }

    let home_folder = glib::home_dir();

    path != home_folder && path.starts_with(&home_folder)
}
