use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{
    about,
    config::{APP_ID, PKGDATADIR, PROFILE, VERSION},
    format_time,
    preferences_dialog::PreferencesDialog,
    settings::Settings,
    window::Window,
};

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub(super) settings: OnceCell<Settings>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "KoohaApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            self.parent_activate();

            let obj = self.obj();

            if let Some(window) = obj.windows().first() {
                window.present();
                return;
            }

            let window = Window::new(&obj);
            window.present();
        }

        fn startup(&self) {
            self.parent_startup();

            gtk::Window::set_default_icon_name(APP_ID);

            let obj = self.obj();

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
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("resource-base-path", "/io/github/seadve/Kooha/")
            .build()
    }

    /// Returns the global instance of `Application`.
    ///
    /// # Panics
    ///
    /// Panics if the app is not running or if this is called on a non-main thread.
    pub fn get() -> Self {
        debug_assert!(
            gtk::is_initialized_main_thread(),
            "application must only be accessed in the main thread"
        );

        gio::Application::default().unwrap().downcast().unwrap()
    }

    pub fn settings(&self) -> &Settings {
        self.imp().settings.get_or_init(|| {
            let settings = Settings::default();

            if tracing::enabled!(tracing::Level::TRACE) {
                settings.connect_changed(None, |settings, key| {
                    tracing::trace!("Settings `{}` changed to `{}`", key, settings.value(key));
                });
            }

            settings
        })
    }

    pub fn window(&self) -> Window {
        self.active_window()
            .map_or_else(|| Window::new(self), |w| w.downcast().unwrap())
    }

    pub async fn send_record_success_notification(
        &self,
        recording_file: &gio::File,
        duration: gst::ClockTime,
    ) {
        let mut body_fragments = vec![format_time::duration(duration)];

        match recording_file
            .query_info_future(
                gio::FILE_ATTRIBUTE_STANDARD_SIZE,
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT_IDLE,
            )
            .await
        {
            Ok(file_info) => {
                let formatted_size = glib::format_size(file_info.size() as u64);
                body_fragments.push(formatted_size.to_string());
            }
            Err(err) => tracing::warn!("Failed to get file size: {:?}", err),
        }

        // Translators: This is a message that the user will see when the recording is finished.
        let notification = gio::Notification::new(&gettext("Screencast recorded"));
        notification.set_body(Some(&body_fragments.join(", ")));
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

    pub fn present_preferences_dialog(&self) {
        let dialog = PreferencesDialog::new(self.settings());
        dialog.present(&self.window());
    }

    pub fn run(&self) -> glib::ExitCode {
        tracing::info!("Kooha ({})", APP_ID);
        tracing::info!("Version: {} ({})", VERSION, PROFILE);
        tracing::info!("Datadir: {}", PKGDATADIR);

        ApplicationExtManual::run(self)
    }

    pub fn quit(&self) {
        glib::spawn_future_local(clone!(@weak self as obj => async move {
            if obj.quit_request().await.is_proceed() {
                ApplicationExt::quit(&obj);
            }
        }));
    }

    async fn quit_request(&self) -> glib::Propagation {
        if let Some(window) = self.active_window() {
            let window = window.downcast::<Window>().unwrap();

            if window.is_busy() {
                return window.run_quit_confirmation_dialog().await;
            }
        }

        glib::Propagation::Proceed
    }

    async fn try_show_uri(&self, uri: &str) {
        let window = self.window();
        if let Err(err) = gtk::FileLauncher::new(Some(&gio::File::for_uri(uri)))
            .launch_future(Some(&window))
            .await
        {
            if !err.matches(gio::IOErrorEnum::Cancelled) {
                tracing::error!("Failed to launch default for uri `{}`: {:?}", uri, err);
                window.present_error_dialog(&err.into());
            }
        }
    }

    fn setup_gactions(&self) {
        let action_launch_default_for_uri = gio::SimpleAction::new(
            "launch-default-for-uri",
            Some(&String::static_variant_type()),
        );
        action_launch_default_for_uri.connect_activate(
            clone!(@weak self as obj => move |_, param| {
                let file_uri = param.unwrap().get::<String>().unwrap();

                glib::spawn_future_local(async move {
                    obj.try_show_uri(&file_uri).await;
                });
            }),
        );
        self.add_action(&action_launch_default_for_uri);

        let action_show_in_files =
            gio::SimpleAction::new("show-in-files", Some(&String::static_variant_type()));
        action_show_in_files.connect_activate(clone!(@weak self as obj => move |_, param| {
            let uri = param.unwrap().get::<String>().unwrap();

            glib::spawn_future_local(async move {
                if let Err(err) = gtk::FileLauncher::new(Some(&gio::File::for_uri(&uri)))
                    .open_containing_folder_future(Some(&obj.window()))
                    .await
                {
                    tracing::warn!("Failed to show items: {:?}", err);

                    obj.try_show_uri(&uri).await;
                }
            });
        }));
        self.add_action(&action_show_in_files);

        let quit_action = gio::ActionEntry::builder("quit")
            .activate(|obj: &Self, _, _| {
                obj.quit();
            })
            .build();
        let show_preferences_action = gio::ActionEntry::builder("show-preferences")
            .activate(|obj: &Self, _, _| {
                obj.present_preferences_dialog();
            })
            .build();
        let show_about_action = gio::ActionEntry::builder("show-about")
            .activate(|obj: &Self, _, _| {
                about::present_dialog(&obj.window());
            })
            .build();
        self.add_action_entries([quit_action, show_preferences_action, show_about_action]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.show-preferences", &["<Control>comma"]);
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.record-speaker", &["<Control>a"]);
        self.set_accels_for_action("win.record-mic", &["<Control>m"]);
        self.set_accels_for_action("win.show-pointer", &["<Control>p"]);
        self.set_accels_for_action("win.toggle-record", &["<Control>r"]);
        // self.set_accels_for_action("win.toggle-pause", &["<Control>k"]); // See issue #112 in GitHub repo
        self.set_accels_for_action("win.cancel-record", &["<Control>c"]);
    }
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}
