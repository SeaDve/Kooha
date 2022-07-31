use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
    CompositeTemplate,
};

use std::string::ToString;

use crate::{
    backend::{RecorderController, RecorderControllerState, RecorderResponse},
    config::PROFILE,
    Application,
};

#[derive(Debug, PartialEq, strum_macros::Display)]
#[strum(serialize_all = "snake_case")]
enum View {
    Main,
    Recording,
    Delay,
    Flushing,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct Window {
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

        pub recorder_controller: RecorderController,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "KoohaWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("win.toggle-record", None, move |obj, _, _| {
                obj.on_toggle_record();
            });

            klass.install_action("win.toggle-pause", None, move |obj, _, _| {
                obj.on_toggle_pause();
            });

            klass.install_action("win.cancel-delay", None, move |obj, _, _| {
                obj.on_cancel_delay();
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

            obj.set_view(&View::Main);
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
        let allowed_states = [
            RecorderControllerState::Null,
            RecorderControllerState::Delayed,
        ];
        allowed_states.contains(&self.imp().recorder_controller.state())
    }

    fn set_view(&self, view: &View) {
        self.imp()
            .main_stack
            .set_visible_child_name(view.to_string().as_ref());

        self.action_set_enabled("win.toggle-record", *view != View::Delay);
        self.action_set_enabled("win.toggle-pause", *view == View::Recording);
        self.action_set_enabled("win.cancel-delay", *view == View::Delay);
    }

    fn update_audio_toggles_sensitivity(&self) {
        let settings = Application::default().settings();
        let is_enabled = settings.video_format() != "gif";

        self.action_set_enabled("win.record-speaker", is_enabled);
        self.action_set_enabled("win.record-mic", is_enabled);
    }

    fn on_toggle_record(&self) {
        let imp = self.imp();

        if imp.recorder_controller.state() == RecorderControllerState::Null {
            let settings = Application::default().settings();
            let record_delay = settings.record_delay();
            imp.recorder_controller.start(record_delay);
        } else {
            imp.recorder_controller.stop();
        }
    }

    fn on_toggle_pause(&self) {
        let imp = self.imp();

        if imp.recorder_controller.state() == RecorderControllerState::Paused {
            imp.recorder_controller.resume();
        } else {
            imp.recorder_controller.pause();
        };
    }

    fn on_cancel_delay(&self) {
        self.imp().recorder_controller.cancel_delay();
    }

    fn on_recorder_controller_state_notify(&self, recorder_controller: &RecorderController) {
        let imp = self.imp();

        match recorder_controller.state() {
            RecorderControllerState::Null => self.set_view(&View::Main),
            RecorderControllerState::Flushing => self.set_view(&View::Flushing),
            RecorderControllerState::Delayed => self.set_view(&View::Delay),
            RecorderControllerState::Recording => {
                self.set_view(&View::Recording);
                imp.pause_record_button
                    .set_icon_name("media-playback-pause-symbolic");
                imp.recording_label.set_label(&gettext("Recording"));
                imp.recording_time_label.remove_css_class("paused");
            }
            RecorderControllerState::Paused => {
                imp.pause_record_button
                    .set_icon_name("media-playback-start-symbolic");
                imp.recording_label.set_label(&gettext("Paused"));
                imp.recording_time_label.add_css_class("paused");
            }
        };
    }

    fn on_recorder_controller_time_notify(&self, recorder_controller: &RecorderController) {
        let imp = self.imp();

        let current_time = recorder_controller.time();
        let seconds = current_time % 60;
        let minutes = (current_time / 60) % 60;
        let formatted_time = format!("{:02}∶{:02}", minutes, seconds);

        imp.recording_time_label.set_label(&formatted_time);
        imp.delay_label.set_label(&current_time.to_string());
    }

    fn on_recorder_controller_response(&self, _: &RecorderController, response: &RecorderResponse) {
        match response {
            RecorderResponse::Success(recording_file_path) => {
                let application: Application = self.application().unwrap().downcast().unwrap();
                application.send_record_success_notification(recording_file_path);

                let recent_manager = gtk::RecentManager::default();
                recent_manager.add_item(&gio::File::for_path(recording_file_path).uri());
            }
            RecorderResponse::Failed(error) => {
                let error_dialog = gtk::MessageDialog::builder()
                    .text(&error.to_string())
                    .secondary_text(&error.help())
                    .secondary_use_markup(true)
                    .buttons(gtk::ButtonsType::Ok)
                    .message_type(gtk::MessageType::Error)
                    .transient_for(self)
                    .modal(true)
                    .build();
                error_dialog.connect_response(|error_dialog, _| error_dialog.destroy());
                error_dialog.present();
            }
        };
    }

    fn setup_signals(&self) {
        let imp = self.imp();

        let settings = Application::default().settings();

        settings.bind_key("capture-mode", &*imp.title_stack, "visible-child-name");

        settings.connect_changed_notify(
            Some("video-format"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_audio_toggles_sensitivity();
            }),
        );

        imp.recorder_controller.connect_state_notify(
            clone!(@weak self as obj => move |recorder_controller| {
                obj.on_recorder_controller_state_notify(recorder_controller);
            }),
        );

        imp.recorder_controller.connect_time_notify(
            clone!(@weak self as obj => move |recorder_controller| {
                obj.on_recorder_controller_time_notify(recorder_controller);
            }),
        );

        imp.recorder_controller.connect_response(
            clone!(@weak self as obj => move |recorder_controller, response| {
                obj.on_recorder_controller_response(recorder_controller, response);
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn view_to_string() {
        assert_eq!(View::Main.to_string(), "main");
        assert_eq!(View::Recording.to_string(), "recording");
        assert_eq!(View::Delay.to_string(), "delay");
        assert_eq!(View::Flushing.to_string(), "flushing");
    }
}
