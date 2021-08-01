use adw::subclass::prelude::*;
use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};

use crate::{
    application::Application,
    backend::{RecorderController, RecorderControllerState, RecorderResponse, Settings},
    config::PROFILE,
    i18n::i18n,
};

#[derive(Debug, PartialEq)]
enum View {
    MainScreen,
    Recording,
    Delay,
}

mod imp {
    use super::*;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct MainWindow {
        pub settings: Settings,
        pub recorder_controller: RecorderController,
        #[template_child]
        pub start_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub pause_record_button: TemplateChild<gtk::Button>,
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
    impl ObjectSubclass for MainWindow {
        const NAME: &'static str = "MainWindow";
        type Type = super::MainWindow;
        type ParentType = adw::ApplicationWindow;

        fn new() -> Self {
            Self {
                settings: Settings::new(),
                recorder_controller: RecorderController::new(),
                start_record_button: TemplateChild::default(),
                pause_record_button: TemplateChild::default(),
                main_stack: TemplateChild::default(),
                title_stack: TemplateChild::default(),
                recording_label: TemplateChild::default(),
                recording_time_label: TemplateChild::default(),
                delay_label: TemplateChild::default(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("win.toggle-record", None, move |obj, _, _| {
                let imp = obj.private();

                if imp.recorder_controller.state() == RecorderControllerState::Null {
                    let record_delay = imp.settings.record_delay();
                    imp.recorder_controller.start(record_delay);
                } else {
                    imp.recorder_controller.stop();
                }
            });

            klass.install_action("win.toggle-pause", None, move |obj, _, _| {
                let imp = obj.private();

                if imp.recorder_controller.state() == RecorderControllerState::Paused {
                    imp.recorder_controller.resume();
                } else {
                    imp.recorder_controller.pause();
                };
            });

            klass.install_action("win.cancel-delay", None, move |obj, _, _| {
                let imp = obj.private();

                imp.recorder_controller.cancel_delay();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MainWindow {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            if PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

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

            self.settings
                .bind_property("capture-mode", &*self.title_stack, "visible-child-name");

            self.recorder_controller.set_window(obj);

            obj.update_audio_toggles_sensitivity();
            self.settings.connect_changed_notify(
                Some("video-format"),
                clone!(@weak obj => move |_, _| {
                    obj.update_audio_toggles_sensitivity();
                }),
            );

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
                            imp.recording_label.set_label(&i18n("Recording"));
                            imp.recording_time_label.remove_css_class("paused");
                        }
                        RecorderControllerState::Paused => {
                            imp.pause_record_button.set_icon_name("media-playback-start-symbolic");
                            imp.recording_label.set_label(&i18n("Paused"));
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
            self.recorder_controller
                .connect_local(
                    "response",
                    false,
                    clone!(@weak obj => @default-return None, move |args| {
                        let response = args[1].get().unwrap();
                        match response {
                            RecorderResponse::Success(recording_file_path) => {
                                let application: Application = obj.application().unwrap().downcast().unwrap();
                                application.send_record_success_notification(&recording_file_path);
                            },
                            RecorderResponse::Failed(error_message) => {
                                let error_dialog = gtk::MessageDialogBuilder::new()
                                    .modal(true)
                                    .buttons(gtk::ButtonsType::Ok)
                                    .transient_for(&obj)
                                    .title(&i18n("Sorry! An error has occurred."))
                                    .text(&error_message)
                                    .build();
                                error_dialog.connect_response(|error_dialog, _| error_dialog.destroy());
                                error_dialog.present();
                            }
                        };
                        None
                    }),
                )
                .unwrap();
        }
    }

    impl WidgetImpl for MainWindow {}
    impl WindowImpl for MainWindow {}
    impl ApplicationWindowImpl for MainWindow {}
    impl AdwApplicationWindowImpl for MainWindow {}
}

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl MainWindow {
    pub fn new(app: &Application) -> Self {
        glib::Object::new(&[("application", app)]).expect("Failed to create MainWindow")
    }

    fn private(&self) -> &imp::MainWindow {
        &imp::MainWindow::from_instance(self)
    }

    fn update_audio_toggles_sensitivity(&self) {
        let imp = self.private();

        let is_enabled = imp.settings.video_format() != "gif";

        self.action_set_enabled("win.record-speaker", is_enabled);
        self.action_set_enabled("win.record-mic", is_enabled);
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
