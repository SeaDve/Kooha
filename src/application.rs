use gtk::{
    gdk, gio,
    glib::{self, clone, WeakRef},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::OnceCell;

use std::path::Path;

use crate::{
    backend::Settings,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    i18n::{i18n, i18n_f},
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
        const NAME: &'static str = "Application";
        type Type = super::Application;
        type ParentType = gtk::Application;
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

            app.setup_css();
            app.setup_actions();
        }
    }

    impl GtkApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::new(&[
            ("application-id", &Some(APP_ID)),
            ("flags", &gio::ApplicationFlags::empty()),
            ("resource-base-path", &Some("/io/github/seadve/Kooha/")),
        ])
        .expect("Failed to initialize Application")
    }

    fn main_window(&self) -> MainWindow {
        let imp = imp::Application::from_instance(self);
        imp.window.get().unwrap().upgrade().unwrap()
    }

    fn setup_actions(&self) {
        let action_show_saving_location = gio::SimpleAction::new(
            "show-saving-location",
            Some(glib::VariantTy::new("s").unwrap()),
        );
        action_show_saving_location.connect_activate(clone!(@weak self as app => move |_, param| {
            let saving_location = param.unwrap().get::<String>().unwrap();
            let uri = format!("file://{}", saving_location);
            gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>).unwrap();
        }));
        self.add_action(&action_show_saving_location);

        let action_show_saved_recording = gio::SimpleAction::new(
            "show-saved-recording",
            Some(glib::VariantTy::new("s").unwrap()),
        );
        action_show_saved_recording.connect_activate(clone!(@weak self as app => move |_, param| {
            let saved_recording = param.unwrap().get::<String>().unwrap();
            let uri = format!("file://{}", saved_recording);
            gio::AppInfo::launch_default_for_uri(&uri, None::<&gio::AppLaunchContext>).unwrap();
        }));
        self.add_action(&action_show_saved_recording);

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

        self.set_accels_for_action("app.quit", &["<Primary>q"]);
        self.set_accels_for_action("win.record-speaker", &["<Primary>a"]);
        self.set_accels_for_action("win.record-mic", &["<Primary>m"]);
        self.set_accels_for_action("win.show-pointer", &["<Primary>p"]);
        self.set_accels_for_action("win.toggle-record", &["<Primary>r"]);
        self.set_accels_for_action("win.toggle-pause", &["<Primary>k"]);
        self.set_accels_for_action("win.cancel-delay", &["<Primary>c"]);
    }

    fn setup_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/io/github/seadve/Kooha/style.css");
        if let Some(display) = gdk::Display::default() {
            gtk::StyleContext::add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn select_saving_location(&self) {
        let settings = Settings::new();
        let chooser = gtk::FileChooserDialogBuilder::new()
            .transient_for(&self.main_window())
            .modal(true)
            .action(gtk::FileChooserAction::SelectFolder)
            .title(&i18n("Select a Folder"))
            .build();

        chooser.add_button(&i18n("_Cancel"), gtk::ResponseType::Cancel);
        chooser.add_button(&i18n("_Select"), gtk::ResponseType::Accept);
        chooser.set_default_response(gtk::ResponseType::Accept);
        chooser
            .set_current_folder(&gio::File::for_path(settings.saving_location()))
            .expect("Failed to set current folder");

        chooser.connect_response(clone!(@weak self as app => move |chooser, response| {
            if response != gtk::ResponseType::Accept {
                chooser.close();
                return;
            }

            let directory = chooser.file().unwrap().path().unwrap();
            let is_accessible = utils::check_if_accessible(&directory);

            if !is_accessible {
                let error_dialog = gtk::MessageDialogBuilder::new()
                    .modal(true)
                    .buttons(gtk::ButtonsType::Ok)
                    .transient_for(&app.main_window())
                    .title(&i18n_f("Inaccessible location '{}'", &[directory.to_str().unwrap()]))
                    .text(&i18n("Please choose an accessible location and retry."))
                    .build();
                error_dialog.connect_response(|error_dialog, _| error_dialog.close());
                error_dialog.present();
                return;
            };

            settings.set_saving_location(&directory);
            chooser.close();
        }));

        chooser.present()
    }

    fn show_about_dialog(&self) {
        let dialog = gtk::AboutDialogBuilder::new()
            .transient_for(&self.main_window())
            .modal(true)
            .program_name(&i18n("Kooha"))
            .comments(&i18n("Elegantly record your screen"))
            .version(VERSION)
            .logo_icon_name(APP_ID)
            .authors(vec![
                "Dave Patrick".into(),
                "".into(),
                "Mathiascode".into(),
                "Felix Weilbach".into(),
            ])
            // Translators: Replace "translator-credits" with your names. Put a comma between.
            .translator_credits(&i18n("translator-credits"))
            .copyright(&i18n("Copyright 2021 Dave Patrick"))
            .license_type(gtk::License::Gpl30)
            .website("https://github.com/SeaDve/Kooha")
            .website_label(&i18n("GitHub"))
            .build();

        dialog.show();
    }

    pub fn send_record_success_notification(&self, recording_file_path: &Path) {
        let saving_location = recording_file_path.parent().expect("File doesn't exist");
        let saving_location_str = saving_location.to_str().unwrap();
        let saving_location_variant = saving_location_str.to_variant();
        let recording_file_path_variant = recording_file_path.to_str().unwrap().to_variant();

        let notification = gio::Notification::new(&i18n("Screencast Recorded!"));
        notification.set_body(Some(&i18n_f(
            "The recording has been saved in {}",
            &[saving_location_str],
        )));
        notification.set_default_action_and_target_value(
            "app.show-saving-location",
            Some(&saving_location_variant),
        );
        notification.add_button_with_target_value(
            &i18n("Open File"),
            "app.show-saved-recording",
            Some(&recording_file_path_variant),
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
