use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, closure, BoxedAnyObject},
    pango,
};

use crate::{profile::Profile, settings::Settings, IS_EXPERIMENTAL_MODE};

/// Used to represent "none" profile in the profiles model
type NoneProfile = BoxedAnyObject;

mod imp {
    use std::cell::OnceCell;

    use super::*;
    use gtk::CompositeTemplate;

    #[derive(Debug, Default, glib::Properties, CompositeTemplate)]
    #[properties(wrapper_type = super::PreferencesDialog)]
    #[template(resource = "/io/github/seadve/Kooha/ui/preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[property(get, set, construct_only)]
        pub(super) settings: OnceCell<Settings>,

        #[template_child]
        pub(super) framerate_button: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub(super) framerate_warning: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) profile_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) delay_button: TemplateChild<gtk::SpinButton>,
        #[template_child]
        pub(super) file_chooser_button_content: TemplateChild<adw::ButtonContent>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "KoohaPreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action_async(
                "preferences.select-saving-location",
                None,
                |obj, _, _| async move {
                    if let Err(err) = obj
                        .settings()
                        .select_saving_location(
                            obj.root()
                                .map(|r| r.downcast::<gtk::Window>().unwrap())
                                .as_ref(),
                        )
                        .await
                    {
                        tracing::error!("Failed to select saving location: {:?}", err);

                        let toast = adw::Toast::new(&gettext("Failed to set recordings folder"));
                        obj.add_toast(toast);
                    }
                },
            );
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for PreferencesDialog {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let settings = obj.settings();
            let active_profile = settings.profile();

            self.profile_row
                .set_factory(Some(&profile_row_factory(&self.profile_row, false)));
            self.profile_row
                .set_list_factory(Some(&profile_row_factory(&self.profile_row, true)));
            let profiles = Profile::all()
                .inspect_err(|err| tracing::error!("Failed to load profiles: {:?}", err))
                .unwrap_or_default();
            let profiles_model = gio::ListStore::new::<glib::Object>();
            if active_profile.is_none() {
                profiles_model.append(&NoneProfile::new(()));
            }
            profiles_model.splice(profiles_model.n_items(), 0, profiles);

            let is_using_experimental =
                *IS_EXPERIMENTAL_MODE || active_profile.is_some_and(|p| p.is_experimental());
            let filter = gtk::BoolFilter::new(Some(&gtk::ClosureExpression::new::<bool>(
                &[] as &[&gtk::Expression],
                closure!(|obj: glib::Object| {
                    profile_from_obj(&obj).map_or(true, |profile| {
                        profile.is_available()
                            && (!profile.is_experimental() || is_using_experimental)
                    })
                }),
            )));
            let filter_model = gtk::FilterListModel::new(Some(profiles_model), Some(filter));

            self.profile_row.set_model(Some(&filter_model));

            settings
                .bind_record_delay(&self.delay_button.get(), "value")
                .build();

            settings
                .bind_video_framerate(&self.framerate_button.get(), "value")
                .build();

            settings.connect_video_framerate_changed(clone!(@weak obj => move |_| {
                obj.update_framerate_warning();
            }));

            settings.connect_saving_location_changed(clone!(@weak obj => move |_| {
                obj.update_file_chooser_button();
            }));

            settings.connect_profile_changed(clone!(@weak obj => move |_| {
                obj.update_profile_row();
                obj.update_framerate_warning();
            }));

            obj.update_file_chooser_button();
            obj.update_framerate_warning();
            obj.update_profile_row();

            // Load last active profile first in `update_profile_row` before
            // connecting to the signal to avoid unnecessary updates.
            self.profile_row
                .connect_selected_item_notify(clone!(@weak obj => move |row| {
                    if let Some(item) = row.selected_item() {
                        let profile = profile_from_obj(&item);
                        obj.settings().set_profile(profile);
                    }
                }));
        }
    }

    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends gtk::Widget, adw::Dialog, adw::PreferencesDialog;
}

impl PreferencesDialog {
    pub fn new(settings: &Settings) -> Self {
        glib::Object::builder()
            .property("settings", settings)
            .build()
    }

    fn update_file_chooser_button(&self) {
        let saving_location_display = self.settings().saving_location().display().to_string();

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
        let settings = self.settings();
        let active_profile = settings.profile();

        let imp = self.imp();
        let position = imp
            .profile_row
            .model()
            .unwrap()
            .into_iter()
            .position(
                |item| match (profile_from_obj(&item.unwrap()), &active_profile) {
                    (Some(profile), Some(active_profile)) => profile.id() == active_profile.id(),
                    (None, None) => true,
                    _ => false,
                },
            );

        if let Some(position) = position {
            imp.profile_row.set_selected(position as u32);
        } else {
            tracing::error!(
                "Active profile `{:?}` was not found on profile model",
                active_profile.as_ref().map(|p| p.id())
            );
        }
    }

    fn update_framerate_warning(&self) {
        let imp = self.imp();
        let settings = self.settings();

        imp.framerate_warning.set_visible(
            settings.profile().is_some_and(|profile| {
                settings.video_framerate() > profile.suggested_max_framerate()
            }),
        );
    }
}

fn profile_row_factory(
    profile_row: &adw::ComboRow,
    show_selected_indicator: bool,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(clone!(@weak profile_row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let item_expression = list_item.property_expression("item");

        let hbox = gtk::Box::builder().spacing(12).build();

        let warning_indicator = gtk::Image::builder()
            .tooltip_text(gettext("This format is experimental and unsupported."))
            .icon_name("warning-symbolic")
            .build();
        warning_indicator.add_css_class("warning");
        hbox.append(&warning_indicator);

        item_expression
            .chain_closure::<bool>(closure!(
                |_: Option<glib::Object>, obj: Option<glib::Object>| {
                    obj.as_ref()
                        .and_then(|obj| profile_from_obj(obj))
                        .is_some_and(|profile| profile.is_experimental())
                }
            ))
            .bind(&warning_indicator, "visible", glib::Object::NONE);

        let label = gtk::Label::builder()
            .valign(gtk::Align::Center)
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .max_width_chars(20)
            .build();
        hbox.append(&label);

        item_expression
            .chain_closure::<String>(closure!(
                |_: Option<glib::Object>, obj: Option<glib::Object>| {
                    obj.as_ref()
                        .and_then(|o| profile_from_obj(o))
                        .map_or(gettext("None"), |profile| profile.name().to_string())
                }
            ))
            .bind(&label, "label", glib::Object::NONE);

        if show_selected_indicator {
            let selected_indicator = gtk::Image::from_icon_name("object-select-symbolic");
            hbox.append(&selected_indicator);

            gtk::ClosureExpression::new::<f64>(
                &[
                    profile_row.property_expression("selected-item"),
                    item_expression,
                ],
                closure!(|_: Option<glib::Object>,
                          selected_item: Option<glib::Object>,
                          item: Option<glib::Object>| {
                    if item == selected_item {
                        1.0
                    } else {
                        0.0
                    }
                }),
            )
            .bind(&selected_indicator, "opacity", glib::Object::NONE);
        }

        list_item.set_child(Some(&hbox));
    }));
    factory
}

/// Returns `Some` if the object is a `Profile`, otherwise `None`, if the object is a `NoneProfile`.
fn profile_from_obj(obj: &glib::Object) -> Option<&Profile> {
    if let Some(profile) = obj.downcast_ref::<Profile>() {
        Some(profile)
    } else if obj.downcast_ref::<NoneProfile>().is_some() {
        None
    } else {
        tracing::warn!("Unexpected object type `{}`", obj.type_());
        None
    }
}
