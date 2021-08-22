use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, WeakRef},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::OnceCell;

use std::path::Path;

use crate::{
    backend::Settings,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    utils,
    widgets::MainWindow,
};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub window: OnceCell<WeakRef<MainWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "KoohaApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self, app: &Self::Type) {
            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.show();
                window.present();
                return;
            }

            let window = MainWindow::new(app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.main_window().present();
        }

        fn startup(&self, app: &Self::Type) {
            self.parent_startup(app);
            gtk::Window::set_default_icon_name(APP_ID);

            app.setup_gactions();
            app.setup_accels();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::new(&[
            ("application-id", &Some(APP_ID)),
            ("flags", &gio::ApplicationFlags::empty()),
            ("resource-base-path", &Some("/io/github/seadve/Kooha/")),
        ])
        .expect("Failed to create Application.")
    }

    fn setup_gactions(&self) {
        let action_launch_default_for_file = gio::SimpleAction::new(
            "launch-default-for-file",
            Some(glib::VariantTy::new("s").unwrap()),
        );
        action_launch_default_for_file.connect_activate(|_, param| {
            let file_path = param.unwrap().get::<String>().unwrap();
            let uri = format!("file://{}", file_path);
            gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>).unwrap();
        });
        self.add_action(&action_launch_default_for_file);

        let action_select_saving_location = gio::SimpleAction::new("select-saving-location", None);
        action_select_saving_location.connect_activate(clone!(@weak self as app => move |_, _| {
            app.select_saving_location();
        }));
        self.add_action(&action_select_saving_location);

        let action_show_about = gio::SimpleAction::new("show-about", None);
        action_show_about.connect_activate(clone!(@weak self as app => move |_, _| {
            app.show_about_dialog();
        }));
        self.add_action(&action_show_about);

        let action_quit = gio::SimpleAction::new("quit", None);
        action_quit.connect_activate(clone!(@weak self as app => move |_, _| {
            if app.main_window().is_safe_to_quit() {
                app.quit();
            };
        }));
        self.add_action(&action_quit);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<primary>q"]);
        self.set_accels_for_action("win.record-speaker", &["<primary>a"]);
        self.set_accels_for_action("win.record-mic", &["<primary>m"]);
        self.set_accels_for_action("win.show-pointer", &["<primary>p"]);
        self.set_accels_for_action("win.toggle-record", &["<primary>r"]);
        self.set_accels_for_action("win.toggle-pause", &["<primary>k"]);
        self.set_accels_for_action("win.cancel-delay", &["<primary>c"]);
    }

    fn select_saving_location(&self) {
        let settings = Settings::new();
        let chooser = gtk::FileChooserDialogBuilder::new()
            .transient_for(&self.main_window())
            .modal(true)
            .action(gtk::FileChooserAction::SelectFolder)
            .title(&gettext("Select Recordings Folder"))
            .build();

        chooser.add_button(&gettext("_Cancel"), gtk::ResponseType::Cancel);
        chooser.add_button(&gettext("_Select"), gtk::ResponseType::Accept);
        chooser.set_default_response(gtk::ResponseType::Accept);
        chooser
            .set_current_folder(&gio::File::for_path(settings.saving_location()))
            .expect("Failed to set current folder.");

        chooser.connect_response(clone!(@weak self as app => move |chooser, response| {
            if response != gtk::ResponseType::Accept {
                chooser.destroy();
                return;
            }

            let directory = chooser.file().unwrap().path().unwrap();
            let is_accessible = utils::check_if_accessible(&directory);

            if !is_accessible {
                let error_dialog = gtk::MessageDialogBuilder::new()
                    .text(&gettext!("Cannot access “{}”", directory.to_str().unwrap()))
                    .secondary_text(&gettext("Please choose an accessible location and try again."))
                    .buttons(gtk::ButtonsType::Ok)
                    .message_type(gtk::MessageType::Error)
                    .transient_for(chooser)
                    .modal(true)
                    .build();
                error_dialog.connect_response(|error_dialog, _| error_dialog.destroy());
                error_dialog.present();
                return;
            };

            settings.set_saving_location(&directory);
            chooser.destroy();
        }));

        chooser.present();
    }

    fn show_about_dialog(&self) {
        let dialog = gtk::AboutDialogBuilder::new()
            .transient_for(&self.main_window())
            .modal(true)
            .program_name(&gettext("Kooha"))
            .comments(&gettext("Elegantly record your screen"))
            .version(VERSION)
            .logo_icon_name(APP_ID)
            .authors(vec![
                "Dave Patrick".into(),
                "".into(),
                "Mathiascode".into(),
                "Felix Weilbach".into(),
            ])
            // Translators: Replace "translator-credits" with your names. Put a comma between.
            .translator_credits(&gettext("translator-credits"))
            .copyright(&gettext("Copyright 2021 Dave Patrick"))
            .license_type(gtk::License::Gpl30)
            .website("https://github.com/SeaDve/Kooha")
            .website_label(&gettext("GitHub"))
            .build();

        dialog.show();
    }

    pub fn main_window(&self) -> MainWindow {
        let imp = imp::Application::from_instance(self);
        imp.window.get().unwrap().upgrade().unwrap()
    }

    pub fn send_record_success_notification(&self, recording_file_path: &Path) {
        let saving_location = recording_file_path
            .parent()
            .expect("Directory doesn't exist.");

        let notification = gio::Notification::new(&gettext("Screencast Recorded!"));
        notification.set_body(Some(&gettext!(
            "The recording has been saved in “{}”",
            saving_location.to_str().unwrap()
        )));
        notification.set_default_action_and_target_value(
            "app.launch-default-for-file",
            Some(&saving_location.to_str().unwrap().to_variant()),
        );
        notification.add_button_with_target_value(
            &gettext("Open File"),
            "app.launch-default-for-file",
            Some(&recording_file_path.to_str().unwrap().to_variant()),
        );

        self.send_notification(Some("record-success"), &notification);
    }

    pub fn run(&self) {
        log::info!("Kooha ({})", APP_ID);
        log::info!("Version: {} ({})", VERSION, PROFILE);
        log::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self);
    }
}
