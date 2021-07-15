use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};

use crate::application::KhaApplication;
use crate::backend::RecorderControllerState;
use crate::widgets::KhaToggleButton;

mod imp {
    use super::*;

    use gtk::CompositeTemplate;

    use crate::backend::{KhaRecorderController, KhaSettings};
    use crate::config::PROFILE;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct KhaWindow {
        pub settings: KhaSettings,
        pub recorder_controller: KhaRecorderController,
        #[template_child]
        pub start_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub stop_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub pause_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub cancel_delay_button: TemplateChild<gtk::Button>,
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
                start_record_button: TemplateChild::default(),
                stop_record_button: TemplateChild::default(),
                pause_record_button: TemplateChild::default(),
                cancel_delay_button: TemplateChild::default(),
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

                if imp.recorder_controller.is_recording() {
                    imp.recorder_controller.stop();
                } else {
                    let record_delay = imp.settings.record_delay();
                    imp.recorder_controller.start(record_delay);
                }
            });

            klass.install_action("win.toggle-pause", None, move |widget, _, _| {
                let imp = imp::KhaWindow::from_instance(widget);

                if imp.recorder_controller.is_paused() {
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

            obj.setup_signals();

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

    fn setup_signals(&self) {
        let imp = self.private();

        imp.recorder_controller.connect_notify_local(Some("state"), clone!(@weak self as win => move |recorder_controller, _| {
            let win_ = win.private();
            // FIXME disable action where necessary
            match recorder_controller.property("state").unwrap().get::<RecorderControllerState>().unwrap() {
                RecorderControllerState::Null => win_.main_stack.set_visible_child_name("main-screen"),
                RecorderControllerState::Delayed => win_.main_stack.set_visible_child_name("delay"),
                RecorderControllerState::Playing => {
                    win_.main_stack.set_visible_child_name("recording");
                    win_.pause_record_button.set_icon_name("media-playback-pause-symbolic");
                    win_.recording_label.set_label(&gettext("Recording"));
                    win_.recording_time_label.remove_css_class("paused");
                }
                RecorderControllerState::Paused => {
                    win_.pause_record_button.set_icon_name("media-playback-start-symbolic");
                    win_.recording_label.set_label(&gettext("Paused"));
                    win_.recording_time_label.add_css_class("paused");
                },
            };
        }));

        imp.recorder_controller.connect_notify_local(Some("time"), clone!(@weak self as win => move |recorder_controller, _| {
            let win_ = win.private();

            let current_time = recorder_controller.property("time").unwrap().get::<u32>().unwrap();
            let seconds = current_time % 60;
            let minutes = (current_time / 60) % 60;
            let formatted_time = format!("{:02}∶{:02}", minutes, seconds);

            win_.recording_time_label.set_label(&formatted_time);
            win_.delay_label.set_label(&current_time.to_string());
        }));
    }
}
