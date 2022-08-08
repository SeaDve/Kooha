use adw::{prelude::*, subclass::prelude::*};
use anyhow::{ensure, Error, Result};
use futures_util::lock::Mutex;
use gettextrs::gettext;
use gtk::{
    gdk, gio,
    glib::{self, clone},
    CompositeTemplate,
};

use std::time::Duration;

use crate::{
    cancelled::Cancelled,
    config::PROFILE,
    help::Help,
    recording::{Recording, State as RecordingState},
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
                utils::spawn(clone!(@weak obj => async move {
                    if let Err(err) = obj.toggle_pause().await {
                        let err = err.context("Failed to toggle pause");
                        tracing::error!("{:?}", err);
                        obj.present_error(&err);
                    }
                }));
            });

            klass.install_action("win.cancel-record", None, move |obj, _, _| {
                utils::spawn(clone!(@weak obj => async move {
                    obj.cancel_record().await;
                }));
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

    pub async fn close(&self) -> Result<()> {
        let is_safe_to_close =
            self.imp()
                .recording
                .lock()
                .await
                .as_ref()
                .map_or(true, |(ref recording, _)| {
                    matches!(
                        recording.state(),
                        RecordingState::Init
                            | RecordingState::Delayed { .. }
                            | RecordingState::Finished
                    )
                });

        ensure!(
            is_safe_to_close,
            "Cannot close window while recording is in progress"
        );

        GtkWindowExt::close(self);
        Ok(())
    }

    pub fn present_error(&self, err: &Error) {
        let err_text = format!("{:?}", err);

        let err_view = gtk::TextView::builder()
            .buffer(&gtk::TextBuffer::builder().text(&err_text).build())
            .editable(false)
            .monospace(true)
            .top_margin(6)
            .bottom_margin(6)
            .left_margin(6)
            .right_margin(6)
            .build();

        let scrolled_window = gtk::ScrolledWindow::builder()
            .child(&err_view)
            .min_content_height(120)
            .min_content_width(360)
            .build();

        let scrolled_window_row = gtk::ListBoxRow::builder()
            .child(&scrolled_window)
            .overflow(gtk::Overflow::Hidden)
            .activatable(false)
            .selectable(false)
            .build();
        scrolled_window_row.add_css_class("error-view");

        let copy_button = gtk::Button::builder()
            .tooltip_text(&gettext("Copy to clipboard"))
            .icon_name("edit-copy-symbolic")
            .valign(gtk::Align::Center)
            .build();
        copy_button.connect_clicked(move |button| {
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&err_text);
                button.set_tooltip_text(Some(&gettext("Copied to clipboard")));
                button.set_icon_name("checkmark-symbolic");
                button.add_css_class("copy-done");
            } else {
                tracing::error!("Failed to copy error to clipboard: No display");
            }
        });

        let expander = adw::ExpanderRow::builder()
            .title(&gettext("Show detailed error"))
            .activatable(false)
            .build();
        expander.add_row(&scrolled_window_row);
        expander.add_action(&copy_button);

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        list_box.add_css_class("boxed-list");
        list_box.append(&expander);

        let err_dialog = adw::MessageDialog::builder()
            .heading(&err.to_string())
            .body_use_markup(true)
            .default_response("ok")
            .transient_for(self)
            .modal(true)
            .extra_child(&list_box)
            .build();

        if let Some(ref help) = err.downcast_ref::<Help>() {
            err_dialog.set_body(&format!("<b>{}</b>: {}", gettext("Help"), help));
        }

        err_dialog.add_response("ok", &gettext("Ok"));
        err_dialog.present();
    }

    async fn toggle_record(&self) {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.lock().await {
            recording.stop();
            return;
        }

        let recording = Recording::new();
        let handler_ids = vec![
            recording.connect_state_notify(clone!(@weak self as obj => move |_| {
                obj.update_view();
            })),
            recording.connect_duration_notify(clone!(@weak self as obj => move |recording| {
                obj.on_recording_duration_notify(recording);
            })),
            recording.connect_finished(clone!(@weak self as obj => move |recording, res| {
                obj.on_recording_finished(recording, res);
            })),
        ];
        *imp.recording.lock().await = Some((recording.clone(), handler_ids));

        let settings = Application::default().settings();
        let record_delay = settings.record_delay();

        recording
            .start(Duration::from_secs(record_delay as u64))
            .await;
    }

    async fn toggle_pause(&self) -> Result<()> {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.lock().await {
            if matches!(recording.state(), RecordingState::Paused) {
                recording.resume()?;
            } else {
                recording.pause()?;
            };
        }

        Ok(())
    }

    async fn cancel_record(&self) {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.lock().await {
            recording.cancel();
        }
    }

    fn on_recording_finished(&self, recording: &Recording, res: &Result<gio::File>) {
        debug_assert_eq!(recording.state(), RecordingState::Finished);

        match res {
            Ok(ref recording_file) => {
                let application = Application::default();
                application.send_record_success_notification(recording_file);

                let recent_manager = gtk::RecentManager::default();
                recent_manager.add_item(&recording_file.uri());
            }
            Err(ref err) => {
                if err.is::<Cancelled>() {
                    tracing::info!("{:?}", err);
                } else {
                    tracing::error!("{:?}", err);
                    self.present_error(err);
                }
            }
        }

        utils::spawn(clone!(@weak self as obj => async move {
            let recording = obj.imp().recording.lock().await.take();

            if let Some((recording, handler_ids)) = recording {
                for handler_id in handler_ids {
                    recording.disconnect(handler_id);
                }
            } else {
                tracing::warn!("Recording finished but no stored recording");
            }
        }));
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
        utils::spawn(clone!(@weak self as obj => async move {
            obj.update_view_inner().await;
        }));
    }

    async fn update_view_inner(&self) {
        let imp = self.imp();

        // TODO disregard ms granularity recording state change

        let state = imp
            .recording
            .lock()
            .await
            .as_ref()
            .map_or(RecordingState::Init, |(recording, _)| recording.state());

        match state {
            RecordingState::Init | RecordingState::Finished => {
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
            "win.cancel-record",
            matches!(
                state,
                RecordingState::Delayed { .. } | RecordingState::Flushing
            ),
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
