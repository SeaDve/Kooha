use gtk::{glib, prelude::*, subclass::prelude::*};

use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::ToggleButton)]
    pub struct ToggleButton {
        /// Icon name to show on un-toggled state
        #[property(get, set = Self::set_default_icon_name, explicit_notify)]
        pub(super) default_icon_name: RefCell<String>,
        /// Icon name to show on toggled state
        #[property(get, set = Self::set_toggled_icon_name, explicit_notify)]
        pub(super) toggled_icon_name: RefCell<String>,
        /// Tooltip text to show on un-toggled state
        #[property(get, set = Self::set_default_tooltip_text, explicit_notify)]
        pub(super) default_tooltip_text: RefCell<String>,
        /// Tooltip text to show on toggled state
        #[property(get, set = Self::set_toggled_tooltip_text, explicit_notify)]
        pub(super) toggled_tooltip_text: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ToggleButton {
        const NAME: &'static str = "KoohaToggleButton";
        type Type = super::ToggleButton;
        type ParentType = gtk::ToggleButton;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ToggleButton {}

    impl WidgetImpl for ToggleButton {}
    impl ButtonImpl for ToggleButton {}

    impl ToggleButtonImpl for ToggleButton {
        fn toggled(&self) {
            let obj = self.obj();

            obj.update_icon_name();
            obj.update_tooltip_text();

            self.parent_toggled();
        }
    }

    impl ToggleButton {
        fn set_default_icon_name(&self, default_icon_name: &str) {
            let obj = self.obj();

            if default_icon_name == obj.default_icon_name().as_str() {
                return;
            }

            self.default_icon_name
                .replace(default_icon_name.to_string());
            obj.update_icon_name();
            obj.notify_default_icon_name();
        }

        fn set_toggled_icon_name(&self, toggled_icon_name: &str) {
            let obj = self.obj();

            if toggled_icon_name == obj.toggled_icon_name().as_str() {
                return;
            }

            self.toggled_icon_name
                .replace(toggled_icon_name.to_string());
            obj.update_icon_name();
            obj.notify_toggled_icon_name();
        }

        fn set_default_tooltip_text(&self, default_tooltip_text: &str) {
            let obj = self.obj();

            if default_tooltip_text == obj.default_tooltip_text().as_str() {
                return;
            }

            self.default_tooltip_text
                .replace(default_tooltip_text.to_string());
            obj.update_tooltip_text();
            obj.notify_default_tooltip_text();
        }

        fn set_toggled_tooltip_text(&self, toggled_tooltip_text: &str) {
            let obj = self.obj();

            if toggled_tooltip_text == obj.toggled_tooltip_text().as_str() {
                return;
            }

            self.toggled_tooltip_text
                .replace(toggled_tooltip_text.to_string());
            obj.update_tooltip_text();
            obj.notify_toggled_tooltip_text();
        }
    }
}

glib::wrapper! {
    /// A toggle button that shows different icons and tooltips depending on the state.
    ///
    /// Note: `icon-name` and `tooltip-text` must not be set directly.
     pub struct ToggleButton(ObjectSubclass<imp::ToggleButton>)
        @extends gtk::Widget, gtk::Button, gtk::ToggleButton;
}

impl ToggleButton {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn update_icon_name(&self) {
        let icon_name = if self.is_active() {
            self.toggled_icon_name()
        } else {
            self.default_icon_name()
        };
        self.set_icon_name(&icon_name);
    }

    fn update_tooltip_text(&self) {
        let tooltip_text = if self.is_active() {
            self.toggled_tooltip_text()
        } else {
            self.default_tooltip_text()
        };
        self.set_tooltip_text(if tooltip_text.is_empty() {
            None
        } else {
            Some(&tooltip_text)
        });
    }
}
