use anyhow::{anyhow, Context, Result};
use gst::prelude::*;
use gtk::{
    gio,
    gio::{prelude::*, subclass::prelude::*},
    glib::{self, clone},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "KoohaDeviceClass")]
pub enum DeviceClass {
    #[default]
    Source,
    Sink,
}

impl DeviceClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::Source => "Audio/Source",
            Self::Sink => "Audio/Sink",
        }
    }
}

mod imp {
    use std::cell::{OnceCell, RefCell};

    use gst::bus::BusWatchGuard;
    use indexmap::IndexSet;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::DeviceManager)]
    pub struct DeviceManager {
        #[property(get, set, construct_only, builder(DeviceClass::default()))]
        pub(super) device_class: OnceCell<DeviceClass>,
        #[property(get, set = Self::set_selected_device, explicit_notify, nullable)]
        pub(super) selected_device: RefCell<Option<gst::Device>>,

        pub(super) monitor: gst::DeviceMonitor,
        pub(super) bus_watch_guard: OnceCell<BusWatchGuard>,

        pub(super) devices: RefCell<IndexSet<gst::Device>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DeviceManager {
        const NAME: &'static str = "KoohaDeviceManager";
        type Type = super::DeviceManager;
        type Interfaces = (gio::ListModel,);
    }

    #[glib::derived_properties]
    impl ObjectImpl for DeviceManager {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            if let Err(err) = obj.init() {
                tracing::error!("Failed to initialize: {:?}", err);
            }
        }

        fn dispose(&self) {
            self.monitor.stop();
        }
    }

    impl ListModelImpl for DeviceManager {
        fn item_type(&self) -> glib::Type {
            gst::Device::static_type()
        }

        fn n_items(&self) -> u32 {
            self.devices.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.devices
                .borrow()
                .get_index(position as usize)
                .map(|d| d.upcast_ref())
                .cloned()
        }
    }

    impl DeviceManager {
        fn set_selected_device(&self, device: Option<gst::Device>) {
            let obj = self.obj();

            if device == obj.selected_device() {
                return;
            }

            if let Some(ref device) = device {
                if !self.devices.borrow().contains(device) {
                    tracing::error!("Can't set selected device to a device not in the list");
                    return;
                }
            }

            self.selected_device.replace(device);
            obj.notify_selected_device();
        }
    }
}

glib::wrapper! {
    pub struct DeviceManager(ObjectSubclass<imp::DeviceManager>)
        @implements gio::ListModel;
}

impl DeviceManager {
    pub fn new(device_class: DeviceClass) -> Self {
        glib::Object::builder()
            .property("device-class", device_class)
            .build()
    }

    fn init(&self) -> Result<()> {
        let imp = self.imp();

        imp.monitor.add_filter(
            Some(self.device_class().as_str()),
            Some(&gst::Caps::new_empty_simple("audio/x-raw")),
        );

        let bus_watch_guard = imp
            .monitor
            .bus()
            .add_watch_local(
                clone!(@weak self as obj => @default-panic,move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
            .context("Failed to add watch on bus")?;
        imp.bus_watch_guard.set(bus_watch_guard).unwrap();

        imp.monitor.start().context("Failed to start monitor")?;

        for device in imp.monitor.devices() {
            self.handle_added_device(&device);
        }

        Ok(())
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        match message.view() {
            gst::MessageView::DeviceAdded(da) => {
                tracing::debug!("Device added");

                let added = da.device();
                self.handle_added_device(&added);
            }
            gst::MessageView::DeviceRemoved(dr) => {
                tracing::debug!("Device removed");

                let removed = dr.device();
                self.handle_removed_device(&removed);
            }
            gst::MessageView::DeviceChanged(dc) => {
                tracing::debug!("Device changed");

                let (device, old_device) = dc.device_changed();
                self.handle_removed_device(&old_device);
                self.handle_added_device(&device);
            }
            other => {
                tracing::warn!("Received other message on bus: {:?}", other);
            }
        }

        glib::ControlFlow::Continue
    }

    fn handle_added_device(&self, device: &gst::Device) {
        let imp = self.imp();

        if !device.has_classes(self.device_class().as_str()) {
            tracing::debug!(
                "Skipping device `{}` as it has unknown device class `{}`",
                device.name(),
                device.device_class()
            );
            return;
        }

        let (position, was_appended) = imp.devices.borrow_mut().insert_full(device.clone());
        if was_appended {
            self.items_changed(position as u32, 0, 1);
        } else {
            self.items_changed(position as u32, 1, 1);
        }

        if device.is_default() {
            self.set_selected_device(Some(device.clone()));
        }
    }

    fn handle_removed_device(&self, device: &gst::Device) {
        let imp = self.imp();

        let entry = imp.devices.borrow_mut().shift_remove_full(device);
        if let Some((position, _)) = entry {
            self.items_changed(position as u32, 1, 0);
        }

        if self.selected_device().is_some_and(|d| &d == device) {
            self.set_selected_device(gst::Device::NONE);
        }
    }
}

pub trait KoohaDeviceExt: IsA<gst::Device> {
    fn id(&self) -> String {
        // FIXME
        self.as_ref().name().to_string()
    }

    fn create_audiosrc(&self) -> Result<gst::Element> {
        let device = self.as_ref();

        let audiosrc =
            DeviceExt::create_element(device, None).context("Failed to create element")?;

        if device.has_classes(DeviceClass::Sink.as_str()) {
            let pulsesrc = gst::ElementFactory::make("pulsesrc").build()?;

            let monitor_name = device
                .properties()
                .and_then(|p| p.get::<String>("node.name").ok())
                .or_else(|| audiosrc.property::<Option<String>>("device"))
                .context("Can't find sink device name")?;

            pulsesrc.set_property("device", format!("{}.monitor", monitor_name));

            Ok(pulsesrc)
        } else if device.has_classes(DeviceClass::Source.as_str()) {
            Ok(audiosrc)
        } else {
            Err(anyhow!("Unknown device class `{}`", device.device_class()))
        }
    }

    fn display_name(&self) -> String {
        let this = self.as_ref();

        let Some(properties) = this.properties() else {
            return this.id();
        };

        if let Ok(description) = properties.get::<String>("device.description") {
            return description;
        }

        this.id()
    }

    fn is_default(&self) -> bool {
        self.as_ref().properties().is_some_and(|p| {
            p.get::<bool>("is-default")
                .is_ok_and(|is_default| is_default)
        })
    }
}

impl<O: IsA<gst::Device>> KoohaDeviceExt for O {}
