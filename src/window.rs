use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};

use std::{cell::RefCell, string::ToString, time::Duration};

use crate::{
    config::PROFILE,
    recording::{Recording, RecordingError, RecordingState},
    settings::VideoFormat,
    utils, Application,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Main,
    Recording,
    Delay,
    Flushing,
}

impl View {
    fn to_ui_file_id(self) -> &'static str {
        match self {
            View::Main => "main",
            View::Recording => "recording",
            View::Delay => "delay",
            View::Flushing => "flushing",
        }
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) pause_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) title_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) recording_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) recording_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) delay_label: TemplateChild<gtk::Label>,

        pub(super) recording: RefCell<Option<(Recording, Vec<glib::SignalHandlerId>)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "KoohaWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("win.toggle-record", None, move |obj, _, _| {
                utils::spawn(clone!(@weak obj => async move {
                    if let Err(err) = obj.toggle_record().await {
                        tracing::error!("Failed to toggle record: {:?}", err);
                    }
                }));
            });

            klass.install_action("win.toggle-pause", None, move |obj, _, _| {
                if let Err(err) = obj.toggle_pause() {
                    tracing::error!("Failed to toggle pause: {:?}", err);
                }
            });

            klass.install_action("win.cancel-delay", None, move |obj, _, _| {
                obj.cancel_delay();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            obj.setup_gactions();
            obj.setup_signals();

            obj.set_view(View::Main);
            obj.update_audio_toggles_sensitivity();
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup, gtk::Native;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        glib::Object::new(&[("application", app)]).expect("Failed to create Window.")
    }

    pub fn is_safe_to_close(&self) -> bool {
        self.imp()
            .recording
            .borrow()
            .as_ref()
            .map_or(true, |(ref recording, _)| {
                matches!(
                    recording.state(),
                    RecordingState::Null
                        | RecordingState::Delayed { .. }
                        | RecordingState::Finished(..)
                )
            })
    }

    fn set_view(&self, view: View) {
        self.imp()
            .main_stack
            .set_visible_child_name(view.to_ui_file_id());

        self.action_set_enabled("win.toggle-record", view != View::Delay);
        self.action_set_enabled("win.toggle-pause", view == View::Recording);
        self.action_set_enabled("win.cancel-delay", view == View::Delay);
    }

    fn update_audio_toggles_sensitivity(&self) {
        let settings = Application::default().settings();
        let is_enabled = settings.video_format() != VideoFormat::Gif;

        self.action_set_enabled("win.record-speaker", is_enabled);
        self.action_set_enabled("win.record-mic", is_enabled);
    }

    async fn toggle_record(&self) -> anyhow::Result<()> {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.borrow() {
            recording.stop()?;
            return Ok(());
        }

        let recording = Recording::new();
        let handler_ids = vec![
            recording.connect_state_notify(clone!(@weak self as obj => move |recording| {
                obj.on_recording_state_notify(recording);
            })),
            recording.connect_duration_notify(clone!(@weak self as obj => move |recording| {
                obj.on_recording_duration_notify(recording);
            })),
        ];
        imp.recording
            .replace(Some((recording.clone(), handler_ids)));

        let settings = Application::default().settings();
        let record_delay = settings.record_delay();

        if let Err(err) = recording
            .start(Duration::from_secs(record_delay as u64))
            .await
        {
            imp.recording.replace(None);
            return Err(err);
        }

        Ok(())
    }

    fn toggle_pause(&self) -> anyhow::Result<()> {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.borrow() {
            if matches!(recording.state(), RecordingState::Paused) {
                recording.resume()?;
            } else {
                recording.pause()?;
            };
        }

        Ok(())
    }

    fn cancel_delay(&self) {
        let imp = self.imp();

        if let Some((recording, handler_ids)) = imp.recording.take() {
            utils::spawn(async move {
                recording.cancel().await;

                for handler_id in handler_ids {
                    recording.disconnect(handler_id);
                }
            });
        }
    }

    fn on_recording_state_notify(&self, recorder_controller: &Recording) {
        let imp = self.imp();

        match recorder_controller.state() {
            RecordingState::Null => self.set_view(View::Main),
            RecordingState::Flushing => self.set_view(View::Flushing),
            RecordingState::Delayed { secs_left } => {
                imp.delay_label.set_text(&secs_left.to_string());
                self.set_view(View::Delay);
            }
            RecordingState::Recording => {
                self.set_view(View::Recording);
                imp.pause_record_button
                    .set_icon_name("media-playback-pause-symbolic");
                imp.recording_label.set_label(&gettext("Recording"));
                imp.recording_time_label.remove_css_class("paused");
            }
            RecordingState::Paused => {
                imp.pause_record_button
                    .set_icon_name("media-playback-start-symbolic");
                imp.recording_label.set_label(&gettext("Paused"));
                imp.recording_time_label.add_css_class("paused");
            }
            RecordingState::Finished(res) => {
                self.set_view(View::Main);

                match res {
                    Ok(recording_file_path) => {
                        let application: Application =
                            self.application().unwrap().downcast().unwrap();
                        application.send_record_success_notification(&recording_file_path);

                        let recent_manager = gtk::RecentManager::default();
                        recent_manager.add_item(&gio::File::for_path(recording_file_path).uri());
                    }
                    Err(err) => {
                        match err {
                            RecordingError::Cancelled(cancelled) => {
                                tracing::info!("Cancelled: {}", cancelled);
                            }
                            RecordingError::Gstreamer(_) => {
                                let err_dialog = adw::MessageDialog::builder()
                                    .heading(&err.to_string())
                                    // .body(&error.help()) // TODO improve err handling
                                    .body_use_markup(true)
                                    .default_response("ok")
                                    .transient_for(self)
                                    .modal(true)
                                    .build();
                                err_dialog.add_response("ok", &gettext("Ok"));
                                err_dialog.present();
                            }
                        }
                    }
                }

                if let Some((recording, handler_ids)) = imp.recording.take() {
                    for handler_id in handler_ids {
                        recording.disconnect(handler_id);
                    }
                }
            }
        };
    }

    fn on_recording_duration_notify(&self, recording: &Recording) {
        let imp = self.imp();

        let duration_secs = recording.duration().as_secs();

        let seconds_display = duration_secs % 60;
        let minutes_display = (duration_secs / 60) % 60;
        let formatted_time = format!("{:02}∶{:02}", minutes_display, seconds_display);

        imp.recording_time_label.set_label(&formatted_time);
    }

    fn setup_signals(&self) {
        let imp = self.imp();

        let settings = Application::default().settings();

        settings
            .bind("capture-mode", &*imp.title_stack, "visible-child-name")
            .build();

        settings.connect_changed(
            Some("video-format"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_audio_toggles_sensitivity();
            }),
        );
    }

    fn setup_gactions(&self) {
        let actions = [
            "record-speaker",
            "record-mic",
            "show-pointer",
            "capture-mode",
            "record-delay",
            "video-format",
        ];

        let settings = Application::default().settings();

        for action in actions {
            let settings_action = settings.create_action(action);
            self.add_action(&settings_action);
        }
    }
}
