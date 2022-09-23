use adw::{prelude::*, subclass::prelude::*};
use gettextrs::{gettext, ngettext};
use gst::prelude::*;
use gtk::{
    gio,
    glib::{self, clone, closure},
};
use once_cell::unsync::OnceCell;

use std::cell::RefCell;

use crate::{
    element_factory_profile::ElementFactoryProfile, profile::Profile,
    profile_manager::ProfileManager, profile_tile::ProfileTile,
};

mod imp {
    use super::*;
    use gtk::CompositeTemplate;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/profile-window.ui")]
    pub struct ProfileWindow {
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) profiles_box: TemplateChild<gtk::FlowBox>,
        #[template_child]
        pub(super) name_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) file_extension_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) muxer_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) video_encoder_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub(super) audio_encoder_row: TemplateChild<adw::ComboRow>,

        pub(super) model: RefCell<Option<ProfileManager>>,

        pub(super) profile_purgatory: RefCell<Vec<Profile>>,
        pub(super) undo_delete_toast: RefCell<Option<adw::Toast>>,

        pub(super) encoder_filter: OnceCell<gtk::BoolFilter>,
        pub(super) model_signal_handler_ids: RefCell<Vec<glib::SignalHandlerId>>,

        pub(super) name_row_binding: RefCell<Option<glib::Binding>>,
        pub(super) file_extension_row_binding: RefCell<Option<glib::Binding>>,
        pub(super) muxer_row_handler_id: OnceCell<glib::SignalHandlerId>,
        pub(super) video_encoder_row_handler_id: OnceCell<glib::SignalHandlerId>,
        pub(super) audio_encoder_row_handler_id: OnceCell<glib::SignalHandlerId>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileWindow {
        const NAME: &'static str = "KoohaProfileWindow";
        type Type = super::ProfileWindow;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);

            klass.install_action("profile-window.new-profile", None, |obj, _, _| {
                if let Some(model) = obj.model() {
                    model.set_active_profile(Some(&Profile::new("New Profile")));
                } else {
                    tracing::warn!("Found no model!");
                }
            });

            klass.install_action("undo-delete-toast.undo", None, |obj, _, _| {
                if let Some(model) = obj.model() {
                    for profile in obj.imp().profile_purgatory.take() {
                        model.add_profile(profile);
                    }
                } else {
                    tracing::warn!("Found no model!");
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProfileWindow {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    // Profile model
                    glib::ParamSpecObject::builder("model", ProfileManager::static_type())
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
                "model" => {
                    let model = value.get().unwrap();
                    obj.set_model(model);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "model" => obj.model().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            self.profiles_box
                .connect_child_activated(clone!(@weak obj => move |_, profile_tile| {
                    let profile_tile = profile_tile.downcast_ref::<ProfileTile>().unwrap();
                    let profile = profile_tile.profile();

                    obj.model().unwrap().set_active_profile(profile.as_ref());
                }));

            let element_factory_name_expression =
                gtk::ClosureExpression::new::<String, &[gtk::Expression], _>(
                    &[],
                    closure!(|element_factory: &gst::ElementFactory| {
                        element_factory
                            .metadata(&gst::ELEMENT_METADATA_LONGNAME)
                            .map_or_else(|| element_factory.name(), glib::GString::from)
                    }),
                );
            self.muxer_row
                .set_expression(Some(&element_factory_name_expression));
            self.video_encoder_row
                .set_expression(Some(&element_factory_name_expression));
            self.audio_encoder_row
                .set_expression(Some(&element_factory_name_expression));

            self.muxer_row
                .connect_selected_notify(clone!(@weak obj => move |_| {
                    obj.encoder_filter().changed(gtk::FilterChange::Different);
                }));

            self.muxer_row_handler_id.set(self.muxer_row
                .connect_selected_notify(clone!(@weak obj => move |row| {
                    if let Some(selected_item) = row.selected_item() {
                        if let Some(profile) = obj.model().and_then(|model| model.active_profile()) {
                            let element_factory = selected_item.downcast::<gst::ElementFactory>().unwrap();
                            let element_factory_name = element_factory.name();
                            profile.set_muxer_profile(Some(ElementFactoryProfile::new(&element_factory_name)));
                        } else {
                            tracing::warn!("No model or active profile found but selected an element");
                        }
                    }
                }))).unwrap();
            self.video_encoder_row_handler_id.set(self.video_encoder_row
                .connect_selected_notify(clone!(@weak obj => move |row| {
                    if let Some(selected_item) = row.selected_item() {
                        if let Some(profile) = obj.model().and_then(|model| model.active_profile()) {
                            let element_factory = selected_item.downcast::<gst::ElementFactory>().unwrap();
                            let element_factory_name = element_factory.name();
                            profile.set_video_encoder_profile(Some(ElementFactoryProfile::new(&element_factory_name)));
                        } else {
                            tracing::warn!("No model or active profile found but selected an element");
                        }
                    }
                }))).unwrap();
            self.audio_encoder_row_handler_id.set(self.audio_encoder_row
                .connect_selected_notify(clone!(@weak obj => move |row| {
                    if let Some(selected_item) = row.selected_item() {
                        if let Some(profile) = obj.model().and_then(|model| model.active_profile()) {
                            let element_factory = selected_item.downcast::<gst::ElementFactory>().unwrap();
                            let element_factory_name = element_factory.name();
                            profile.set_audio_encoder_profile(Some(ElementFactoryProfile::new(&element_factory_name)));
                        } else {
                            tracing::warn!("No model or active profile found but selected an element");
                        }
                    }
                }))).unwrap();
        }

        fn dispose(&self, obj: &Self::Type) {
            obj.disconnect_model_signals();
        }
    }

    impl WidgetImpl for ProfileWindow {}
    impl WindowImpl for ProfileWindow {}
    impl AdwWindowImpl for ProfileWindow {}
}

glib::wrapper! {
    pub struct ProfileWindow(ObjectSubclass<imp::ProfileWindow>)
        @extends gtk::Widget, gtk::Window, adw::Window;
}

impl ProfileWindow {
    pub fn new(model: &ProfileManager) -> Self {
        glib::Object::new(&[("model", model)]).expect("Failed to create ProfileWindow.")
    }

    pub fn set_model(&self, model: Option<&ProfileManager>) {
        if model == self.model().as_ref() {
            return;
        }

        self.disconnect_model_signals();

        let imp = self.imp();
        imp.model.replace(model.cloned());

        imp.profiles_box.bind_model(
            model,
            clone!(@weak self as obj => @default-panic, move |profile| {
                obj.create_profile_tile(profile.downcast_ref::<Profile>().unwrap()).upcast()
            }),
        );

        imp.muxer_row
            .block_signal(imp.muxer_row_handler_id.get().unwrap());
        imp.video_encoder_row
            .block_signal(imp.video_encoder_row_handler_id.get().unwrap());
        imp.audio_encoder_row
            .block_signal(imp.audio_encoder_row_handler_id.get().unwrap());

        imp.muxer_row
            .set_model(model.map(|model| model.known_muxers()));
        imp.video_encoder_row.set_model(
            model
                .map(|model| {
                    gtk::FilterListModel::new(
                        Some(model.known_video_encoders()),
                        Some(self.encoder_filter()),
                    )
                })
                .as_ref(),
        );
        imp.audio_encoder_row.set_model(
            model
                .map(|model| {
                    gtk::FilterListModel::new(
                        Some(model.known_audio_encoders()),
                        Some(self.encoder_filter()),
                    )
                })
                .as_ref(),
        );

        imp.muxer_row
            .unblock_signal(imp.muxer_row_handler_id.get().unwrap());
        imp.video_encoder_row
            .unblock_signal(imp.video_encoder_row_handler_id.get().unwrap());
        imp.audio_encoder_row
            .unblock_signal(imp.audio_encoder_row_handler_id.get().unwrap());

        if let Some(model) = model {
            self.add_model_signal(model.connect_active_profile_notify(
                clone!(@weak self as obj => move |_| {
                    obj.update_rows();
                }),
            ));
        }

        self.update_rows();

        self.notify("model");
    }

    pub fn model(&self) -> Option<ProfileManager> {
        self.imp().model.borrow().clone()
    }

    fn encoder_filter(&self) -> &gtk::BoolFilter {
        let imp = self.imp();
        imp.encoder_filter.get_or_init(|| {
            let closure = closure!(
                |encoder: &gst::ElementFactory, selected_muxer: Option<&glib::Object>| {
                    selected_muxer.map_or(false, |muxer| {
                        let muxer = muxer.downcast_ref::<gst::ElementFactory>().unwrap();
                        encoder.static_pad_templates().any(|template| {
                            template.direction() == gst::PadDirection::Src
                                && muxer.can_sink_any_caps(&template.caps())
                        })
                    })
                }
            );

            gtk::BoolFilter::new(Some(&gtk::ClosureExpression::new::<bool, _, _>(
                &[imp.muxer_row.property_expression("selected-item")],
                closure,
            )))
        })
    }

    fn show_undo_delete_toast(&self) {
        let imp = self.imp();

        if imp.undo_delete_toast.borrow().is_none() {
            let toast = adw::Toast::builder()
                .priority(adw::ToastPriority::High)
                .button_label(&gettext("_Undo"))
                .action_name("undo-delete-toast.undo")
                .build();

            toast.connect_dismissed(clone!(@weak self as obj => move |_| {
                let imp = obj.imp();
                imp.profile_purgatory.borrow_mut().clear();
                imp.undo_delete_toast.take();
            }));

            imp.toast_overlay.add_toast(&toast);
            imp.undo_delete_toast.replace(Some(toast));
        }

        // Add this point we should already have a toast setup
        if let Some(ref toast) = *imp.undo_delete_toast.borrow() {
            let n_removed = imp.profile_purgatory.borrow().len();
            toast.set_title(&ngettext!(
                "Removed {} profile",
                "Removed {} profiles",
                n_removed as u32,
                n_removed
            ));
        }
    }

    fn create_profile_tile(&self, profile: &Profile) -> ProfileTile {
        let profile_tile = ProfileTile::new(profile);

        let model = self.model().unwrap();

        if model.active_profile().as_ref() == Some(profile) {
            profile_tile.set_selected(true);
        }

        self.add_model_signal(model.connect_active_profile_notify(
            clone!(@weak profile_tile => move |model| {
                profile_tile.set_selected(profile_tile.profile() == model.active_profile());
            }),
        ));

        profile_tile.connect_delete_request(clone!(@weak self as obj => move |profile_tile| {
            let to_remove = profile_tile.profile().unwrap();
            if obj.model().unwrap().remove_profile(&to_remove) {
                obj.imp().profile_purgatory.borrow_mut().push(to_remove);
                obj.show_undo_delete_toast();
            }
        }));

        profile_tile.connect_copy_request(clone!(@weak self as obj => move |profile_tile| {
            let original =  profile_tile.profile().unwrap();
            let copy = Profile::new_from(&original, &gettext!("{} (copy)", original.name()));
            obj.model().unwrap().set_active_profile(Some(&copy));
        }));

        profile_tile
    }

    fn add_model_signal(&self, handler_id: glib::SignalHandlerId) {
        self.imp()
            .model_signal_handler_ids
            .borrow_mut()
            .push(handler_id);
    }

    fn disconnect_model_signals(&self) {
        for handler_id in self.imp().model_signal_handler_ids.borrow_mut().drain(..) {
            if let Some(model) = self.model() {
                model.disconnect(handler_id);
            } else {
                tracing::warn!("Model removed before disconnecting signals!");
            }
        }
    }

    fn update_rows(&self) {
        let imp = self.imp();

        if let Some(binding) = imp.name_row_binding.take() {
            binding.unbind();
        }
        if let Some(binding) = imp.file_extension_row_binding.take() {
            binding.unbind();
        }

        let active_profile = self.model().and_then(|model| model.active_profile());
        let has_active_profile = active_profile.is_some();

        imp.name_row.set_visible(has_active_profile);
        imp.file_extension_row.set_visible(has_active_profile);
        imp.muxer_row.set_visible(has_active_profile);
        imp.video_encoder_row.set_visible(has_active_profile);
        imp.audio_encoder_row.set_visible(has_active_profile);

        let active_profile = if let Some(profile) = active_profile {
            profile
        } else {
            return;
        };

        imp.name_row_binding.replace(Some(
            active_profile
                .bind_property("name", &imp.name_row.get(), "text")
                .flags(glib::BindingFlags::SYNC_CREATE | glib::BindingFlags::BIDIRECTIONAL)
                .build(),
        ));
        imp.file_extension_row_binding.replace(Some(
            active_profile
                .bind_property("file-extension", &imp.file_extension_row.get(), "text")
                .transform_to(|_: &glib::Binding, value: &glib::Value| {
                    let file_extension = value.get::<Option<String>>().unwrap();
                    Some(file_extension.unwrap_or_default().to_value())
                })
                .flags(glib::BindingFlags::SYNC_CREATE | glib::BindingFlags::BIDIRECTIONAL)
                .build(),
        ));

        imp.muxer_row
            .block_signal(imp.muxer_row_handler_id.get().unwrap());
        imp.video_encoder_row
            .block_signal(imp.video_encoder_row_handler_id.get().unwrap());
        imp.audio_encoder_row
            .block_signal(imp.audio_encoder_row_handler_id.get().unwrap());

        set_selected_item(&imp.muxer_row.get(), |item: gst::ElementFactory| {
            active_profile
                .muxer_profile()
                .and_then(|p| p.factory().ok().cloned())
                .map_or(false, |factory| factory == item)
        });
        set_selected_item(&imp.video_encoder_row.get(), |item: gst::ElementFactory| {
            active_profile
                .video_encoder_profile()
                .and_then(|p| p.factory().ok().cloned())
                .map_or(false, |factory| factory == item)
        });
        set_selected_item(&imp.audio_encoder_row.get(), |item: gst::ElementFactory| {
            active_profile
                .audio_encoder_profile()
                .and_then(|p| p.factory().ok().cloned())
                .map_or(false, |factory| factory == item)
        });

        imp.muxer_row
            .unblock_signal(imp.muxer_row_handler_id.get().unwrap());
        imp.video_encoder_row
            .unblock_signal(imp.video_encoder_row_handler_id.get().unwrap());
        imp.audio_encoder_row
            .unblock_signal(imp.audio_encoder_row_handler_id.get().unwrap());
    }
}

fn set_selected_item<I: IsA<glib::Object>>(combo: &adw::ComboRow, func: impl Fn(I) -> bool) {
    fn find<I: IsA<glib::Object>>(model: &gio::ListModel, func: impl Fn(I) -> bool) -> Option<u32> {
        for i in 0..model.n_items() {
            if func(model.item(i).unwrap().downcast().unwrap()) {
                return Some(i);
            }
        }
        None
    }

    combo.set_selected(find(&combo.model().unwrap(), func).unwrap_or(gtk::INVALID_LIST_POSITION));
}
