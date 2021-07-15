use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::backend::KhaSettings;
use crate::config::{APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::widgets::KhaWindow;

mod imp {
    use super::*;
    use glib::WeakRef;
    use once_cell::sync::OnceCell;

    #[derive(Debug, Default)]
    pub struct KhaApplication {
        pub window: OnceCell<WeakRef<KhaWindow>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaApplication {
        const NAME: &'static str = "KhaApplication";
        type Type = super::KhaApplication;
        type ParentType = gtk::Application;
    }

    impl ObjectImpl for KhaApplication {}

    impl ApplicationImpl for KhaApplication {
        fn activate(&self, app: &Self::Type) {
            log::debug!("GtkApplication<KhaApplication>::activate");

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.show();
                window.present();
                return;
            }

            let window = KhaWindow::new(app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.main_window().present();
        }

        fn startup(&self, app: &Self::Type) {
            log::debug!("GtkApplication<KhaApplication>::startup");
            self.parent_startup(app);

            gtk::Window::set_default_icon_name(APP_ID);

            app.setup_css();
            app.setup_actions();
        }
    }

    impl GtkApplicationImpl for KhaApplication {}
}

glib::wrapper! {
    pub struct KhaApplication(ObjectSubclass<imp::KhaApplication>)
        @extends gio::Application, gtk::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl KhaApplication {
    pub fn new() -> Self {
        glib::Object::new(&[
            ("application-id", &Some(APP_ID)),
            ("flags", &gio::ApplicationFlags::empty()),
            ("resource-base-path", &Some("/io/github/seadve/Kooha/")),
        ])
        .expect("Failed to initialize KhaApplication")
    }

    fn main_window(&self) -> KhaWindow {
        let imp = imp::KhaApplication::from_instance(self);
        imp.window.get().unwrap().upgrade().unwrap()
    }

    fn setup_actions(&self) {
        let action_about = gio::SimpleAction::new("select-saving-location", None);
        action_about.connect_activate(clone!(@weak self as app => move |_, _| {
            app.select_saving_location();
        }));
        self.add_action(&action_about);

        let action_about = gio::SimpleAction::new("show-about", None);
        action_about.connect_activate(clone!(@weak self as app => move |_, _| {
            app.show_about_dialog();
        }));
        self.add_action(&action_about);

        let action_quit = gio::SimpleAction::new("quit", None);
        action_quit.connect_activate(clone!(@weak self as app => move |_, _| {
            app.quit();
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
        let settings = KhaSettings::new();
        let chooser = gtk::FileChooserDialogBuilder::new()
            .transient_for(&self.main_window())
            .modal(true)
            .action(gtk::FileChooserAction::SelectFolder)
            .title(&gettext("Select a Folder"))
            .build();

        chooser.add_button(&gettext("_Cancel"), gtk::ResponseType::Cancel);
        chooser.add_button(&gettext("_Select"), gtk::ResponseType::Accept);
        chooser.set_default_response(gtk::ResponseType::Accept);
        chooser
            .set_current_folder(&gio::File::for_path(settings.saving_location()))
            .expect("Failed to set current folder");

        chooser.connect_response(clone!(@weak self as app => move |chooser, response| {
            if response != gtk::ResponseType::Accept {
                chooser.close();
                return;
            }
            let directory = chooser.file().unwrap().path().unwrap().as_path().display().to_string();
            let homefolder = glib::home_dir().as_path().display().to_string();
            let is_in_homefolder = directory.starts_with(&homefolder);
            if !is_in_homefolder || directory == homefolder {
                let error_dialog = gtk::MessageDialogBuilder::new()
                    .modal(true)
                    .buttons(gtk::ButtonsType::Ok)
                    .transient_for(&app.main_window())
                    .title(&gettext(&format!("Inaccessible location '{}'", directory)))
                    .text(&gettext("Please choose an accessible location and retry."))
                    .build();
                error_dialog.connect_response(move |error_dialog, _| { error_dialog.close() });
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
            .translator_credits(&gettext("translator-credits"))
            .copyright(&gettext("Copyright 2021 Dave Patrick"))
            .license_type(gtk::License::Gpl30)
            .website("https://github.com/SeaDve/Kooha")
            .website_label(&gettext("GitHub"))
            .build();

        dialog.show();
    }

    pub fn run(&self) {
        log::info!("Kooha ({})", APP_ID);
        log::info!("Version: {} ({})", VERSION, PROFILE);
        log::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self);
    }
}
