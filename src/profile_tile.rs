use gtk::{
    glib::{self, closure_local},
    prelude::*,
    subclass::prelude::*,
};

use std::cell::{Cell, RefCell};

use crate::profile::Profile;

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use gtk::CompositeTemplate;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/profile-tile.ui")]
    pub struct ProfileTile {
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) muxer_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) video_encoder_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) audio_encoder_label: TemplateChild<gtk::Label>,

        pub(super) profile: RefCell<Option<Profile>>,
        pub(super) is_selected: Cell<bool>,

        pub(super) binding_group: glib::BindingGroup,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileTile {
        const NAME: &'static str = "KoohaProfileTile";
        type Type = super::ProfileTile;
        type ParentType = gtk::FlowBoxChild;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.set_css_name("profiletile");

            klass.install_action("profile-tile.delete", None, |obj, _, _| {
                obj.emit_by_name::<()>("delete-request", &[]);
            });

            klass.install_action("profile-tile.copy", None, |obj, _, _| {
                obj.emit_by_name::<()>("copy-request", &[]);
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProfileTile {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    // Profile to show by self
                    glib::ParamSpecObject::builder("profile", Profile::static_type())
                        .flags(glib::ParamFlags::READWRITE | glib::ParamFlags::EXPLICIT_NOTIFY)
                        .build(),
                    // Whether self should be displayed as selected
                    glib::ParamSpecBoolean::builder("selected")
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
                "profile" => {
                    let profile = value.get().unwrap();
                    obj.set_profile(profile);
                }
                "selected" => {
                    let is_selected = value.get().unwrap();
                    obj.set_selected(is_selected);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "profile" => obj.profile().to_value(),
                "selected" => obj.is_selected().to_value(),
                _ => unimplemented!(),
            }
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("delete-request", &[], <()>::static_type().into()).build(),
                    Signal::builder("copy-request", &[], <()>::static_type().into()).build(),
                ]
            });

            SIGNALS.as_ref()
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.binding_group
                .bind("name", &self.name_label.get(), "label")
                .build();
            self.binding_group
                .bind("container-preset-name", &self.muxer_label.get(), "label")
                .build();
            self.binding_group
                .bind(
                    "video-preset-name",
                    &self.video_encoder_label.get(),
                    "label",
                )
                .build();
            self.binding_group
                .bind(
                    "audio-preset-name",
                    &self.audio_encoder_label.get(),
                    "label",
                )
                .build();
        }
    }

    impl WidgetImpl for ProfileTile {}
    impl FlowBoxChildImpl for ProfileTile {}
}

glib::wrapper! {
     pub struct ProfileTile(ObjectSubclass<imp::ProfileTile>)
        @extends gtk::Widget, gtk::FlowBoxChild;
}

impl ProfileTile {
    pub fn new(profile: &Profile) -> Self {
        glib::Object::new(&[("profile", profile)]).expect("Failed to create ProfileTile.")
    }

    pub fn set_profile(&self, profile: Option<&Profile>) {
        if profile == self.profile().as_ref() {
            return;
        }

        let imp = self.imp();
        imp.profile.replace(profile.cloned());
        imp.binding_group.set_source(profile);
        self.notify("profile");
    }

    pub fn profile(&self) -> Option<Profile> {
        self.imp().profile.borrow().clone()
    }

    pub fn set_selected(&self, is_selected: bool) {
        if is_selected == self.is_selected() {
            return;
        }

        self.imp().is_selected.set(is_selected);

        if is_selected {
            self.add_css_class("selected");
        } else {
            self.remove_css_class("selected");
        }

        self.notify("selected");
    }

    pub fn is_selected(&self) -> bool {
        self.imp().is_selected.get()
    }

    pub fn connect_delete_request<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "delete-request",
            true,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
    }

    pub fn connect_copy_request<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_closure(
            "copy-request",
            true,
            closure_local!(|obj: &Self| {
                f(obj);
            }),
        )
    }
}
