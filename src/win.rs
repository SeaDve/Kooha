use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use gettextrs::gettext;
use gst::prelude::*;
use gtk::{
    gdk, gio,
    glib::{self, clone},
};

use crate::{
    application::Application,
    area_selector::{Selection, ViewPort},
    audio_device::{self, Class as AudioDeviceClass},
    config::PROFILE,
    pipeline,
    screencast_session::{CursorMode, PersistMode, ScreencastSession, SourceType},
    toggle_button::ToggleButton,
    utils,
};

const PREVIEW_FPS: u32 = 60;

mod imp {
    use std::cell::{Cell, RefCell};

    use gst::bus::BusWatchGuard;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/win.ui")]
    pub struct Win {
        #[template_child]
        pub(super) view_port: TemplateChild<ViewPort>,
        #[template_child]
        pub(super) selection_toggle: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub(super) desktop_audio_level: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) microphone_level: TemplateChild<gtk::LevelBar>,
        #[template_child]
        pub(super) info_label: TemplateChild<gtk::Label>,

        pub(super) session: RefCell<Option<(ScreencastSession, gst::Pipeline, BusWatchGuard)>>,
        pub(super) stream_size: Cell<Option<(i32, i32)>>,

        pub(super) previous_selection: Cell<Option<Selection>>,

