use adw::subclass::prelude::*;
use gtk::{
    glib::{self, clone},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::{cell::Cell, time::Duration};

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct ToggleButton {
        action_enabled: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToggleButton {
        const NAME: &'static str = "ToggleButton";
        type Type = super::ToggleButton;
        type ParentType = gtk::ToggleButton;

        fn new() -> Self {
            Self {
                action_enabled: Cell::new(false),
            }
        }
    }

    impl ObjectImpl for ToggleButton {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpec::new_boolean(
                    "action-enabled",
                    "action-enabled",
                    "action-enabled",
                    false,
                    glib::ParamFlags::READWRITE,
                )]
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
                "action-enabled" => {
                    let action_enabled = value.get().unwrap();
                    self.action_enabled.set(action_enabled);

                    // This is a workaround. For some reason, sensitive property doesn't
                    // get updated on widget construction, so we have to add 2ms delay.
                    glib::timeout_add_local_once(
                        Duration::from_millis(2),
                        clone!(@weak obj as button => move || {
                            button.set_sensitive(action_enabled);
                        }),
                    );
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "action-enabled" => self.action_enabled.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl WidgetImpl for ToggleButton {}
    impl ButtonImpl for ToggleButton {}
    impl ToggleButtonImpl for ToggleButton {}
}

glib::wrapper! {
    pub struct ToggleButton(ObjectSubclass<imp::ToggleButton>)
        @extends gtk::Widget, gtk::Button, gtk::ToggleButton;
}

impl ToggleButton {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create ToggleButton")
    }

    pub fn set_action_enabled(&self, is_enabled: bool) {
        self.set_property("action-enabled", is_enabled)
            .expect("Failed to set action enabled");
    }
}
