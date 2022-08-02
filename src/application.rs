use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone, WeakRef},
    prelude::*,
};
use once_cell::unsync::OnceCell;

use std::path::{Path, PathBuf};

use crate::{
    about_window,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    settings::Settings,
    utils,
    window::Window,
};

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub(super) window: OnceCell<WeakRef<Window>>,
        pub(super) settings: Settings,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "KoohaApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self, obj: &Self::Type) {
            self.parent_activate(obj);

            if let Some(window) = obj.main_window() {
                window.present();
            }
        }

        fn startup(&self, obj: &Self::Type) {
            self.parent_startup(obj);

            gtk::Window::set_default_icon_name(APP_ID);

            obj.setup_gactions();
            obj.setup_accels();
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

    pub fn settings(&self) -> Settings {
        self.imp().settings.clone()
    }

    pub fn main_window(&self) -> Option<Window> {
        let main_window = self
            .imp()
            .window
            .get_or_init(|| Window::new(self).downgrade())
            .upgrade();

        if main_window.is_none() {
            tracing::warn!("Failed to upgrade WeakRef<Window>");
        }

        main_window
    }

    pub fn send_record_success_notification(&self, recording_file_path: &Path) {
        let saving_location = recording_file_path
            .parent()
            .expect("Directory doesn't exist.");

        let notification = gio::Notification::new(&gettext("Screencast recorded"));
        notification.set_body(Some(&gettext("Click here to view the video.")));
        notification.set_default_action_and_target_value(
            "app.launch-default-for-path",
            Some(&recording_file_path.to_variant()),
        );
        notification.add_button_with_target_value(
            &gettext("Show in Files"),
            "app.launch-default-for-path",
            Some(&saving_location.to_variant()),
        );

        self.send_notification(Some("record-success"), &notification);
    }

    pub fn run(&self) {
        tracing::info!("Kooha ({})", APP_ID);
        tracing::info!("Version: {} ({})", VERSION, PROFILE);
        tracing::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self);
    }

    fn setup_gactions(&self) {
        let action_launch_default_for_file = gio::SimpleAction::new(
            "launch-default-for-path",
            Some(glib::VariantTy::new("ay").unwrap()),
        );
        action_launch_default_for_file.connect_activate(clone!(@weak self as obj => move |_, param| {
            let file_path = param.unwrap().get::<PathBuf>().unwrap();
            let file_uri = gio::File::for_path(file_path).uri();

            utils::spawn(async move {
                if let Err(err) = gtk::show_uri_full_future(
                    obj.main_window().as_ref(),
                    &file_uri,
                    gdk::CURRENT_TIME,
                )
                .await
                {
                    tracing::warn!("Failed to launch default for uri `{}`: {:?}", file_uri, err);
                }
            });
        }));
        self.add_action(&action_launch_default_for_file);

        let action_select_saving_location = gio::SimpleAction::new("select-saving-location", None);
        action_select_saving_location.connect_activate(clone!(@weak self as obj => move |_, _| {
            utils::spawn(async move {
                obj.settings().select_saving_location(obj.main_window().as_ref()).await;
            });
        }));
        self.add_action(&action_select_saving_location);

        let action_show_about = gio::SimpleAction::new("show-about", None);
        action_show_about.connect_activate(clone!(@weak self as obj => move |_, _| {
            about_window::present(obj.main_window().as_ref());
        }));
        self.add_action(&action_show_about);

        let action_quit = gio::SimpleAction::new("quit", None);
        action_quit.connect_activate(clone!(@weak self as obj => move |_, _| {
            if obj.main_window().map_or(true, |win| win.is_safe_to_close()) {
                obj.quit();
            }
        }));
        self.add_action(&action_quit);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<primary>q"]);
        self.set_accels_for_action("win.record-speaker", &["<primary>a"]);
        self.set_accels_for_action("win.record-mic", &["<primary>m"]);
        self.set_accels_for_action("win.show-pointer", &["<primary>p"]);
        self.set_accels_for_action("win.toggle-record", &["<primary>r"]);
        // self.set_accels_for_action("win.toggle-pause", &["<primary>k"]); // See issue #112 in GitHub repo
        self.set_accels_for_action("win.cancel-delay", &["<primary>c"]);
    }
}

impl Default for Application {
    fn default() -> Self {
        debug_assert!(
            gtk::is_initialized_main_thread(),
            "Application can only be accessed in the main thread"
        );

        gio::Application::default().unwrap().downcast().unwrap()
    }
}
