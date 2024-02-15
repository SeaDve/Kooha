use std::{fmt, path::Path};

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, closure, translate::FromGlib, BoxedAnyObject},
};
use num_traits::Signed;

use crate::{
    item_row::ItemRow, pipeline::Framerate, profile::Profile, settings::Settings,
    IS_EXPERIMENTAL_MODE,
};

const ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY: &str = "kooha-row-selected-item-notify-handler-id";
const SETTINGS_PROFILE_CHANGED_HANDLER_ID_KEY: &str = "kooha-settings-profile-changed-handler-id";

/// Used to represent "none" profile in the profiles model
type NoneProfile = BoxedAnyObject;

#[derive(Debug, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "KoohaFramerateOption")]
pub enum FramerateOption {
    _10,
    _20,
    _23_976,
    _24,
    _25,
    _29_97,
    _30,
    _48,
    _50,
    _59_94,
    _60,
}

impl FramerateOption {
    /// Returns the closest `FramerateOption` to the given `Framerate`.
    pub fn from_framerate(framerate: Framerate) -> Self {
        let all = [
            Self::_10,
            Self::_20,
            Self::_23_976,
            Self::_24,
            Self::_25,
            Self::_29_97,
            Self::_30,
            Self::_48,
            Self::_50,
            Self::_59_94,
            Self::_60,
        ];

        *all.iter()
            .min_by(|a, b| {
                (a.to_framerate() - framerate)
                    .abs()
                    .cmp(&(b.to_framerate() - framerate).abs())
            })
            .unwrap()
    }

    /// Converts a `FramerateOption` to a `Framerate`.
    pub fn to_framerate(self) -> Framerate {
        let (numer, denom) = match self {
            Self::_10 => (10, 1),
            Self::_20 => (20, 1),
            Self::_23_976 => (24_000, 1001),
            Self::_24 => (24, 1),
            Self::_25 => (25, 1),
            Self::_29_97 => (30_000, 1001),
            Self::_30 => (30, 1),
            Self::_48 => (48, 1),
            Self::_50 => (50, 1),
            Self::_59_94 => (60_000, 1001),
            Self::_60 => (60, 1),
        };
        Framerate::new(numer, denom)
    }
}

