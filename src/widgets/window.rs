use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::application::KhaApplication;
use crate::backend::{KhaRecorderController, RecorderControllerState};
use crate::widgets::KhaToggleButton;

#[derive(Debug, PartialEq)]
enum View {
    MainScreen,
    Recording,
    Delay,
}

mod imp {
    use super::*;

    use gtk::CompositeTemplate;

    use crate::backend::KhaSettings;
    use crate::config::PROFILE;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct KhaWindow {
        pub settings: KhaSettings,
        pub recorder_controller: KhaRecorderController,
        #[template_child]
        pub pause_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub record_speaker_toggle: TemplateChild<KhaToggleButton>,
        #[template_child]
        pub record_mic_toggle: TemplateChild<KhaToggleButton>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub title_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub recording_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub recording_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub delay_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaWindow {
        const NAME: &'static str = "KhaWindow";
        type Type = super::KhaWindow;
        type ParentType = adw::ApplicationWindow;

        fn new() -> Self {
            Self {
                settings: KhaSettings::new(),
                recorder_controller: KhaRecorderController::new(),
                pause_record_button: TemplateChild::default(),
                record_speaker_toggle: TemplateChild::default(),
                record_mic_toggle: TemplateChild::default(),
                main_stack: TemplateChild::default(),
                title_stack: TemplateChild::default(),
                recording_label: TemplateChild::default(),
                recording_time_label: TemplateChild::default(),
                delay_label: TemplateChild::default(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            KhaToggleButton::static_type();
            Self::bind_template(klass);

            klass.install_action("win.toggle-record", None, move |widget, _, _| {
                let imp = imp::KhaWindow::from_instance(widget);

                if imp.recorder_controller.state() == RecorderControllerState::Null {
                    let record_delay = imp.settings.record_delay();
                    imp.recorder_controller.start(record_delay);
                } else {
                    imp.recorder_controller.stop();
                }
            });

            klass.install_action("win.toggle-pause", None, move |widget, _, _| {
                let imp = imp::KhaWindow::from_instance(widget);

                if imp.recorder_controller.state() == RecorderControllerState::Paused {
                    imp.recorder_controller.resume();
                } else {
                    imp.recorder_controller.pause();
                };
            });

            klass.install_action("win.cancel-delay", None, move |widget, _, _| {
                let imp = imp::KhaWindow::from_instance(widget);

                imp.recorder_controller.cancel_delay();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for KhaWindow {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            obj.update_audio_toggles_sensitivity();
            self.settings.connect_changed_notify(
                Some("video-format"),
                clone!(@weak obj => move |_, _| {
                    obj.update_audio_toggles_sensitivity();
                }),
            );

            self.settings
                .bind_property("capture-mode", &*self.title_stack, "visible-child-name");

            let actions = &[
                "record-speaker",
                "record-mic",
                "show-pointer",
                "capture-mode",
                "record-delay",
                "video-format",
            ];

            for action in actions {
                let settings_action = self.settings.create_action(action);
                obj.add_action(&settings_action);
            }

            obj.set_view(View::MainScreen);
            self.recorder_controller.connect_notify_local(
                Some("state"),
                clone!(@weak obj => move |recorder_controller, _| {
                    let imp = obj.private();

                    match recorder_controller.state() {
                        RecorderControllerState::Null => obj.set_view(View::MainScreen),
                        RecorderControllerState::Delayed => obj.set_view(View::Delay),
                        RecorderControllerState::Recording => {
                            obj.set_view(View::Recording);
                            imp.pause_record_button.set_icon_name("media-playback-pause-symbolic");
                            imp.recording_label.set_label(&gettext("Recording"));
                            imp.recording_time_label.remove_css_class("paused");
                        }
                        RecorderControllerState::Paused => {
                            imp.pause_record_button.set_icon_name("media-playback-start-symbolic");
                            imp.recording_label.set_label(&gettext("Paused"));
                            imp.recording_time_label.add_css_class("paused");
                        },
                    };
                }),
            );
            self.recorder_controller.connect_notify_local(
                Some("time"),
                clone!(@weak obj => move |recorder_controller, _| {
                    let imp = obj.private();

                    let current_time = recorder_controller.time();
                    let seconds = current_time % 60;
                    let minutes = (current_time / 60) % 60;
                    let formatted_time = format!("{:02}∶{:02}", minutes, seconds);

                    imp.recording_time_label.set_label(&formatted_time);
                    imp.delay_label.set_label(&current_time.to_string());
                }),
            );
        }
    }

    impl WidgetImpl for KhaWindow {}
    impl WindowImpl for KhaWindow {}
    impl ApplicationWindowImpl for KhaWindow {}
    impl AdwApplicationWindowImpl for KhaWindow {}
}

glib::wrapper! {
    pub struct KhaWindow(ObjectSubclass<imp::KhaWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl KhaWindow {
    pub fn new(app: &KhaApplication) -> Self {
        glib::Object::new(&[("application", app)]).expect("Failed to create KhaWindow")
    }

    fn private(&self) -> &imp::KhaWindow {
        &imp::KhaWindow::from_instance(self)
    }

    fn update_audio_toggles_sensitivity(&self) {
        let imp = self.private();

        let is_enabled = imp.settings.video_format() != "gif";
        imp.record_speaker_toggle.set_action_enabled(is_enabled);
        imp.record_mic_toggle.set_action_enabled(is_enabled);
    }

    fn set_view(&self, view: View) {
        let imp = self.private();

        match view {
            View::MainScreen => imp.main_stack.set_visible_child_name("main-screen"),
            View::Recording => imp.main_stack.set_visible_child_name("recording"),
            View::Delay => imp.main_stack.set_visible_child_name("delay"),
        }

        self.action_set_enabled("win.toggle-record", view != View::Delay);
        self.action_set_enabled("win.toggle-pause", view == View::Recording);
        self.action_set_enabled("win.cancel-delay", view == View::Delay);
    }

    pub fn is_safe_to_quit(&self) -> bool {
        let imp = self.private();
        let allowed_states = [
            RecorderControllerState::Null,
            RecorderControllerState::Delayed,
        ];
        allowed_states.contains(&imp.recorder_controller.state())
    }
}
