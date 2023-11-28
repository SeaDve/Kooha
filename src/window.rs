use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
};

use crate::{
    application::Application,
    cancelled::Cancelled,
    config::PROFILE,
    pipeline::{CropData, Pipeline, RecordingState},
    screencast_session::{CursorMode, PersistMode, ScreencastSession, SourceType},
    timer::Timer,
    toggle_button::ToggleButton,
    utils,
    view_port::{Selection, ViewPort},
};

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub(super) record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) view_port: TemplateChild<ViewPort>,
        #[template_child]
        pub(super) selection_toggle: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub(super) desktop_audio_level_left: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) desktop_audio_level_right: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) microphone_level_left: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) microphone_level_right: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) recording_indicator: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) recording_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) info_label: TemplateChild<gtk::Label>,

        pub(super) pipeline: Pipeline,
        pub(super) timer: RefCell<Option<Timer>>,
        pub(super) session: RefCell<Option<ScreencastSession>>,
        pub(super) prev_selection: Cell<Option<Selection>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "KoohaWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            ToggleButton::ensure_type();

            klass.bind_template();

            klass.install_action_async("win.select-video-source", None, |obj, _, _| async move {
                if let Err(err) = obj.replace_session(None).await {
                    if err.is::<Cancelled>() {
                        tracing::debug!("Select video source cancelled: {:?}", err);
                    } else {
                        tracing::error!("Failed to select video source: {:?}", err);
                    }
                }
            });

            klass.install_action_async("win.toggle-record", None, |obj, _, _| async move {
                if let Err(err) = obj.toggle_record().await {
                    if err.is::<Cancelled>() {
                        tracing::debug!("Recording cancelled: {:?}", err);
                    } else {
                        tracing::error!("Failed to toggle record: {:?}", err);
                    }
                }
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

            self.selection_toggle
                .connect_active_notify(clone!(@weak obj => move |toggle| {
                    let imp = obj.imp();
                    if toggle.is_active() {
                        let selection = obj.imp().prev_selection.get().unwrap_or_else(|| {
                            let mid_x = imp.view_port.width() as f32 / 2.0;
                            let mid_y = imp.view_port.height() as f32 / 2.0;
                            let offset = 20.0 * imp.view_port.scale_factor() as f32;
                            Selection::new(
                                mid_x - offset,
                                mid_y - offset,
                                mid_x + offset,
                                mid_y + offset,
                            )
                        });
                        imp.view_port.set_selection(Some(selection));
                    } else {
                        imp.view_port.set_selection(None::<Selection>);
                    }
                }));
            self.view_port
                .connect_paintable_notify(clone!(@weak obj => move |_| {
                    obj.update_selection_toggle_sensitivity();
                    obj.update_info_label();
                }));
            self.view_port
                .connect_selection_notify(clone!(@weak obj => move |view_port| {
                    if let Some(selection) = view_port.selection() {
                        obj.imp().prev_selection.replace(Some(selection));
                    }
                    obj.update_selection_toggle();
                    obj.update_info_label();
                }));

            self.pipeline
                .connect_stream_size_notify(clone!(@weak obj => move |_| {
                    obj.update_info_label();
                }));
            self.pipeline
                .connect_recording_state_notify(clone!(@weak obj => move |_| {
                    obj.update_recording_ui();
                }));
            self.pipeline
                .connect_desktop_audio_peak(clone!(@weak obj => move |_, peaks| {
                    let imp = obj.imp();
                    imp.desktop_audio_level_left.set_value(peaks.left());
                    imp.desktop_audio_level_right.set_value(peaks.right());
                }));
            self.pipeline
                .connect_microphone_peak(clone!(@weak obj => move |_, peaks| {
                    let imp = obj.imp();
                    imp.microphone_level_left.set_value(peaks.left());
                    imp.microphone_level_right.set_value(peaks.right());
                }));
            self.view_port
                .set_paintable(Some(self.pipeline.paintable()));

            obj.load_window_size();

            glib::spawn_future_local(clone!(@weak obj => async move {
                if let Err(err) = obj.load_session().await {
                    tracing::error!("Failed to load session: {:?}", err);
                }
            }));

            obj.update_selection_toggle_sensitivity();
            obj.update_selection_toggle();
            obj.update_info_label();
            obj.update_recording_ui();
            obj.update_desktop_audio_pipeline();
            obj.update_microphone_pipeline();
        }

        fn dispose(&self) {
            let obj = self.obj();

            obj.close_session();

            self.dispose_template();
        }
    }

    impl WidgetImpl for Window {}

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            if let Err(err) = obj.save_window_size() {
                tracing::warn!("Failed to save window state, {:?}", &err);
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
    pub fn new(application: &Application) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn start_recording(&self) -> Result<()> {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        let crop_data = imp.view_port.selection().map(|selection| CropData {
            full_rect: imp.view_port.paintable_rect().unwrap(),
            selection_rect: selection.rect(),
        });
        imp.pipeline
            .start_recording(&settings.saving_location(), crop_data)?;

        Ok(())
    }

    fn stop_recording(&self) -> Result<()> {
        let imp = self.imp();

        imp.pipeline.stop_recording()?;

        Ok(())
    }

    async fn toggle_record(&self) -> Result<()> {
        let imp = self.imp();

        match imp.pipeline.recording_state() {
            RecordingState::Idle => {
                if let Some(timer) = imp.timer.take() {
                    timer.cancel();
                    self.update_recording_ui();
                    return Ok(());
                }

                let app = utils::app_instance();
                let settings = app.settings();

                let timer = Timer::new(settings.record_delay(), |secs_left| {
                    println!("secs_left: {}", secs_left);
                });
                imp.timer.replace(Some(timer.clone()));
                self.update_recording_ui();

                timer.await?;

                let _ = imp.timer.take();
                self.update_recording_ui();

                self.start_recording()
                    .context("Failed to start recording")?;
            }
            RecordingState::Started { .. } => {
                self.stop_recording().context("Failed to stop recording")?;
            }
        }

        Ok(())
    }

    fn close_session(&self) {
        let imp = self.imp();

        if let Some(session) = imp.session.take() {
            glib::spawn_future_local(async move {
                if let Err(err) = session.close().await {
                    tracing::error!("Failed to end ScreencastSession: {:?}", err);
                }
            });
        }
    }

    async fn replace_session(&self, restore_token: Option<&str>) -> Result<()> {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        let session = ScreencastSession::new()
            .await
            .context("Failed to create ScreencastSession")?;

        tracing::debug!(
            version = ?session.version(),
            available_cursor_modes = ?session.available_cursor_modes(),
            available_source_types = ?session.available_source_types(),
            "Screencast session created"
        );

        let (streams, restore_token, fd) = session
            .begin(
                if settings.show_pointer() {
                    CursorMode::EMBEDDED
                } else {
                    CursorMode::HIDDEN
                },
                if utils::is_experimental_mode() {
                    SourceType::MONITOR | SourceType::WINDOW
                } else {
                    SourceType::MONITOR
                },
                true,
                restore_token,
                PersistMode::ExplicitlyRevoked,
                Some(self),
            )
            .await
            .context("Failed to begin ScreencastSession")?;
        settings.set_screencast_restore_token(&restore_token.unwrap_or_default());

        self.close_session();

        imp.pipeline.set_streams(&streams, fd)?;
        imp.session.replace(Some(session));

        Ok(())
    }

    async fn load_session(&self) -> Result<()> {
        let app = utils::app_instance();
        let settings = app.settings();

        let restore_token = settings.screencast_restore_token();
        settings.set_screencast_restore_token("");

        self.replace_session(Some(&restore_token)).await?;

        Ok(())
    }

    fn load_window_size(&self) {
        let app = utils::app_instance();
        let settings = app.settings();

        self.set_default_size(settings.window_width(), settings.window_height());

        if settings.window_maximized() {
            self.maximize();
        }
    }

    fn save_window_size(&self) -> Result<()> {
        let app = utils::app_instance();
        let settings = app.settings();

        let (width, height) = self.default_size();

        settings.try_set_window_width(width)?;
        settings.try_set_window_height(height)?;

        settings.try_set_window_maximized(self.is_maximized())?;

        Ok(())
    }

    fn update_desktop_audio_pipeline(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        if settings.record_speaker() {
            glib::spawn_future_local(clone!(@weak self as obj => async move {
                if let Err(err) = obj.imp().pipeline.load_desktop_audio().await {
                    tracing::error!("Failed to load desktop audio: {:?}", err);
                }
            }));
        } else {
            if let Err(err) = imp.pipeline.unload_desktop_audio() {
                tracing::error!("Failed to unload desktop audio: {:?}", err);
            }

            imp.desktop_audio_level_left.set_value(0.0);
            imp.desktop_audio_level_right.set_value(0.0);
        }
    }

    fn update_microphone_pipeline(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        if settings.record_mic() {
            glib::spawn_future_local(clone!(@weak self as obj => async move {
                if let Err(err) = obj.imp().pipeline.load_microphone().await {
                    tracing::error!("Failed to load microphone: {:?}", err);
                }
            }));
        } else {
            if let Err(err) = imp.pipeline.unload_microphone() {
                tracing::error!("Failed to unload microphone: {:?}", err);
            }

            imp.microphone_level_left.set_value(0.0);
            imp.microphone_level_right.set_value(0.0);
        }
    }

    fn update_selection_toggle_sensitivity(&self) {
        let imp = self.imp();

        imp.selection_toggle
            .set_sensitive(imp.view_port.paintable().is_some());
    }

    fn update_selection_toggle(&self) {
        let imp = self.imp();

        imp.selection_toggle
            .set_active(imp.view_port.selection().is_some());
    }

    fn update_info_label(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        let mut info_list = vec![
            settings
                .profile()
                .map_or_else(|| gettext("No Profile"), |profile| profile.name()),
            format!("{} FPS", settings.video_framerate()),
        ];

        match (imp.pipeline.stream_size(), imp.view_port.selection()) {
            (Some(stream_size), Some(selection)) => {
                let paintable_rect = imp.view_port.paintable_rect().unwrap();
                let scale_factor_h = stream_size.width() as f32 / paintable_rect.width();
                let scale_factor_v = stream_size.height() as f32 / paintable_rect.height();
                let selection_rect_scaled = selection.rect().scale(scale_factor_h, scale_factor_v);
                info_list.push(format!(
                    "{}×{}",
                    selection_rect_scaled.width().round() as i32,
                    selection_rect_scaled.height().round() as i32,
                ));
            }
            (Some(stream_size), None) => {
                info_list.push(format!("{}×{}", stream_size.width(), stream_size.height()));
            }
            _ => {}
        }

        imp.info_label.set_label(&info_list.join(" • "));
    }

    fn update_recording_ui(&self) {
        let imp = self.imp();

        match imp.pipeline.recording_state() {
            RecordingState::Idle => {
                if imp.timer.borrow().is_some() {
                    imp.record_button.set_label(&gettext("Cancel"));

                    imp.record_button.remove_css_class("suggested-action");
                    imp.record_button.add_css_class("destructive-action");
                } else {
                    imp.record_button.set_label(&gettext("Record"));

                    imp.record_button.remove_css_class("destructive-action");
                    imp.record_button.add_css_class("suggested-action");
                }

                imp.recording_indicator.remove_css_class("red");
                imp.recording_indicator.add_css_class("dim-label");

                imp.recording_time_label.set_label("00∶00∶00");
            }
            RecordingState::Started { duration } => {
                imp.record_button.set_label(&gettext("Stop"));

                imp.record_button.remove_css_class("suggested-action");
                imp.record_button.add_css_class("destructive-action");

                imp.recording_indicator.remove_css_class("dim-label");
                imp.recording_indicator.add_css_class("red");

                let secs = duration.seconds();
                let hours_display = secs / 3600;
                let minutes_display = (secs / 60) % 60;
                let seconds_display = secs % 60;
                imp.recording_time_label.set_label(&format!(
                    "{:02}∶{:02}∶{:02}",
                    hours_display, minutes_display, seconds_display
                ));
            }
        }
    }

    fn update_desktop_audio_level_sensitivity(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        let is_record_desktop_audio = settings.record_speaker();
        imp.desktop_audio_level_left
            .set_sensitive(is_record_desktop_audio);
        imp.desktop_audio_level_right
            .set_sensitive(is_record_desktop_audio);
    }

    fn update_microphone_level_sensitivity(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        let is_record_microphone = settings.record_mic();
        imp.microphone_level_left
            .set_sensitive(is_record_microphone);
        imp.microphone_level_right
            .set_sensitive(is_record_microphone);
    }

    fn setup_settings(&self) {
        let app = utils::app_instance();
        let settings = app.settings();

        self.add_action(&settings.create_record_speaker_action());
        self.add_action(&settings.create_record_mic_action());
        self.add_action(&settings.create_show_pointer_action());

        settings.connect_record_speaker_changed(clone!(@weak self as obj => move |_| {
            obj.update_desktop_audio_level_sensitivity();
            obj.update_desktop_audio_pipeline();
        }));
        settings.connect_record_mic_changed(clone!(@weak self as obj => move |_| {
            obj.update_microphone_level_sensitivity();
            obj.update_microphone_pipeline();
        }));

        settings.connect_video_framerate_changed(clone!(@weak self as obj => move |_| {
            obj.update_info_label();
        }));
        settings.connect_profile_changed(clone!(@weak self as obj => move |_| {
            obj.update_info_label();
        }));

        self.update_desktop_audio_level_sensitivity();
        self.update_microphone_level_sensitivity();
    }
}