impl fmt::Display for FramerateOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::_10 => "10",
            Self::_20 => "20",
            Self::_23_976 => "23.976",
            Self::_24 => "24 NTSC",
            Self::_25 => "25 PAL",
            Self::_29_97 => "29.97",
            Self::_30 => "30",
            Self::_48 => "48",
            Self::_50 => "50 PAL",
            Self::_59_94 => "59.94",
            Self::_60 => "60",
        };
        f.write_str(name)
    }
}

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
        pub(super) profile_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) framerate_row: TemplateChild<adw::ComboRow>,
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

            self.framerate_row.set_factory(Some(&row_factory(
                &self.framerate_row,
                &gettext("This frame rate may cause performance issues on the selected format."),
                clone!(@strong settings => move |list_item| {
                    let item = list_item.item().unwrap();
                    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

                    let enum_list_item = item.downcast_ref::<adw::EnumListItem>().unwrap();
                    let framerate_option = unsafe { FramerateOption::from_glib(enum_list_item.value()) };
                    item_row.set_title(framerate_option.to_string());

                    unsafe {
                        list_item.set_data(
                            SETTINGS_PROFILE_CHANGED_HANDLER_ID_KEY,
                            settings.connect_profile_changed(
                                clone!(@weak list_item => move |settings| {
                                    update_item_row_shows_warning_icon(settings, &list_item);
                                }),
                            ),
                        );
                    }

                    update_item_row_shows_warning_icon(&settings, list_item);
                }),
                clone!(@strong settings => move |list_item| {
                    unsafe {
                        let handler_id = list_item
                            .steal_data(SETTINGS_PROFILE_CHANGED_HANDLER_ID_KEY)
                            .unwrap();
                        settings.disconnect(handler_id);
                    }
                }),
            )));
            self.framerate_row.set_model(Some(&adw::EnumListModel::new(
                FramerateOption::static_type(),
            )));

            self.profile_row.set_factory(Some(&row_factory(
                &self.profile_row,
                &gettext(gettext("This format is experimental and unsupported.")),
                |list_item| {
                    let item = list_item.item().unwrap();
                    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

                    let profile = profile_from_obj(&item);
                    item_row.set_title(
                        profile
                            .map_or_else(|| gettext("None"), |profile| profile.name().to_string()),
                    );
                    item_row.set_shows_warning_icon(
                        profile.is_some_and(|profile| profile.is_experimental()),
                    );
                },
                |_| {},
            )));
            let profiles = Profile::all()
                .inspect_err(|err| tracing::error!("Failed to load profiles: {:?}", err))
                .unwrap_or_default();
            let profiles_model = gio::ListStore::new::<glib::Object>();
            let active_profile = settings.profile();
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

            settings.connect_framerate_changed(clone!(@weak obj => move |_| {
                obj.update_framerate_row();
            }));

            settings.connect_saving_location_changed(clone!(@weak obj => move |_| {
                obj.update_file_chooser_button();
            }));

            settings.connect_profile_changed(clone!(@weak obj => move |_| {
                obj.update_profile_row();
            }));

            obj.update_file_chooser_button();
            obj.update_profile_row();
            obj.update_framerate_row();

            // Load last active value first in `update_*_row` before connecting to
            // the signal to avoid unnecessary updates.
            self.profile_row
                .connect_selected_item_notify(clone!(@weak obj => move |row| {
                    if let Some(item) = row.selected_item() {
                        let profile = profile_from_obj(&item);
                        obj.settings().set_profile(profile);
                    }
                }));
            self.framerate_row
                .connect_selected_item_notify(clone!(@weak obj => move |row| {
                    if let Some(item) = row.selected_item() {
                        let enum_list_item = item.downcast_ref::<adw::EnumListItem>().unwrap();
                        let framerate_option = unsafe { FramerateOption::from_glib(enum_list_item.value()) };
                        obj.settings().set_framerate(framerate_option.to_framerate());
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
        let imp = self.imp();

        let settings = self.settings();
        let active_profile = settings.profile();

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

    fn update_framerate_row(&self) {
        let imp = self.imp();

        let settings = self.settings();
        let framerate_option = FramerateOption::from_framerate(settings.framerate());

        let position = imp
            .framerate_row
            .model()
            .unwrap()
            .into_iter()
            .position(|item| {
                let item = item.unwrap();
                let enum_list_item = item.downcast::<adw::EnumListItem>().unwrap();
                enum_list_item.value() == framerate_option as i32
            });
        if let Some(position) = position {
            imp.framerate_row.set_selected(position as u32);
        } else {
            tracing::error!(
                "Active framerate `{:?}` was not found on framerate model",
                framerate_option
            );
        }
    }
}

fn row_factory(
    row: &adw::ComboRow,
    warning_tooltip_text: &str,
    bind_cb: impl Fn(&gtk::ListItem) + 'static,
    unbind_cb: impl Fn(&gtk::ListItem) + 'static,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    let warning_tooltip_text = warning_tooltip_text.to_string();
    factory.connect_setup(clone!(@weak row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        let item_row = ItemRow::new();
        item_row.set_warning_tooltip_text(warning_tooltip_text.as_str());

        list_item.set_child(Some(&item_row));
    }));

    factory.connect_bind(clone!(@weak row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

        // Only show the selected icon when it is inside the given row's popover. This assumes that
        // the parent of the given row is not a popover, so we can tell which is which.
        if item_row.ancestor(gtk::Popover::static_type()).is_some() {
            debug_assert!(row.ancestor(gtk::Popover::static_type()).is_none());

            item_row.set_shows_selected_icon(true);

            unsafe {
                list_item.set_data(
                    ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY,
                    row.connect_selected_item_notify(clone!(@weak list_item => move |row| {
                        update_item_row_is_selected(row, &list_item);
                    })),
                );
            }

            update_item_row_is_selected(&row, list_item);
        } else {
            item_row.set_shows_selected_icon(false);
        }

        bind_cb(list_item);
    }));

    factory.connect_unbind(clone!(@weak row => move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        unsafe {
            if let Some(handler_id) = list_item.steal_data(ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY)
            {
                row.disconnect(handler_id);
            }
        }

        unbind_cb(list_item);
    }));

    factory
}

fn update_item_row_is_selected(row: &adw::ComboRow, list_item: &gtk::ListItem) {
    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

    item_row.set_is_selected(row.selected_item() == list_item.item());
}

fn update_item_row_shows_warning_icon(settings: &Settings, list_item: &gtk::ListItem) {
    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();
    let item = list_item.item().unwrap();

    let enum_list_item = item.downcast_ref::<adw::EnumListItem>().unwrap();

    let framerate_option = unsafe { FramerateOption::from_glib(enum_list_item.value()) };

    item_row.set_shows_warning_icon(settings.profile().is_some_and(|profile| {
        framerate_option.to_framerate() > profile.suggested_max_framerate()
    }));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framerate_option() {
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(5)),
            FramerateOption::_10
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(10)),
            FramerateOption::_10
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(20)),
            FramerateOption::_20
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::approximate_float(23.976).unwrap()),
            FramerateOption::_23_976
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(24)),
            FramerateOption::_24
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(25)),
            FramerateOption::_25
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::approximate_float(29.97).unwrap()),
            FramerateOption::_29_97
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(30)),
            FramerateOption::_30
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(48)),
            FramerateOption::_48
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(50)),
            FramerateOption::_50
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::approximate_float(59.94).unwrap()),
            FramerateOption::_59_94
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(60)),
            FramerateOption::_60
        );
        assert_eq!(
            FramerateOption::from_framerate(Framerate::from_integer(120)),
            FramerateOption::_60
        );
    }
}
