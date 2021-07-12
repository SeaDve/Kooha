use crate::config;
use crate::widgets::KhaWindow;

use gio::ApplicationFlags;
use glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use log::{debug, info};

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
            debug!("GtkApplication<KhaApplication>::activate");

            if let Some(window) = self.window.get() {
                let window = window.upgrade().unwrap();
                window.show();
                window.present();
                return;
            }

            app.set_resource_base_path(Some("/io/github/seadve/Kooha/"));
            app.setup_css();

            let window = KhaWindow::new(app);
            self.window
                .set(window.downgrade())
                .expect("Window already set.");

            app.setup_gactions();
            app.setup_accels();

            app.get_main_window().present();
        }

        fn startup(&self, app: &Self::Type) {
            debug!("GtkApplication<KhaApplication>::startup");
            self.parent_startup(app);
        }
    }

    impl GtkApplicationImpl for KhaApplication {}
}

glib::wrapper! {
    pub struct KhaApplication(ObjectSubclass<imp::KhaApplication>)
        @extends gio::Application, gtk::Application, @implements gio::ActionMap, gio::ActionGroup;
}

impl KhaApplication {
    pub fn new() -> Self {
        glib::Object::new(&[
            ("application-id", &Some(config::APP_ID)),
            ("flags", &ApplicationFlags::empty()),
        ])
        .expect("Application initialization failed...")
    }

    fn get_main_window(&self) -> KhaWindow {
        let imp = imp::KhaApplication::from_instance(self);
        imp.window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
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
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Primary>q"]);
        self.set_accels_for_action("win.show-help-overlay", &["<Primary>question"]);
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
        let chooser = gtk::FileChooserDialogBuilder::new()
            .transient_for(&self.get_main_window())
            .modal(true)
            .action(gtk::FileChooserAction::SelectFolder)
            .title("Select a Folder")
            .build();

        chooser.add_button("Cancel", gtk::ResponseType::Cancel);
        chooser.add_button("Select", gtk::ResponseType::Accept);
        // chooser.connect_response()
        chooser.present()
    }

    fn show_about_dialog(&self) {
        let dialog = gtk::AboutDialogBuilder::new()
            .transient_for(&self.get_main_window())
            .modal(true)
            .program_name("Kooha")
            .version(config::VERSION)
            .logo_icon_name(config::APP_ID)
            .website("https://github.com/SeaDve/Kooha")
            .license_type(gtk::License::Gpl30)
            .copyright("Copyright 2021 Dave Patrick")
            .authors(vec!["SeaDve".into()])
            .build();

        dialog.show();
    }

    pub fn run(&self) {
        info!("Kooha ({})", config::APP_ID);
        info!("Version: {} ({})", config::VERSION, config::PROFILE);
        info!("Datadir: {}", config::PKGDATADIR);

        ApplicationExtManual::run(self);
    }
}
