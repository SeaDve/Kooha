use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, closure},
};

use crate::{
    profile::{self, BoxedProfile},
    utils,
};

mod imp {
    use super::*;
    use gtk::CompositeTemplate;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/preferences-window.ui")]
    pub struct PreferencesWindow {
        #[template_child]
        pub(super) experimental_indicator_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) experimental_indicator_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) disable_experimental_features_button: TemplateChild<gtk::Button>,
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

            self.disable_experimental_features_button
                .connect_clicked(|_| {
                    let settings = utils::app_settings();
                    settings.reset_video_framerate();
                    settings.reset_profile();
                });

            let settings = utils::app_settings();

            self.frame_rate_row.set_visible(
                utils::is_experimental_mode()
                    || settings.video_framerate() != settings.video_framerate_default_value(),
            );

            self.profile_row
                .set_expression(Some(&gtk::ClosureExpression::new::<
                    String,
                    &[gtk::Expression],
                    _,
                >(
                    &[],
                    closure!(|profile: BoxedProfile| {
                        profile
                            .get()
                            .map_or(gettext("None"), |profile| profile.name())
                    }),
                )));
            let profiles = if utils::is_experimental_mode()
                || settings.profile().map_or(false, |profile| {
                    profile::is_experimental(profile.id()).unwrap()
                }) {
                profile::all()
            } else {
                profile::builtins()
            };
            let profiles_model = gio::ListStore::new(BoxedProfile::static_type());
            profiles_model.append(&BoxedProfile::new_none());
            profiles_model.splice(
                1,
                0,
                &profiles
                    .into_iter()
                    .map(BoxedProfile::new)
                    .collect::<Vec<_>>(),
            );
            let filter = gtk::BoolFilter::new(Some(&gtk::ClosureExpression::new::<
                bool,
                &[gtk::Expression],
                _,
            >(
                &[],
                closure!(|profile: BoxedProfile| {
                    profile.get().map_or(true, |profile| profile.is_available())
                }),
            )));
            let filter_model = gtk::FilterListModel::new(Some(&profiles_model), Some(&filter));
            self.profile_row.set_model(Some(&filter_model));

            settings
                .bind_record_delay(&self.delay_button.get(), "value")
                .build();

            settings
                .bind_video_framerate(&self.frame_rate_button.get(), "value")
                .build();

            settings.connect_video_framerate_changed(clone!(@weak obj => move |_| {
                obj.update_experimental_indicator();
            }));

            settings.connect_saving_location_changed(clone!(@weak obj => move |_| {
                obj.update_file_chooser_button();
            }));

            settings.connect_profile_changed(clone!(@weak obj => move |_| {
                obj.update_profile_row();
                obj.update_experimental_indicator();
            }));

            obj.update_experimental_indicator();
            obj.update_file_chooser_button();
            obj.update_profile_row();

            // Load last active profile first in `update_profile_row` before
            // connecting to the signal to avoid unnecessary updates.
            self.profile_row.connect_selected_item_notify(|row| {
                if let Some(item) = row.selected_item() {
                    let profile = item.downcast::<BoxedProfile>().unwrap();
                    utils::app_settings().set_profile(profile.get());
                }
            });
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

    fn update_experimental_indicator(&self) {
        let settings = utils::app_settings();
        let imp = self.imp();

        let is_experimental_mode = utils::is_experimental_mode();
        let is_using_experimental_features = (settings.video_framerate()
            != settings.video_framerate_default_value())
            || settings.profile().map_or(false, |profile| {
                profile::is_experimental(profile.id()).unwrap()
            });

        imp.disable_experimental_features_button
            .set_visible(!is_experimental_mode && is_using_experimental_features);

        if is_experimental_mode {
            imp.experimental_indicator_row
                .set_title(&gettext("Experimental Mode Enabled"));
            imp.experimental_indicator_group.set_visible(true);
        } else if is_using_experimental_features {
            imp.experimental_indicator_row
                .set_title(&gettext("Using Experimental Features"));
            imp.experimental_indicator_group.set_visible(true);
        } else {
            imp.experimental_indicator_row.set_title("");
            imp.experimental_indicator_group.set_visible(false);
        }
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
                let profile = item.downcast::<BoxedProfile>().unwrap();

                match (profile.get(), &active_profile) {
                    (Some(profile), Some(active_profile)) => profile.id() == active_profile.id(),
                    (None, None) => true,
                    _ => false,
                }
            });

        if let Some(position) = position {
            imp.profile_row.set_selected(position as u32);
        } else {
            tracing::error!(
                "Active profile `{:?}` was not found on profile model",
                active_profile.as_ref().map(|p| p.id())
            );
        }
    }
}

impl Default for PreferencesWindow {
    fn default() -> Self {
        Self::new()
    }
}
