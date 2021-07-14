use adw::subclass::prelude::*;
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
            Self::bind_template(klass);

            KhaToggleButton::static_type();
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
        let win: Self = glib::Object::new(&[]).expect("Failed to create KhaWindow");
        win.set_application(Some(app));

        win.setup_bindings();
        win.setup_signals();
        win.setup_actions();
        win
    }

    fn private(&self) -> &imp::KhaWindow {
        &imp::KhaWindow::from_instance(self)
    }

    fn setup_bindings(&self) {
        let imp = self.private();

        imp.settings
            .bind_property("capture-mode", &*imp.title_stack, "visible-child-name");
    }

    fn setup_signals(&self) {
        let imp = self.private();

        imp.recorder_controller.connect_notify_local(Some("state"), clone!(@weak self as win => move |recorder_controller, _| {
            let win_ = win.private();
            match recorder_controller.property("state").unwrap().get::<RecorderControllerState>().unwrap() {
                RecorderControllerState::Null => win_.main_stack.set_visible_child_name("main-screen"),
                RecorderControllerState::Delayed => win_.main_stack.set_visible_child_name("delay"),
                RecorderControllerState::Playing => {
                    win_.main_stack.set_visible_child_name("recording");
                    win_.pause_record_button.set_icon_name("media-playback-pause-symbolic");
                    win_.recording_label.set_label("Recording");
                    win_.recording_time_label.remove_css_class("paused");
                }
                RecorderControllerState::Paused => {
                    win_.pause_record_button.set_icon_name("media-playback-start-symbolic");
                    win_.recording_label.set_label("Paused");
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

        imp.start_record_button
            .connect_clicked(clone!(@weak self as win => move |_| {
                let win_ = win.private();
                let record_delay = win_.settings.record_delay();

                win_.recorder_controller.start(record_delay);
            }));

        imp.stop_record_button
            .connect_clicked(clone!(@weak self as win => move |_| {
                let win_ = win.private();

                win_.recorder_controller.stop();
            }));

        imp.pause_record_button
            .connect_clicked(clone!(@weak self as win => move |_| {
                let win_ = win.private();

                if win_.recorder_controller.is_paused() {
                    win_.recorder_controller.resume();
                } else {
                    win_.recorder_controller.pause();
                };
            }));

        imp.cancel_delay_button
            .connect_clicked(clone!(@weak self as win => move |_| {
                let win_ = win.private();

                win_.recorder_controller.cancel_delay();
            }));
    }

    fn setup_actions(&self) {
        let imp = self.private();

        let actions = &[
            "record-speaker",
            "record-mic",
            "show-pointer",
            "capture-mode",
            "record-delay",
            "video-format",
        ];

        for action in actions {
            let settings_action = imp.settings.create_action(action);
            self.add_action(&settings_action);
        }
    }
}
