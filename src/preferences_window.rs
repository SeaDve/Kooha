use adw::{prelude::*, subclass::prelude::*};
use gtk::{
    gio,
    glib::{self, clone, closure, BoxedAnyObject},
};

use crate::{
    profile::{self, Profile},
    utils,
};

mod imp {
    use super::*;
    use gtk::CompositeTemplate;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/preferences-window.ui")]
    pub struct PreferencesWindow {
        #[template_child]
        pub(super) frame_rate_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) frame_rate_button: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub(super) profile_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) delay_button: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub(super) file_chooser_button_content: TemplateChild<adw::ButtonContent>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesWindow {
        const NAME: &'static str = "KoohaPreferencesWindow";
        type Type = super::PreferencesWindow;
        type ParentType = adw::PreferencesWindow;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("preferences.select-saving-location", None, |obj, _, _| {
                utils::app_settings().select_saving_location(Some(obj));
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesWindow {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.frame_rate_row
                .set_visible(utils::is_experimental_mode());

            self.profile_row
                .set_expression(Some(&gtk::ClosureExpression::new::<
                    String,
                    &[gtk::Expression],
                    _,
                >(
                    &[],
                    closure!(|obj: BoxedAnyObject| {
                        let profile = obj.borrow::<Box<dyn Profile>>();
                        profile.name()
                    }),
                )));
            let profiles_model = gio::ListStore::new(BoxedAnyObject::static_type());
            profiles_model.splice(
                0,
                0,
                &profile::get_all()
                    .into_iter()
                    .map(BoxedAnyObject::new)
                    .collect::<Vec<_>>(),
            );
            self.profile_row.set_model(Some(&profiles_model));
            self.profile_row.connect_selected_item_notify(|row| {
                if let Some(item) = row.selected_item() {
                    let obj = item.downcast::<BoxedAnyObject>().unwrap();
                    let profile = obj.borrow::<Box<dyn Profile>>();
                    utils::app_settings().set_profile(&**profile);
                }
            });

            let settings = utils::app_settings();

            settings
                .bind_record_delay(&self.delay_button.get(), "value")
                .build();

            settings
                .bind_video_framerate(&self.frame_rate_button.get(), "value")
                .build();

            settings.connect_saving_location_changed(clone!(@weak obj => move |_| {
                obj.update_file_chooser_button();
            }));

            settings.connect_profile_changed(clone!(@weak obj => move |_| {
                obj.update_profile_row();
            }));

            obj.update_file_chooser_button();
            obj.update_profile_row();
        }
    }

    impl WidgetImpl for PreferencesWindow {}
    impl WindowImpl for PreferencesWindow {}
    impl AdwWindowImpl for PreferencesWindow {}
    impl PreferencesWindowImpl for PreferencesWindow {}
}

glib::wrapper! {
     pub struct PreferencesWindow(ObjectSubclass<imp::PreferencesWindow>)
        @extends gtk::Widget, gtk::Window, adw::Window, adw::PreferencesWindow;
}

impl PreferencesWindow {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create PreferencesWindow.")
    }

    fn update_file_chooser_button(&self) {
        let saving_location_display = utils::app_settings()
            .saving_location()
            .display()
            .to_string();

        if let Some(stripped) =
            saving_location_display.strip_prefix(&glib::home_dir().display().to_string())
        {
            self.imp()
                .file_chooser_button_content
                .set_label(&format!("~{}", stripped));
        } else {
            self.imp()
                .file_chooser_button_content
                .set_label(&saving_location_display);
        }
    }

    fn update_profile_row(&self) {
        let active_profile = utils::app_settings().profile();

        let imp = self.imp();
        let position = imp
            .profile_row
            .model()
            .unwrap()
            .into_iter()
            .position(|item| {
                let obj = item.downcast::<BoxedAnyObject>().unwrap();
                let profile = obj.borrow::<Box<dyn Profile>>();
                profile.typetag_name() == active_profile.typetag_name()
            });

        if let Some(position) = position {
            imp.profile_row.set_selected(position as u32);
        } else {
            tracing::error!(
                "Active profile `{}` was not found on profile model",
                active_profile.typetag_name()
            );
        }
    }
}

impl Default for PreferencesWindow {
    fn default() -> Self {
        Self::new()
    }
}
