use gst::prelude::*;
use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use once_cell::unsync::OnceCell;

use std::cell::RefCell;

use crate::{
    element_properties::{ElementFactoryPropertiesMap, ElementProperties},
    profile::Profile,
    utils,
};

// TODO serialize

const SUPPORTED_MUXERS: [&str; 3] = ["webmmux", "mp4mux", "matroskamux"];
const SUPPORTED_VIDEO_ENCODERS: [&str; 2] = ["vp8enc", "x264enc"];
const SUPPORTED_AUDIO_ENCODERS: [&str; 2] = ["opusenc", "lamemp3enc"];

mod imp {
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct ProfileManager {
        pub(super) active_profile: RefCell<Option<Profile>>,

        pub(super) profiles: RefCell<Vec<Profile>>,

        pub(super) known_muxers: OnceCell<gtk::SortListModel>,
        pub(super) known_audio_encoders: OnceCell<gtk::SortListModel>,
        pub(super) known_video_encoders: OnceCell<gtk::SortListModel>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileManager {
        const NAME: &'static str = "KoohaProfileManager";
        type Type = super::ProfileManager;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ProfileManager {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder("active-profile", Profile::static_type())
                        .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "active-profile" => {
                    let profile = value.get().unwrap();
                    obj.set_active_profile(profile);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "active-profile" => obj.active_profile().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            for profile in builtin_profiles() {
                obj.add_profile(profile);
            }

            if let Some(first_item) = obj.get_profile(0) {
                obj.set_active_profile(Some(&first_item));
            }
        }
    }

    impl ListModelImpl for ProfileManager {
        fn item_type(&self, _obj: &Self::Type) -> glib::Type {
            Profile::static_type()
        }

        fn n_items(&self, _obj: &Self::Type) -> u32 {
            self.profiles.borrow().len() as u32
        }

        fn item(&self, obj: &Self::Type, position: u32) -> Option<glib::Object> {
            obj.get_profile(position).map(|profile| profile.upcast())
        }
    }
}

glib::wrapper! {
     pub struct ProfileManager(ObjectSubclass<imp::ProfileManager>)
        @implements gio::ListModel;
}

impl ProfileManager {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ProfileManager.")
    }

    pub fn active_profile(&self) -> Option<Profile> {
        self.imp().active_profile.borrow().clone()
    }

    pub fn set_active_profile(&self, profile: Option<&Profile>) {
        if profile == self.active_profile().as_ref() {
            return;
        }

        tracing::debug!(
            "Set active profile to {:?}",
            profile.map(|profile| profile.name())
        );

        if let Some(profile) = profile {
            if !self.contains_profile(profile) {
                self.add_profile(profile.clone());
            }
        }

        self.imp().active_profile.replace(profile.cloned());
        self.notify("active-profile");
    }

    pub fn connect_active_profile_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("active-profile"), move |obj, _| f(obj))
    }

    pub fn add_profile(&self, profile: Profile) {
        let position_appended = {
            let mut profiles = self.imp().profiles.borrow_mut();
            profiles.push(profile);
            profiles.len() as u32 - 1
        };
        self.items_changed(position_appended, 0, 1);
    }

    pub fn remove_profile(&self, profile: &Profile) -> bool {
        let imp = self.imp();
        let position = imp
            .profiles
            .borrow()
            .iter()
            .position(|stored_profile| stored_profile == profile);

        if let Some(position) = position {
            let removed = imp.profiles.borrow_mut().remove(position);
            self.items_changed(position as u32, 1, 0);

            if Some(removed) == self.active_profile() {
                if let Some(first_item) = self.get_profile(0) {
                    self.set_active_profile(Some(&first_item));
                }
            }
        } else {
            tracing::debug!(
                "Didn't delete profile with name `{}` as it does not exist",
                profile.name()
            );
        }

        position.is_some()
    }

    pub fn known_muxers(&self) -> &gtk::SortListModel {
        self.imp().known_muxers.get_or_init(|| {
            new_element_factory_sort_list_model(
                gst::ElementFactoryType::MUXER,
                gst::Rank::Primary,
                &SUPPORTED_MUXERS,
            )
        })
    }

    pub fn known_video_encoders(&self) -> &gtk::SortListModel {
        self.imp().known_video_encoders.get_or_init(|| {
            new_element_factory_sort_list_model(
                gst::ElementFactoryType::VIDEO_ENCODER,
                gst::Rank::None,
                &SUPPORTED_VIDEO_ENCODERS,
            )
        })
    }

    pub fn known_audio_encoders(&self) -> &gtk::SortListModel {
        self.imp().known_audio_encoders.get_or_init(|| {
            new_element_factory_sort_list_model(
                gst::ElementFactoryType::AUDIO_ENCODER,
                gst::Rank::None,
                &SUPPORTED_AUDIO_ENCODERS,
            )
        })
    }

    fn get_profile(&self, position: u32) -> Option<Profile> {
        self.imp().profiles.borrow().get(position as usize).cloned()
    }

    fn contains_profile(&self, profile: &Profile) -> bool {
        self.imp().profiles.borrow().contains(profile)
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new()
    }
}

