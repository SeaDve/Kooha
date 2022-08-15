use adw::subclass::prelude::*;
use anyhow::{Context, Result};
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone, WeakRef},
    prelude::*,
};
use once_cell::unsync::OnceCell;

use crate::{
    about,
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

    pub fn send_record_success_notification(&self, recording_file: &gio::File) {
        // Translators: This is a message that the user will see when the recording is finished.
        let notification = gio::Notification::new(&gettext("Screencast recorded"));
        notification.set_body(Some(&gettext("Click here to view the video.")));
        notification.set_default_action_and_target_value(
            "app.launch-default-for-uri",
            Some(&recording_file.uri().to_variant()),
        );
        notification.add_button_with_target_value(
            &gettext("Show in Files"),
            "app.show-in-files",
            Some(&recording_file.uri().to_variant()),
        );

        self.send_notification(Some("record-success"), &notification);
    }

    pub fn run(&self) {
        tracing::info!("Kooha ({})", APP_ID);
        tracing::info!("Version: {} ({})", VERSION, PROFILE);
        tracing::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self);
    }

    async fn try_show_uri(&self, uri: &str) {
        if let Err(err) =
            gtk::show_uri_full_future(self.main_window().as_ref(), uri, gdk::CURRENT_TIME).await
        {
            tracing::warn!("Failed to launch default for uri `{}`: {:?}", uri, err);

            if let Some(window) = self.main_window() {
                window.present_error(&err.into());
            }
        }
    }

    fn setup_gactions(&self) {
        let action_launch_default_for_uri =
            gio::SimpleAction::new("launch-default-for-uri", Some(glib::VariantTy::STRING));
        action_launch_default_for_uri.connect_activate(
            clone!(@weak self as obj => move |_, param| {
                let file_uri = param.unwrap().get::<String>().unwrap();

                utils::spawn(async move {
                    obj.try_show_uri(&file_uri).await;
                });
            }),
        );
        self.add_action(&action_launch_default_for_uri);

        let action_show_in_files =
            gio::SimpleAction::new("show-in-files", Some(glib::VariantTy::STRING));
        action_show_in_files.connect_activate(clone!(@weak self as obj => move |_, param| {
            let uri = param.unwrap().get::<String>().unwrap();

            utils::spawn(async move {
                if let Err(err) = show_items(&[&uri], "").await {
                    tracing::warn!("Failed to show items: {:?}", err);

                    obj.try_show_uri(&uri).await;
                }
            });
        }));
        self.add_action(&action_show_in_files);

        let action_select_saving_location = gio::SimpleAction::new("select-saving-location", None);
        action_select_saving_location.connect_activate(clone!(@weak self as obj => move |_, _| {
            obj.settings().select_saving_location(obj.main_window().as_ref());
        }));
        self.add_action(&action_select_saving_location);

        let action_show_about = gio::SimpleAction::new("show-about", None);
        action_show_about.connect_activate(clone!(@weak self as obj => move |_, _| {
            about::present_window(obj.main_window().as_ref());
        }));
        self.add_action(&action_show_about);

        let action_quit = gio::SimpleAction::new("quit", None);
        action_quit.connect_activate(clone!(@weak self as obj => move |_, _| {
            if let Some(window) = obj.main_window() {
                utils::spawn(async move {
                    if let Err(err) = window.close().await {
                        tracing::warn!("Failed to close window: {:?}", err);
                    }
                });
            } else {
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
        self.set_accels_for_action("win.cancel-record", &["<primary>c"]);
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

async fn show_items(uris: &[&str], startup_id: &str) -> Result<()> {
    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .context("Failed to get session bus")?;

    connection
        .call_future(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&(uris, startup_id).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
        )
        .await
        .with_context(|| {
            format!(
                "Failed to invoke org.freedesktop.FileManager1.ShowItems with uris: {:?}",
                &uris
            )
        })?;

    Ok(())
}
