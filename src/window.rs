use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Error, Result};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    CompositeTemplate,
};

use std::cell::RefCell;

use crate::{
    cancelled::Cancelled,
    config::PROFILE,
    help::Help,
    recording::{NoProfileError, Recording, RecordingState},
    settings::CaptureMode,
    toggle_button::ToggleButton,
    Application,
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
        pub(super) main_page: TemplateChild<adw::ToolbarView>,
        #[template_child]
        pub(super) forget_video_sources_revealer: TemplateChild<gtk::Revealer>,
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

        pub(super) recording: RefCell<Option<(Recording, Vec<glib::SignalHandlerId>)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "KoohaWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            ToggleButton::ensure_type();

            klass.bind_template();

            klass.install_action_async("win.toggle-record", None, |obj, _, _| async move {
                obj.toggle_record().await;
            });

            klass.install_action("win.toggle-pause", None, move |obj, _, _| {
                if let Err(err) = obj.toggle_pause() {
                    let err = err.context(gettext("Failed to toggle pause"));
                    tracing::error!("{:?}", err);
                    obj.present_error_dialog(&err);
                }
            });

            klass.install_action("win.cancel-record", None, move |obj, _, _| {
                obj.cancel_record();
            });

            klass.install_action("win.forget-video-sources", None, move |_obj, _, _| {
                Application::get()
                    .settings()
                    .set_screencast_restore_token("");
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

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

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            if obj.is_busy() {
                glib::spawn_future_local(clone!(@weak obj => async move {
                    if obj.run_quit_confirmation_dialog().await.is_proceed() {
                        obj.destroy();
                    }
                }));
                return glib::Propagation::Stop;
            }

            self.parent_close_request()
        }
    }

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
        glib::Object::builder().property("application", app).build()
    }

    /// Returns `true` if the window is busy with a recording.
    pub fn is_busy(&self) -> bool {
        self.imp()
            .recording
            .borrow()
            .as_ref()
            .is_some_and(|(recording, _)| {
                matches!(
                    recording.state(),
                    RecordingState::Recording | RecordingState::Paused | RecordingState::Flushing
                )
            })
    }

    /// Returns `Proceed` if the user wants to proceed with the quit operation.
    pub async fn run_quit_confirmation_dialog(&self) -> glib::Propagation {
        const CANCEL_RESPONSE_ID: &str = "cancel";
        const QUIT_RESPONSE_ID: &str = "quit";

        debug_assert!(
            self.is_busy(),
            "quit confirmation dialog must only be presented when busy"
        );

        let body_text = match self
            .imp()
            .recording
            .borrow()
            .as_ref()
            .map(|(recording, _)| recording.state())
        {
            Some(RecordingState::Recording) | Some(RecordingState::Paused) => gettext(
                "A recording is currently in progress. Quitting immediately will cause the recording to be permanently lost. Please stop the recording before quitting.",
            ),
            Some(RecordingState::Flushing) => gettext(
                "Quitting will cancel the processing and cause the recording to be permanently lost.",
            ),
            state => unreachable!("unexpected recording state: {:?}", state),
        };

        let dialog = adw::MessageDialog::builder()
            .heading(gettext("Discard Recording and Quit?"))
            .body(body_text)
            .close_response(CANCEL_RESPONSE_ID)
            .default_response(CANCEL_RESPONSE_ID)
            .transient_for(self)
            .modal(true)
            .build();
        dialog.add_response(CANCEL_RESPONSE_ID, &gettext("Cancel"));

        dialog.add_response(QUIT_RESPONSE_ID, &gettext("Discard and Quit"));
        dialog.set_response_appearance(QUIT_RESPONSE_ID, adw::ResponseAppearance::Destructive);

        match dialog.choose_future().await.as_str() {
            CANCEL_RESPONSE_ID => glib::Propagation::Stop,
            QUIT_RESPONSE_ID => glib::Propagation::Proceed,
            _ => unreachable!(),
        }
    }

    pub fn present_error_dialog(&self, err: &Error) {
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
            .tooltip_text(gettext("Copy to clipboard"))
            .icon_name("edit-copy-symbolic")
            .valign(gtk::Align::Center)
            .build();
        copy_button.connect_clicked(move |button| {
            button.display().clipboard().set_text(&err_text);
            button.set_tooltip_text(Some(&gettext("Copied to clipboard")));
            button.set_icon_name("checkmark-symbolic");
            button.add_css_class("copy-done");
        });

        let expander = adw::ExpanderRow::builder()
            .title(gettext("Show detailed error"))
            .activatable(false)
            .build();
        expander.add_row(&scrolled_window_row);
        expander.add_suffix(&copy_button);

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        list_box.add_css_class("boxed-list");
        list_box.append(&expander);

        let dialog = adw::MessageDialog::builder()
            .heading(err.to_string())
            .body_use_markup(true)
            .default_response("ok")
            .transient_for(self)
            .modal(true)
            .extra_child(&list_box)
            .build();

        if let Some(ref help) = err.downcast_ref::<Help>() {
            dialog.set_body(&format!("<b>{}</b>: {}", gettext("Help"), help));
        }

        dialog.add_response("ok", &gettext("Ok"));
        dialog.present();
    }

    fn present_no_profile_error_dialog(&self) {
        const OPEN_RESPONSE_ID: &str = "open";
        const LATER_RESPONSE_ID: &str = "later";

        let dialog = adw::MessageDialog::builder()
            .heading(gettext("Open Preferences?"))
            .body(gettext("The previously selected format may have been unavailable. Open preferences and select a format to continue recording."))
            .default_response(OPEN_RESPONSE_ID)
            .transient_for(self)
            .modal(true)
            .build();

        dialog.add_response(LATER_RESPONSE_ID, &gettext("Later"));

        dialog.add_response(OPEN_RESPONSE_ID, &gettext("Open"));
        dialog.set_response_appearance(OPEN_RESPONSE_ID, adw::ResponseAppearance::Suggested);

        dialog.connect_response(Some(OPEN_RESPONSE_ID), |dialog, _| {
            dialog.close();
            Application::get().present_preferences_window();
        });

        dialog.present();
    }

    async fn toggle_record(&self) {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.borrow() {
            recording.stop();
            return;
        }

        let recording = Recording::new();
        let handler_ids = vec![
            recording.connect_state_notify(clone!(@weak self as obj => move |_| {
                obj.update_view();
            })),
            recording.connect_duration_notify(clone!(@weak self as obj => move |recording| {
                let formatted_time = format_time(recording.duration());
                obj.imp().recording_time_label.set_label(&formatted_time);
            })),
            recording.connect_finished(clone!(@weak self as obj => move |recording, res| {
                obj.on_recording_finished(recording, res);
            })),
        ];
        imp.recording
            .replace(Some((recording.clone(), handler_ids)));

        recording
            .start(Some(self), Application::get().settings())
            .await;
    }

    fn toggle_pause(&self) -> Result<()> {
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

    fn cancel_record(&self) {
        let imp = self.imp();

        if let Some((ref recording, _)) = *imp.recording.borrow() {
            recording.cancel();
        }
    }

    fn on_recording_finished(&self, recording: &Recording, res: &Result<gio::File>) {
        debug_assert_eq!(recording.state(), RecordingState::Finished);

        match res {
            Ok(ref recording_file) => {
                let application = Application::get();
                application.send_record_success_notification(recording_file);

                let recent_manager = gtk::RecentManager::default();
                recent_manager.add_item(&recording_file.uri());
            }
            Err(ref err) => {
                if err.is::<Cancelled>() {
                    tracing::debug!("{:?}", err);
                } else if err.is::<NoProfileError>() {
                    self.present_no_profile_error_dialog();
                } else {
                    tracing::error!("{:?}", err);
                    self.surface().beep();
                    self.present_error_dialog(err);
                }
            }
        }

        if let Some((recording, handler_ids)) = self.imp().recording.take() {
            for handler_id in handler_ids {
                recording.disconnect(handler_id);
            }
        } else {
            tracing::warn!("Recording finished but no stored recording");
        }
    }

    fn update_view(&self) {
        let imp = self.imp();

        // TODO disregard ms granularity recording state change

        let state = imp
            .recording
            .borrow()
            .as_ref()
            .map_or(RecordingState::Init, |(recording, _)| recording.state());

        match state {
            RecordingState::Init | RecordingState::Finished => {
                imp.stack.set_visible_child(&*imp.main_page);

                imp.recording_time_label
                    .set_label(&format_time(gst::ClockTime::ZERO));
            }
            RecordingState::Delayed { secs_left } => {
                imp.delay_label.set_label(&secs_left.to_string());

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
            matches!(state, RecordingState::Recording | RecordingState::Paused),
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

        match Application::get().settings().capture_mode() {
            CaptureMode::MonitorWindow => imp.title.set_title(&gettext("Normal")),
            CaptureMode::Selection => imp.title.set_title(&gettext("Selection")),
        }
    }

    fn update_audio_toggles_sensitivity(&self) {
        let is_enabled = Application::get()
            .settings()
            .profile()
            .map_or(true, |profile| profile.supports_audio());

        self.action_set_enabled("win.record-speaker", is_enabled);
        self.action_set_enabled("win.record-mic", is_enabled);
    }

    fn update_forget_video_sources_action(&self) {
        let has_restore_token = !Application::get()
            .settings()
            .screencast_restore_token()
            .is_empty();

        self.imp()
            .forget_video_sources_revealer
            .set_reveal_child(has_restore_token);

        self.action_set_enabled("win.forget-video-sources", has_restore_token);
    }

    fn setup_settings(&self) {
        let app = Application::get();
        let settings = app.settings();

        settings.connect_capture_mode_changed(clone!(@weak self as obj => move |_| {
            obj.update_title_label();
        }));

        settings.connect_profile_changed(clone!(@weak self as obj => move |_| {
            obj.update_audio_toggles_sensitivity();
        }));

        settings.connect_screencast_restore_token_changed(clone!(@weak self as obj => move |_| {
            obj.update_forget_video_sources_action();
        }));

        self.update_title_label();
        self.update_audio_toggles_sensitivity();
        self.update_forget_video_sources_action();

        self.add_action(&settings.create_record_speaker_action());
        self.add_action(&settings.create_record_mic_action());
        self.add_action(&settings.create_show_pointer_action());
        self.add_action(&settings.create_capture_mode_action());
    }
}

/// Format time in MM:SS. The MM part will be more than 2 digits
/// if the time is >= 1 hour.
fn format_time(clock_time: gst::ClockTime) -> String {
    let secs = clock_time.seconds();

    let seconds_display = secs % 60;
    let minutes_display = secs / 60;
    format!("{:02}∶{:02}", minutes_display, seconds_display)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_time_less_than_1_hour() {
        assert_eq!(format_time(gst::ClockTime::ZERO), "00∶00");
        assert_eq!(format_time(gst::ClockTime::from_seconds(31)), "00∶31");
        assert_eq!(
            format_time(gst::ClockTime::from_seconds(8 * 60 + 1)),
            "08∶01"
        );
        assert_eq!(
            format_time(gst::ClockTime::from_seconds(33 * 60 + 3)),
            "33∶03"
        );
        assert_eq!(
            format_time(gst::ClockTime::from_seconds(59 * 60 + 59)),
            "59∶59"
        );
    }

    #[test]
    fn format_time_more_than_1_hour() {
        assert_eq!(format_time(gst::ClockTime::from_seconds(60 * 60)), "60∶00");
        assert_eq!(
            format_time(gst::ClockTime::from_seconds(60 * 60 + 9)),
            "60∶09"
        );
        assert_eq!(
            format_time(gst::ClockTime::from_seconds(60 * 60 + 31)),
            "60∶31"
        );
        assert_eq!(
            format_time(gst::ClockTime::from_seconds(100 * 60 + 20)),
            "100∶20"
        );
    }
}