fn builtin_profiles() -> Vec<Profile> {
    // TODO make builtins readonly
    vec![
        // TODO bring back gif support `gifenc repeat=-1 speed=30`. Disable `win.record-speaker` and `win.record-mic` actions. 15 fps override
        // TODO vaapi?
        // TODO Handle missing plugins (Hide profile if missing)
        {
            let profile = Profile::new("WebM");
            profile.set_container_preset_name("webmmux");
            profile.set_video_preset_name("vp8enc");
            profile.set_video_element_properties(
                ElementProperties::builder()
                    .item(
                        ElementFactoryPropertiesMap::builder("vp8enc")
                            .field("max-quantizer", 17)
                            .field("cpu-used", 16)
                            .field("cq-level", 13)
                            .field("deadline", 1)
                            .field("static-threshold", 100)
                            .field_from_str("keyframe-mode", "disabled")
                            .field("buffer-size", 20000)
                            .field("threads", utils::ideal_thread_count())
                            .build(),
                    )
                    .build(),
            );
            profile.set_audio_preset_name("opusenc");
            profile
        },
        {
            // TODO support "profile" = baseline
            let profile = Profile::new("MP4");
            profile.set_container_preset_name("mp4mux");
            profile.set_video_preset_name("x264enc");
            profile.set_video_element_properties(
                ElementProperties::builder()
                    .item(
                        ElementFactoryPropertiesMap::builder("x264enc")
                            .field("qp-max", 17)
                            .field_from_str("speed-preset", "superfast")
                            .field("threads", utils::ideal_thread_count())
                            .build(),
                    )
                    .build(),
            );
            profile.set_audio_preset_name("lamemp3enc");
            profile
        },
        {
            let profile = Profile::new("Matroska");
            profile.set_container_preset_name("matroskamux");
            profile.set_video_preset_name("x264enc");
            profile.set_video_element_properties(
                ElementProperties::builder()
                    .item(
                        ElementFactoryPropertiesMap::builder("x264enc")
                            .field("qp-max", 17)
                            .field_from_str("speed-preset", "superfast")
                            .field("threads", utils::ideal_thread_count())
                            .build(),
                    )
                    .build(),
            );
            profile.set_audio_preset_name("opusenc");
            profile
        },
    ]
}

fn new_element_factory_sort_list_model(
    type_: gst::ElementFactoryType,
    min_rank: gst::Rank,
    sort_first_names: &'static [&str],
) -> gtk::SortListModel {
    fn new_sorter<T: IsA<glib::Object>>(
        func: impl Fn(&T, &T) -> gtk::Ordering + 'static,
    ) -> gtk::Sorter {
        gtk::CustomSorter::new(move |a, b| {
            let ef_a = a.downcast_ref().unwrap();
            let ef_b = b.downcast_ref().unwrap();
            func(ef_a, ef_b)
        })
        .upcast()
    }

    let factories = gst::ElementFactory::factories_with_type(type_, min_rank);

    let sorter = gtk::MultiSorter::new();
    sorter.append(&new_sorter(
        |a: &gst::ElementFactory, b: &gst::ElementFactory| a.rank().cmp(&b.rank()).reverse().into(),
    ));
    sorter.append(&new_sorter(
        move |a: &gst::ElementFactory, b: &gst::ElementFactory| {
            let a_score = sort_first_names
                .iter()
                .position(|name| *name == a.name())
                .map_or(i32::MAX, |index| index as i32);
            let b_score = sort_first_names
                .iter()
                .position(|name| *name == b.name())
                .map_or(i32::MAX, |index| index as i32);
            a_score.cmp(&b_score).into()
        },
    ));

    let list_store = gio::ListStore::new(gst::ElementFactory::static_type());
    list_store.splice(0, 0, &factories.collect::<Vec<_>>());
    gtk::SortListModel::new(Some(&list_store), Some(&sorter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_work() {
        for profile in builtin_profiles() {
            assert!(profile.to_encoding_profile().is_ok());
        }
    }
}
