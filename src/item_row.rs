use gtk::{glib, prelude::*, subclass::prelude::*};

mod imp {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Default, glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type  = super::ItemRow)]
    #[template(resource = "/io/github/seadve/Kooha/ui/item_row.ui")]
    pub struct ItemRow {
        #[property(get, set = Self::set_title, explicit_notify)]
        pub(super) title: RefCell<String>,
        #[property(get, set = Self::set_warning_tooltip_text, explicit_notify)]
        pub(super) warning_tooltip_text: RefCell<String>,
        #[property(get, set = Self::set_shows_warning_icon, explicit_notify)]
        pub(super) shows_warning_icon: Cell<bool>,
        #[property(get, set = Self::set_is_on_popover, explicit_notify)]
        pub(super) is_on_popover: Cell<bool>,
        #[property(get, set = Self::set_is_selected, explicit_notify)]
        pub(super) is_selected: Cell<bool>,

        #[template_child]
        pub(super) start_warning_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) selected_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) end_warning_icon: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ItemRow {
        const NAME: &'static str = "KoohaItemRow";
        type Type = super::ItemRow;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ItemRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.update_title_label();
            obj.update_warning_icon_tooltip_text();
            obj.update_warning_icons_visibility();
            obj.update_selected_icon_visibility();
            obj.update_selected_icon_opacity();
        }

        fn dispose(&self) {
            self.dispose_template();
        }
    }

    impl WidgetImpl for ItemRow {}

    impl ItemRow {
        fn set_title(&self, title: String) {
            let obj = self.obj();

            if title == obj.title() {
                return;
            }

            self.title.replace(title);
            obj.update_title_label();
            obj.notify_title();
        }

        fn set_warning_tooltip_text(&self, warning_tooltip_text: String) {
            let obj = self.obj();

            if warning_tooltip_text == obj.warning_tooltip_text() {
                return;
            }

            self.warning_tooltip_text.replace(warning_tooltip_text);
            obj.update_warning_icon_tooltip_text();
            obj.notify_warning_tooltip_text();
        }

        fn set_shows_warning_icon(&self, shows_warning_icon: bool) {
            let obj = self.obj();

            if shows_warning_icon == obj.shows_warning_icon() {
                return;
            }

            self.shows_warning_icon.set(shows_warning_icon);
            obj.update_warning_icons_visibility();
            obj.notify_shows_warning_icon();
        }

        fn set_is_on_popover(&self, is_on_popover: bool) {
            let obj = self.obj();

            if is_on_popover == obj.is_on_popover() {
                return;
            }

            self.is_on_popover.set(is_on_popover);
            obj.update_selected_icon_visibility();
            obj.update_warning_icons_visibility();
            obj.notify_is_on_popover();
        }

        fn set_is_selected(&self, is_selected: bool) {
            let obj = self.obj();

            if is_selected == obj.is_selected() {
                return;
            }

            self.is_selected.set(is_selected);
            obj.update_selected_icon_opacity();
            obj.notify_is_selected();
        }
    }
}

glib::wrapper! {
    pub struct ItemRow(ObjectSubclass<imp::ItemRow>)
        @extends gtk::Widget;
}

impl ItemRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn update_title_label(&self) {
        let imp = self.imp();
        imp.title_label.set_label(&self.title());
    }

    fn update_warning_icon_tooltip_text(&self) {
        let imp = self.imp();

        let warning_tooltip_text = Some(self.warning_tooltip_text()).filter(|s| !s.is_empty());
        imp.start_warning_icon
            .set_tooltip_text(warning_tooltip_text.as_deref());
        imp.end_warning_icon
            .set_tooltip_text(warning_tooltip_text.as_deref());
    }

    fn update_warning_icons_visibility(&self) {
        let imp = self.imp();

        let is_on_popover = self.is_on_popover();
        let shows_warning_icon = self.shows_warning_icon();
        imp.start_warning_icon
            .set_visible(shows_warning_icon && !is_on_popover);
        imp.end_warning_icon
            .set_visible(shows_warning_icon && is_on_popover);
    }

    fn update_selected_icon_visibility(&self) {
        let imp = self.imp();
        imp.selected_icon.set_visible(self.is_on_popover());
    }

    fn update_selected_icon_opacity(&self) {
        let imp = self.imp();
        let opacity = if self.is_selected() { 1.0 } else { 0.0 };
        imp.selected_icon.set_opacity(opacity);
    }
}