        pub(super) desktop_audio_pipeline: RefCell<Option<(gst::Pipeline, BusWatchGuard)>>,
        pub(super) microphone_pipeline: RefCell<Option<(gst::Pipeline, BusWatchGuard)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Win {
        const NAME: &'static str = "KoohaWin";
        type Type = super::Win;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            ToggleButton::ensure_type();

            klass.bind_template();

            klass.install_action_async("win.select-video-source", None, |obj, _, _| async move {
                if let Err(err) = obj.replace_session(None).await {
                    tracing::error!("Failed to replace session: {:?}", err);
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Win {
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
                        let selection = obj.imp().previous_selection.get().unwrap_or_else(|| {
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
                        imp.view_port.set_selection(None);
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
                        obj.imp().previous_selection.replace(Some(selection));
                    }
                    obj.update_selection_toggle();
                    obj.update_info_label();
                }));

            obj.load_window_size();

            glib::spawn_future_local(clone!(@weak obj => async move {
                if let Err(err) = obj.load_session().await {
                    tracing::error!("Failed to load session: {:?}", err);
                }
            }));

            obj.update_selection_toggle_sensitivity();
            obj.update_selection_toggle();
            obj.update_info_label();
            obj.update_desktop_audio_pipeline();
            obj.update_microphone_pipeline();
        }

        fn dispose(&self) {
            if let Some((_, pipeline, _)) = self.session.take() {
                let _ = pipeline.set_state(gst::State::Null);
            }

            if let Some((pipeline, _)) = self.desktop_audio_pipeline.take() {
                let _ = pipeline.set_state(gst::State::Null);
            }

            if let Some((pipeline, _)) = self.microphone_pipeline.take() {
                let _ = pipeline.set_state(gst::State::Null);
            }

            self.dispose_template();
        }
    }

    impl WidgetImpl for Win {}

    impl WindowImpl for Win {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            if let Err(err) = obj.save_window_size() {
                tracing::warn!("Failed to save window state, {:?}", &err);
            }

            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for Win {}
    impl AdwApplicationWindowImpl for Win {}
}

glib::wrapper! {
    pub struct Win(ObjectSubclass<imp::Win>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup, gtk::Native;
}

impl Win {
    pub fn new(application: &Application) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
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

        let pipeline = gst::Pipeline::new();
        let videosrc_bin = pipeline::pipewiresrc_bin(fd, &streams, PREVIEW_FPS, None)?;
        let videoconvert = gst::ElementFactory::make("videoconvert")
            .name("sink-videoconvert")
            .build()?;
        let sink = gst::ElementFactory::make("gtk4paintablesink").build()?;
        pipeline.add_many([videosrc_bin.upcast_ref(), &videoconvert, &sink])?;
        gst::Element::link_many([videosrc_bin.upcast_ref(), &videoconvert, &sink])?;

        let bus_watch_guard = pipeline.bus().unwrap().add_watch_local(
            clone!(@weak self as obj => @default-panic, move |_, message| {
                obj.handle_video_bus_message(message)
            }),
        )?;

        imp.stream_size.set(None);
        self.update_info_label();

        if let Some((_, pipeline, _)) = imp.session.take() {
            let _ = pipeline.set_state(gst::State::Null);
        }

        imp.session
            .replace(Some((session, pipeline, bus_watch_guard)));

        imp.session
            .borrow()
            .as_ref()
            .map(|(_, pipeline, _)| pipeline)
            .unwrap()
            .set_state(gst::State::Playing)?;

        let paintable = sink.property::<gdk::Paintable>("paintable");
        imp.view_port.set_paintable(Some(paintable));

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

    async fn load_desktop_audio(&self) -> Result<()> {
        let imp = self.imp();

        let device_name = audio_device::find_default_name(AudioDeviceClass::Sink)
            .await
            .context("No desktop audio source found")?;

        let pulsesrc = gst::ElementFactory::make("pulsesrc")
            .property("device", device_name)
            .build()?;
        let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
        let level = gst::ElementFactory::make("level")
            .property("interval", gst::ClockTime::from_mseconds(80))
            .property("peak-ttl", gst::ClockTime::from_mseconds(80))
            .build()?;
        let fakesink = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()?;

        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&pulsesrc, &audioconvert, &level, &fakesink])?;
        gst::Element::link_many([&pulsesrc, &audioconvert, &level, &fakesink])?;

        let bus = pipeline.bus().unwrap();
        let bus_watch_guard = bus.add_watch_local(
            clone!(@weak self as obj => @default-panic, move |_, message| {
                handle_audio_bus_message(message, |peak| {
                    obj.imp().desktop_audio_level.set_value(peak);
                })
            }),
        )?;

        imp.desktop_audio_pipeline
            .replace(Some((pipeline, bus_watch_guard)));

        imp.desktop_audio_pipeline
            .borrow()
            .as_ref()
            .map(|(pipeline, _)| pipeline)
            .unwrap()
            .set_state(gst::State::Playing)?;

        Ok(())
    }

    async fn load_microphone(&self) -> Result<()> {
        let imp = self.imp();

        let device_name = audio_device::find_default_name(AudioDeviceClass::Source)
            .await
            .context("No microphone source found")?;

        let pulsesrc = gst::ElementFactory::make("pulsesrc")
            .property("device", device_name)
            .build()?;
        let audioconvert = gst::ElementFactory::make("audioconvert").build()?;
        let level = gst::ElementFactory::make("level")
            .property("interval", gst::ClockTime::from_mseconds(80))
            .property("peak-ttl", gst::ClockTime::from_mseconds(80))
            .build()?;
        let fakesink = gst::ElementFactory::make("fakesink")
            .property("sync", true)
            .build()?;

        let pipeline = gst::Pipeline::new();
        pipeline.add_many([&pulsesrc, &audioconvert, &level, &fakesink])?;
        gst::Element::link_many([&pulsesrc, &audioconvert, &level, &fakesink])?;

        let bus = pipeline.bus().unwrap();
        let bus_watch_guard = bus.add_watch_local(
            clone!(@weak self as obj => @default-panic, move |_, message| {
                handle_audio_bus_message(message, |peak| {
                    obj.imp().microphone_level.set_value(peak);
                })
            }),
        )?;

        imp.microphone_pipeline
            .replace(Some((pipeline, bus_watch_guard)));

        imp.microphone_pipeline
            .borrow()
            .as_ref()
            .map(|(pipeline, _)| pipeline)
            .unwrap()
            .set_state(gst::State::Playing)?;

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

    fn handle_video_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        let imp = self.imp();

        match message.view() {
            gst::MessageView::AsyncDone(_) => {
                if imp.stream_size.get().is_some() {
                    return glib::ControlFlow::Continue;
                }

                let videoconvert = imp
                    .session
                    .borrow()
                    .as_ref()
                    .map(|(_, pipeline, _)| pipeline)
                    .unwrap()
                    .by_name("sink-videoconvert")
                    .unwrap();
                let caps = videoconvert
                    .static_pad("src")
                    .unwrap()
                    .current_caps()
                    .unwrap();
                let caps_struct = caps.structure(0).unwrap();
                let stream_width = caps_struct.get::<i32>("width").unwrap();
                let stream_height = caps_struct.get::<i32>("height").unwrap();
                imp.stream_size.set(Some((stream_width, stream_height)));
                self.update_info_label();

                glib::ControlFlow::Continue
            }
            gst::MessageView::Error(e) => {
                tracing::error!(src = ?e.src(), error = ?e.error(), debug = ?e.debug(), "Error from video bus");

                glib::ControlFlow::Break
            }
            _ => {
                tracing::trace!(?message, "Message from video bus");

                glib::ControlFlow::Continue
            }
        }
    }

    fn update_desktop_audio_pipeline(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        if settings.record_speaker() && imp.desktop_audio_pipeline.borrow().is_none() {
            glib::spawn_future_local(clone!(@weak self as obj => async move {
                if let Err(err) = obj.load_desktop_audio().await {
                    tracing::error!("Failed to load desktop audio: {:?}", err);
                }
            }));
        } else if let Some((pipeline, _)) = imp.desktop_audio_pipeline.take() {
            let _ = pipeline.set_state(gst::State::Null);
            imp.desktop_audio_level.set_value(0.0);
        }
    }

    fn update_microphone_pipeline(&self) {
        let imp = self.imp();

        let app = utils::app_instance();
        let settings = app.settings();

        if settings.record_mic() && imp.microphone_pipeline.borrow().is_none() {
            glib::spawn_future_local(clone!(@weak self as obj => async move {
                if let Err(err) = obj.load_microphone().await {
                    tracing::error!("Failed to load microphone: {:?}", err);
                }
            }));
        } else if let Some((pipeline, _)) = imp.microphone_pipeline.take() {
            let _ = pipeline.set_state(gst::State::Null);
            imp.microphone_level.set_value(0.0);
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

        match (imp.stream_size.get(), imp.view_port.selection()) {
            (Some((stream_width, stream_height)), Some(selection)) => {
                let paintable_rect = imp.view_port.paintable_rect().unwrap();
                let scale_factor_h = stream_width as f32 / paintable_rect.width();
                let scale_factor_v = stream_height as f32 / paintable_rect.height();
                let selection_rect_scaled = selection.rect().scale(scale_factor_h, scale_factor_v);
                info_list.push(format!(
                    "{}×{}",
                    selection_rect_scaled.width().round() as i32,
                    selection_rect_scaled.height().round() as i32,
                ));
            }
            (Some((stream_width, stream_height)), None) => {
                info_list.push(format!("{}×{}", stream_width, stream_height));
            }
            _ => {}
        }

        imp.info_label.set_label(&info_list.join(" • "));
    }

    fn update_desktop_audio_level_sensitivity(&self) {
        let app = utils::app_instance();
        let settings = app.settings();

        self.imp()
            .desktop_audio_level
            .set_sensitive(settings.record_speaker());
    }

    fn update_microphone_level_sensitivity(&self) {
        let app = utils::app_instance();
        let settings = app.settings();

        self.imp()
            .microphone_level
            .set_sensitive(settings.record_mic());
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

fn handle_audio_bus_message(message: &gst::Message, callback: impl Fn(f64)) -> glib::ControlFlow {
    match message.view() {
        gst::MessageView::Element(e) => {
            if let Some(structure) = e.structure() {
                if structure.has_name("level") {
                    let peaks = structure.get::<&glib::ValueArray>("rms").unwrap();
                    let left_peak = peaks.nth(0).unwrap().get::<f64>().unwrap();
                    let right_peak = peaks.nth(1).unwrap().get::<f64>().unwrap();
                    let max_peak = left_peak.max(right_peak);
                    let normalized_max_peak = 10_f64.powf(max_peak / 20.0);
                    callback(normalized_max_peak);
                }
            }

            glib::ControlFlow::Continue
        }
        gst::MessageView::Error(e) => {
            tracing::error!(src = ?e.src(), error = ?e.error(), debug = ?e.debug(), "Error from audio bus");

            glib::ControlFlow::Break
        }
        _ => {
            tracing::trace!(?message, "Message from audio bus");

            glib::ControlFlow::Continue
        }
    }
}
