use std::path::Path;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{
    gio,
    glib::{self, clone, BoxedAnyObject},
};

use crate::{
    experimental::Feature, format, item_row::ItemRow, profile::Profile, settings::Settings,
};

static BUILTIN_FRAMERATES: &[gst::Fraction] = &[
    gst::Fraction::from_integer(10),
    gst::Fraction::from_integer(20),
    gst::Fraction::from_integer(24),
    gst::Fraction::from_integer(25),
    gst::Fraction::new_raw(30_000, 1001), // 29.97
    gst::Fraction::from_integer(30),
    gst::Fraction::from_integer(48),
    gst::Fraction::from_integer(50),
    gst::Fraction::new_raw(60_000, 1001), // 59.94
    gst::Fraction::from_integer(60),
];

const ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY: &str = "kooha-row-selected-item-notify-handler-id";
const SETTINGS_PROFILE_CHANGED_HANDLER_ID_KEY: &str = "kooha-settings-profile-changed-handler-id";

/// Used to represent "none" profile in the profiles model
type NoneProfile = BoxedAnyObject;

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Debug, Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::PreferencesDialog)]
    #[template(resource = "/io/github/seadve/Kooha/ui/preferences_dialog.ui")]
    pub struct PreferencesDialog {
        #[property(get, set, construct_only)]
        pub(super) settings: OnceCell<Settings>,

        #[template_child]
        pub(super) delay_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) file_chooser_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) profile_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) framerate_row: TemplateChild<adw::ComboRow>,
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

            obj.setup_rows();

            let settings = obj.settings();
            settings
                .bind_record_delay(&self.delay_row.get(), "value")
                .build();
            settings.connect_saving_location_changed(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_file_chooser_label();
                }
            ));
            settings.connect_profile_changed(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_profile_row_selected();
                }
            ));
            settings.connect_framerate_changed(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_framerate_row_selected();
                }
            ));

            obj.update_file_chooser_label();
            obj.update_profile_row_selected();
            obj.update_framerate_row_selected();

            // Load last active value first in `update_*_row` before connecting to
            // the signal to avoid unnecessary updates.
            self.profile_row.connect_selected_item_notify(clone!(
                #[weak]
                obj,
                move |row| {
                    if let Some(item) = row.selected_item() {
                        let profile = profile_from_obj(&item);
                        obj.settings().set_profile(profile);
                    }
                }
            ));
            self.framerate_row.connect_selected_item_notify(clone!(
                #[weak]
                obj,
                move |row| {
                    if let Some(item) = row.selected_item() {
                        let framerate = item
                            .downcast_ref::<BoxedAnyObject>()
                            .unwrap()
                            .borrow::<gst::Fraction>();
                        obj.settings().set_framerate(*framerate);
                    }
                }
            ));
        }
    }

    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends gtk::Widget, adw::Dialog, adw::PreferencesDialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PreferencesDialog {
    pub fn new(settings: &Settings) -> Self {
        glib::Object::builder()
            .property("settings", settings)
            .build()
    }

    pub fn profile_row_grab_focus(&self) -> bool {
        self.imp().profile_row.grab_focus()
    }

    fn update_file_chooser_label(&self) {
        let imp = self.imp();

        let saving_location = self.settings().saving_location();
        imp.file_chooser_label
            .set_label(&display_path(&saving_location));
        imp.file_chooser_label
            .set_tooltip_text(Some(&saving_location.display().to_string()));
    }

    fn update_profile_row_selected(&self) {
        let imp = self.imp();

        let settings = self.settings();
        let active_profile = settings.profile();

        let model = imp.profile_row.model().unwrap();
        let position = model.iter().position(|item| {
            let item = item.unwrap();
            let profile = profile_from_obj(&item);
            profile.map(|p| p.id()) == active_profile.map(|p| p.id())
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

    fn update_framerate_row_selected(&self) {
        let imp = self.imp();

        let settings = self.settings();
        let framerate = settings.framerate();

        let model = imp.framerate_row.model().unwrap();
        let position = model
            .iter::<BoxedAnyObject>()
            .position(|item| *item.unwrap().borrow::<gst::Fraction>() == framerate);
        if let Some(position) = position {
            imp.framerate_row.set_selected(position as u32);
        } else {
            tracing::error!(
                "Active framerate `{:?}` was not found on framerate model",
                framerate
            );
        }
    }

    fn setup_rows(&self) {
        let imp = self.imp();

        let settings = self.settings();

        imp.framerate_row.set_factory(Some(&row_factory(
            &imp.framerate_row,
            &gettext("This frame rate may cause performance issues on the selected format."),
            clone!(
                #[strong]
                settings,
                move |list_item| {
                    let item = list_item.item().unwrap();
                    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

                    let framerate = item
                        .downcast_ref::<BoxedAnyObject>()
                        .unwrap()
                        .borrow::<gst::Fraction>();
                    item_row.set_title(format::framerate(*framerate));

                    unsafe {
                        list_item.set_data(
                            SETTINGS_PROFILE_CHANGED_HANDLER_ID_KEY,
                            settings.connect_profile_changed(clone!(
                                #[weak]
                                list_item,
                                move |settings| {
                                    update_framerate_row_shows_warning_icon(settings, &list_item);
                                }
                            )),
                        );
                    }

                    update_framerate_row_shows_warning_icon(&settings, list_item);
                }
            ),
            clone!(
                #[strong]
                settings,
                move |list_item| {
                    unsafe {
                        let handler_id = list_item
                            .steal_data(SETTINGS_PROFILE_CHANGED_HANDLER_ID_KEY)
                            .unwrap();
                        settings.disconnect(handler_id);
                    }
                }
            ),
        )));

        let framerate_model = {
            let items = BUILTIN_FRAMERATES
                .iter()
                .map(|fraction| BoxedAnyObject::new(*fraction))
                .collect::<Vec<_>>();

            let model = gio::ListStore::new::<BoxedAnyObject>();
            model.splice(0, 0, &items);

            let active_framerate = settings.framerate();
            if !BUILTIN_FRAMERATES.contains(&active_framerate) {
                model.append(&BoxedAnyObject::new(active_framerate));
            }

            model
        };
        imp.framerate_row.set_model(Some(&framerate_model));

        imp.profile_row.set_factory(Some(&row_factory(
            &imp.profile_row,
            &gettext("This format is experimental and unsupported."),
            |list_item| {
                let item = list_item.item().unwrap();
                let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

                let profile = profile_from_obj(&item);
                item_row.set_title(
                    profile.map_or_else(|| gettext("None"), |profile| profile.name().to_string()),
                );
                item_row.set_shows_warning_icon(
                    profile.is_some_and(|profile| profile.is_experimental()),
                );
            },
            |_| {},
        )));

        let active_profile = settings.profile();
        let profile_model = {
            let profiles = Profile::all()
                .inspect_err(|err| tracing::error!("Failed to load profiles: {:?}", err))
                .unwrap_or_default();

            let model = gio::ListStore::new::<glib::Object>();
            model.splice(0, 0, profiles);

            if active_profile.is_none() {
                model.insert(0, &NoneProfile::new(()));
            }

            model
        };
        let filter = gtk::CustomFilter::new(move |obj| {
            profile_from_obj(obj).is_none_or(|profile| {
                (Feature::ExperimentalFormats.is_enabled()
                    || !profile.is_experimental()
                    || active_profile.is_some_and(|active_profile| active_profile == profile))
                    && profile.is_available()
            })
        });
        let profile_filter_model = gtk::FilterListModel::new(Some(profile_model), Some(filter));
        imp.profile_row.set_model(Some(&profile_filter_model));
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
    factory.connect_setup(move |_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

        let item_row = ItemRow::new();
        item_row.set_warning_tooltip_text(warning_tooltip_text.as_str());

        list_item.set_child(Some(&item_row));
    });

    factory.connect_bind(clone!(
        #[weak]
        row,
        move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

            let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

            // Only show the selected icon when it is inside the given row's popover. This assumes that
            // the parent of the given row is not a popover, so we can tell which is which.
            if item_row.ancestor(gtk::Popover::static_type()).is_some() {
                debug_assert!(row.ancestor(gtk::Popover::static_type()).is_none());

                item_row.set_is_on_popover(true);

                unsafe {
                    list_item.set_data(
                        ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY,
                        row.connect_selected_item_notify(clone!(
                            #[weak]
                            list_item,
                            move |row| {
                                update_item_row_is_selected(row, &list_item);
                            }
                        )),
                    );
                }

                update_item_row_is_selected(&row, list_item);
            } else {
                item_row.set_is_on_popover(false);
            }

            bind_cb(list_item);
        }
    ));

    factory.connect_unbind(clone!(
        #[weak]
        row,
        move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();

            unsafe {
                if let Some(handler_id) =
                    list_item.steal_data(ROW_SELECTED_ITEM_NOTIFY_HANDLER_ID_KEY)
                {
                    row.disconnect(handler_id);
                }
            }

            unbind_cb(list_item);
        }
    ));

    factory
}

fn update_item_row_is_selected(row: &adw::ComboRow, list_item: &gtk::ListItem) {
    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();

    item_row.set_is_selected(row.selected_item() == list_item.item());
}

fn update_framerate_row_shows_warning_icon(settings: &Settings, list_item: &gtk::ListItem) {
    let item_row = list_item.child().unwrap().downcast::<ItemRow>().unwrap();
    let item = list_item.item().unwrap();

    let framerate = item
        .downcast_ref::<BoxedAnyObject>()
        .unwrap()
        .borrow::<gst::Fraction>();

    item_row.set_shows_warning_icon(
        settings
            .profile()
            .is_some_and(|profile| *framerate > profile.suggested_max_framerate()),
    );
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
/// Returns a shortened human-readable representation of the path.
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

    #[track_caller]
    fn assert_simplified_framerate(framerate: gst::Fraction) {
        let reduced = framerate.0.reduced();

        assert_eq!(
            (framerate.numer(), framerate.denom()),
            (*reduced.numer(), *reduced.denom())
        );
    }

    #[test]
    fn simplified_framerate() {
        for framerate in BUILTIN_FRAMERATES {
            assert_simplified_framerate(*framerate);
        }
    }
}
