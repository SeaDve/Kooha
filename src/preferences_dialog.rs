use std::path::Path;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, closure, BoxedAnyObject},
};

use crate::{item_row::ItemRow, profile::Profile, settings::Settings, IS_EXPERIMENTAL_MODE};

/// Used to represent "none" profile in the profiles model
type NoneProfile = BoxedAnyObject;

const PROFILE_ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY: &str =
    "kooha-profile-row-selected-item-notify-handler-id";

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
                    let parent = obj.root().map(|r| r.downcast::<gtk::Window>().unwrap());
                    if let Err(err) = obj.settings().select_saving_location(parent.as_ref()).await {
                        if !err
                            .downcast_ref::<glib::Error>()
                            .is_some_and(|error| error.matches(gtk::DialogError::Dismissed))
                        {
                            tracing::error!("Failed to select saving location: {:?}", err);

                            let toast =
                                adw::Toast::new(&gettext("Failed to set recordings folder"));
                            obj.add_toast(toast);
                        }
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
                .set_factory(Some(&profile_row_factory(&self.profile_row)));
            let profiles = Profile::all()
                .inspect_err(|err| tracing::error!("Failed to load profiles: {:?}", err))
                .unwrap_or_default();
            let profiles_model = gio::ListStore::new::<glib::Object>();
            if active_profile.is_none() {
                profiles_model.append(&NoneProfile::new(()));
            }
            profiles_model.splice(profiles_model.n_items(), 0, profiles);

            let filter = gtk::BoolFilter::new(Some(&gtk::ClosureExpression::new::<bool>(
                &[] as &[&gtk::Expression],
                closure!(|obj: glib::Object| {
                    profile_from_obj(&obj).map_or(true, |profile| {
                        (*IS_EXPERIMENTAL_MODE
                            || !profile.is_experimental()
                            || active_profile
                                .is_some_and(|active_profile| active_profile == profile))
                            && profile.is_available()
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
        let imp = self.imp();

        let saving_location = self.settings().saving_location();
        imp.file_chooser_button_content
            .set_label(&display_path(&saving_location));
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

fn profile_row_factory(profile_row: &adw::ComboRow) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(clone!(@weak profile_row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        let item_row = ItemRow::new();
        item_row.set_warning_tooltip_text(gettext("This format is experimental and unsupported."));

        list_item.set_child(Some(&item_row));
    }));

    factory.connect_bind(clone!(@weak profile_row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

        let item = list_item.item().unwrap();
        let profile = profile_from_obj(&item);

        item_row.set_shows_warning_icon(profile.is_some_and(|profile| profile.is_experimental()));
        item_row.set_title(
            profile.map_or_else(|| gettext("None"), |profile| profile.name().to_string()),
        );

        // Only show the selected icon when it is inside the given row's popover. This assumes that
        // the parent of the given row is not a popover, so we can tell which is which.
        if item_row.ancestor(gtk::Popover::static_type()).is_some() {
            debug_assert!(profile_row.ancestor(gtk::Popover::static_type()).is_none());

            item_row.set_shows_selected_icon(true);

            unsafe {
                list_item.set_data(
                    PROFILE_ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY,
                    profile_row.connect_selected_item_notify(
                        clone!(@weak list_item => move |profile_row| {
                            update_item_row_is_selected(profile_row, &list_item);
                        }),
                    ),
                );
            }

            update_item_row_is_selected(&profile_row, list_item);
        } else {
            item_row.set_shows_selected_icon(false);
        }
    }));

    factory.connect_unbind(clone!(@weak profile_row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        unsafe {
            if let Some(handler_id) =
                list_item.steal_data(PROFILE_ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY)
            {
                profile_row.disconnect(handler_id);
            }
        }
    }));

    factory
}

fn update_item_row_is_selected(row: &adw::ComboRow, list_item: &gtk::ListItem) {
    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

    item_row.set_is_selected(row.selected_item() == list_item.item());
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

// Copied from Delineate
// See https://github.com/SeaDve/Delineate/blob/e5f57835133a85c002961e681dc8935249458ef7/src/utils.rs#L71
/// Returns a human-readable representation of the path.
fn display_path(path: &Path) -> String {
    let home_dir = glib::home_dir();

    if path == home_dir {
        return "~/".to_string();
    }

    let path_display = path.display().to_string();

    if path.starts_with(&home_dir) {
        let home_dir_display = home_dir.display().to_string();
        return format!("~{}", &path_display[home_dir_display.len()..]);
    }

    path_display
}
