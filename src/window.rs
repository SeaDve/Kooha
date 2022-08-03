use adw::{prelude::*, subclass::prelude::*};
use error_stack::{Report, Result};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};
use parking_lot::Mutex;

use std::{path::PathBuf, time::Duration};

use crate::{
    config::PROFILE,
    help::Help,
    recording::{Recording, RecordingError, RecordingState},
    settings::{CaptureMode, VideoFormat},
    utils, Application,
};

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) main_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) recording_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) recording_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) recording_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) pause_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) delay_page: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) delay_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) flushing_page: TemplateChild<gtk::Box>,

        pub(super) recording: Mutex<Option<(Recording, Vec<glib::SignalHandlerId>)>>,
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
                    obj.toggle_record().await;
                }));
            });

            klass.install_action("win.toggle-pause", None, move |obj, _, _| {
                if let Err(err) = obj.toggle_pause() {
                    let err = err.attach_printable("Failed to toggle pause");
                    tracing::error!("{:?}", err);
                    obj.present_error(&err);
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

            obj.setup_settings();

            obj.update_view();
            obj.update_audio_toggles_sensitivity();
            obj.update_title_label();
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
            .lock()
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

    pub fn present_error<T>(&self, err: &Report<T>) {
        let err_dialog = adw::MessageDialog::builder()
            .heading(&err.to_string())
            .body_use_markup(true)
            .default_response("ok")
            .transient_for(self)
            .modal(true)
            .build();

        // TODO add widget to show detailed error

        if let Some(ref help) = err.downcast_ref::<Help>() {
            err_dialog.set_body(&format!("<b>{}</b>: {}", gettext("Help"), help));
        }

        err_dialog.add_response("ok", &gettext("Ok"));
        err_dialog.present();
    }

    async fn toggle_record(&self) {
        let imp = self.imp();

        let mut recording: Option<Recording> = None;

        {
            let _lock = imp.recording.lock();

            if let Some((ref tmp, _)) = *_lock {
                recording = Some(tmp.clone());
            }
        }

        if let Some(ref recording) = recording.take() {
            recording.stop().await;
            return;
        }

        let recording = Recording::new();
        let handler_ids = vec![
            recording.connect_state_notify(clone!(@weak self as obj => move |recording| {
                if let RecordingState::Finished(res) = recording.state() {
                    obj.on_recording_finished(&res);
                }

                obj.update_view();
            })),
            recording.connect_duration_notify(clone!(@weak self as obj => move |recording| {
                obj.on_recording_duration_notify(recording);
            })),
        ];
        imp.recording
            .lock()
            .replace((recording.clone(), handler_ids));

        let settings = Application::default().settings();
        let record_delay = settings.record_delay();

        recording
            .start(Duration::from_secs(record_delay as u64))
            .await;
    }

    fn toggle_pause(&self) -> Result<(), RecordingError> {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.lock() {
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

        if let Some((recording, handler_ids)) = imp.recording.lock().take() {
            utils::spawn(async move {
                recording.cancel().await;

                for handler_id in handler_ids {
                    recording.disconnect(handler_id);
                }
            });
        }
    }

    fn on_recording_finished(&self, res: &Result<PathBuf, RecordingError>) {
        let imp = self.imp();

        match res {
            Ok(ref recording_file_path) => {
                let application = Application::default();
                application.send_record_success_notification(recording_file_path);

                let recent_manager = gtk::RecentManager::default();
                recent_manager.add_item(&gio::File::for_path(recording_file_path).uri());
            }
            Err(ref err) => match err.current_context() {
                RecordingError::Cancelled(cancelled) => {
                    tracing::info!("Cancelled: {}", cancelled);
                }
                _ => {
                    tracing::error!("{:?}", err);
                    self.present_error(err);
                }
            },
        }

        if let Some((recording, handler_ids)) = imp.recording.lock().take() {
            for handler_id in handler_ids {
                recording.disconnect(handler_id);
            }
        } else {
            tracing::error!("Recording finished but no stored recording");
        }
    }

    fn on_recording_duration_notify(&self, recording: &Recording) {
        let imp = self.imp();

        let duration_secs = recording.duration().as_secs();

        let seconds_display = duration_secs % 60;
        let minutes_display = (duration_secs / 60) % 60;
        let formatted_time = format!("{:02}∶{:02}", minutes_display, seconds_display);

        imp.recording_time_label.set_label(&formatted_time);
    }

    fn update_view(&self) {
        let imp = self.imp();

        // TODO disregard ms granularity recording state change

        let state = imp
            .recording
            .lock()
            .as_ref()
            .map_or(RecordingState::Null, |(recording, _)| recording.state());

        match state {
            RecordingState::Null | RecordingState::Finished(_) => {
                imp.stack.set_visible_child(&*imp.main_page);
            }
            RecordingState::Delayed { secs_left } => {
                imp.delay_label.set_text(&secs_left.to_string());

                imp.stack.set_visible_child(&*imp.delay_page);
            }
            RecordingState::Recording => {
                imp.pause_record_button
                    .set_icon_name("media-playback-pause-symbolic");
                imp.recording_label.set_label(&gettext("Recording"));
                imp.recording_time_label.remove_css_class("paused");

                imp.stack.set_visible_child(&*imp.recording_page);
            }
            RecordingState::Paused => {
                imp.pause_record_button
                    .set_icon_name("media-playback-start-symbolic");
                imp.recording_label.set_label(&gettext("Paused"));
                imp.recording_time_label.add_css_class("paused");

                imp.stack.set_visible_child(&*imp.recording_page);
            }
            RecordingState::Flushing => imp.stack.set_visible_child(&*imp.flushing_page),
        }

        self.action_set_enabled(
            "win.toggle-record",
            !matches!(
                state,
                RecordingState::Delayed { .. } | RecordingState::Flushing
            ),
        );
        self.action_set_enabled(
            "win.toggle-pause",
            matches!(state, RecordingState::Recording),
        );
        self.action_set_enabled(
            "win.cancel-delay",
            matches!(state, RecordingState::Delayed { .. }),
        );
    }

    fn update_title_label(&self) {
        let imp = self.imp();

        let settings = Application::default().settings();

        match settings.capture_mode() {
            CaptureMode::MonitorWindow => imp.title.set_title(&gettext("Normal")),
            CaptureMode::Selection => imp.title.set_title(&gettext("Selection")),
        }
    }

    fn update_audio_toggles_sensitivity(&self) {
        let settings = Application::default().settings();
        let is_enabled = settings.video_format() != VideoFormat::Gif;

        self.action_set_enabled("win.record-speaker", is_enabled);
        self.action_set_enabled("win.record-mic", is_enabled);
    }

    fn setup_settings(&self) {
        let settings = Application::default().settings();

        settings.connect_capture_mode_changed(clone!(@weak self as obj => move |_| {
            obj.update_title_label();
        }));

        settings.connect_video_format_changed(clone!(@weak self as obj => move |_| {
            obj.update_audio_toggles_sensitivity();
        }));

        self.add_action(&settings.create_record_speaker_action());
        self.add_action(&settings.create_record_mic_action());
        self.add_action(&settings.create_show_pointer_action());
        self.add_action(&settings.create_capture_mode_action());
        self.add_action(&settings.create_record_delay_action());
        self.add_action(&settings.create_video_format_action());
    }
}
