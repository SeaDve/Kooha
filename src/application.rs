use crate::config;
use crate::widgets::KhaWindow;
use gio::ApplicationFlags;
use glib::clone;
use glib::WeakRef;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib};
use gtk_macros::action;
use log::{debug, info};
use once_cell::sync::OnceCell;

mod imp {
    use super::*;

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

            let priv_ = KhaApplication::from_instance(app);
            if let Some(window) = priv_.window.get() {
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
        let priv_ = imp::KhaApplication::from_instance(self);
        priv_.window.get().unwrap().upgrade().unwrap()
    }

    fn setup_gactions(&self) {
        // Quit
        action!(
            self,
            "select-saving-location",
            clone!(@weak self as app => move |_, _| {
                app.select_saving_location();
            })
        );

        // About
        action!(
            self,
            "show-about",
            clone!(@weak self as app => move |_, _| {
                app.show_about_dialog();
            })
        );

        action!(
            self,
            "quit",
            clone!(@weak self as app => move |_, _| {
                // This is needed to trigger the delete event
                // and saving the window state
                app.get_main_window().close();
                app.quit();
            })
        );
    }

    // Sets up keyboard shortcuts
    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Ctrl>q"]);
        self.set_accels_for_action("win.show-help-overlay", &["<Ctrl>question"]);
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
