use crate::application::KhaApplication;
use crate::backend::KhaRecorder;
use crate::backend::KhaScreencastPortal;
use crate::backend::KhaSettings;
use crate::config::{APP_ID, PROFILE};

use adw::subclass::prelude::*;
use glib::clone;
use gtk::subclass::prelude::*;
use gtk::{self, prelude::*};
use gtk::{gio, glib, CompositeTemplate};

mod imp {
    use super::*;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/window.ui")]
    pub struct KhaWindow {
        pub settings: KhaSettings,
        pub recorder: KhaRecorder,
        #[template_child]
        pub start_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub stop_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub pause_record_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub cancel_delay_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub title_stack: TemplateChild<gtk::Stack>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaWindow {
        const NAME: &'static str = "KhaWindow";
        type Type = super::KhaWindow;
        type ParentType = adw::ApplicationWindow;

        fn new() -> Self {
            Self {
                settings: KhaSettings::new(),
                recorder: KhaRecorder::new(),
                start_record_button: TemplateChild::default(),
                stop_record_button: TemplateChild::default(),
                pause_record_button: TemplateChild::default(),
                cancel_delay_button: TemplateChild::default(),
                title_stack: TemplateChild::default(),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for KhaWindow {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            let builder = gtk::Builder::from_resource("/io/github/seadve/Kooha/ui/help_overlay.ui");
            let help_overlay = builder.object("help_overlay").unwrap();
            obj.set_help_overlay(Some(&help_overlay));

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
        let window: Self = glib::Object::new(&[]).expect("Failed to create KhaWindow");
        window.set_application(Some(app));

        window.setup_bindings();
        window.setup_signals();
        window.setup_actions();

        gtk::Window::set_default_icon_name(APP_ID);

        window
    }

    fn get_private(&self) -> &imp::KhaWindow {
        &imp::KhaWindow::from_instance(self)
    }

    fn setup_bindings(&self) {
        let imp = self.get_private();

        imp.settings
            .bind_property("capture-mode", &*imp.title_stack, "visible-child-name")
    }

    fn setup_signals(&self) {
        let imp = self.get_private();

        imp.start_record_button
            .connect_clicked(clone!(@weak self as win => move |_| {
                let win_ = win.get_private();
                win_.recorder.start();

                let portal = KhaScreencastPortal::new();
                portal.open();

            }));
    }

    fn setup_actions(&self) {
        let imp = self.get_private();

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
