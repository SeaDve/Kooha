use anyhow::{anyhow, Context, Result};
use gst::prelude::*;
use gtk::glib::{
    self,
    translate::{ToGlibPtr, UnsafeFrom},
};

pub trait EncodingProfileExtManual {
    fn set_element_properties(&self, element_properties: ElementProperties);
}

impl<P: IsA<gst_pbutils::EncodingProfile>> EncodingProfileExtManual for P {
    fn set_element_properties(&self, element_properties: ElementProperties) {
        unsafe {
            gst_pbutils::ffi::gst_encoding_profile_set_element_properties(
                self.as_ref().to_glib_none().0,
                element_properties.into_inner().to_glib_full(),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "KoohaElementProperties")]
pub struct ElementProperties(gst::Structure);

impl Default for ElementProperties {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl ElementProperties {
    /// Creates an `ElementProperties` builder that build into
    /// something similar to the following:
    ///
    /// element-properties-map, map = {
    ///     [openh264enc, gop-size=32, ],
    ///     [x264enc, key-int-max=32, tune=zerolatency],
    /// }
    pub fn builder() -> ElementPropertiesBuilder {
        ElementPropertiesBuilder { map: Vec::new() }
    }

    pub fn into_inner(self) -> gst::Structure {
        self.0
    }
}

#[must_use = "The builder must be built to be used"]
#[derive(Debug, Clone)]
pub struct ElementPropertiesBuilder {
    map: Vec<glib::SendValue>,
}

impl ElementPropertiesBuilder {
    /// Insert a new `element-properties-map` map item.
    pub fn item(mut self, structure: ElementFactoryPropertiesMap) -> Self {
        self.map.push(structure.into_inner().to_send_value());
        self
    }

    pub fn build(self) -> ElementProperties {
        ElementProperties(
            gst::Structure::builder("element-properties-map")
                .field("map", gst::List::from(self.map))
                .build(),
        )
    }
}

/// Wrapper around `gst::Structure` for an item
/// on a `ElementPropertiesMapBuilder`.
///
/// # Example
///
/// ```rust
/// ElementFactoryPropertiesMap::new("vp8enc")
///     .field("max-quantizer", 17)
///     .field_from_str("keyframe-mode", "disabled")
///     .field("buffer-size", 20000)
///     .field("threads", 16),
/// ```
#[must_use = "The builder must be built to be used"]
#[derive(Debug, Clone)]
pub struct ElementFactoryPropertiesMap(gst::Structure);

impl ElementFactoryPropertiesMap {
    pub fn builder(factory_name: &str) -> ElementFactoryPropertiesMapBuilder {
        ElementFactoryPropertiesMapBuilder::new(factory_name)
    }

    pub fn into_inner(self) -> gst::Structure {
        self.0
    }

    fn set_field_from_str(&mut self, property_name: &str, string: &str) -> Result<()> {
        let factory_name = self.0.name();
        let element_type = gst::ElementFactory::find(factory_name)
            .ok_or_else(|| anyhow!("Failed to find factory with name `{}`", factory_name))?
            .load()
            .with_context(|| anyhow!("Failed to load factory with name `{}`", factory_name))?
            .element_type();
        let pspec = glib::object::ObjectClass::from_type(element_type)
            .ok_or_else(|| anyhow!("Failed to create object class from type `{}`", element_type))?
            .find_property(property_name)
            .ok_or_else(|| {
                glib::bool_error!(
                    "Property `{}` not found on type `{}`",
                    property_name,
                    element_type
                )
            })?;
        let value = unsafe {
            glib::SendValue::unsafe_from(
                glib::Value::deserialize_with_pspec(string, &pspec)?.into_raw(),
            )
        };
        self.0.set_value(property_name, value);
        Ok(())
    }
}

pub struct ElementFactoryPropertiesMapBuilder {
    prop_map: ElementFactoryPropertiesMap,
}

impl ElementFactoryPropertiesMapBuilder {
    pub fn new(factory_name: &str) -> Self {
        Self {
            prop_map: ElementFactoryPropertiesMap(gst::Structure::new_empty(factory_name)),
        }
    }

    pub fn field<T>(mut self, property_name: &str, value: T) -> Self
    where
        T: ToSendValue + Sync,
    {
        self.prop_map.0.set(property_name, value);
        self
    }

    /// Parses the given string into a property of element from the
    /// given `factory_name` with type based on the property's param spec.
    ///
    /// This works similar to `GObjectExtManualGst::set_property_from_str`.
    ///
    /// Note: The property will not be set if any of `factory_name`, `property_name`
    /// or `string` is invalid.
    pub fn field_from_str(mut self, property_name: &str, string: &str) -> Self {
        if let Err(err) = self.prop_map.set_field_from_str(property_name, string) {
            tracing::error!(
                "Failed to set property `{}` to `{}`: {:?}",
                property_name,
                string,
                err
            );
        }
        self
    }

    pub fn build(self) -> ElementFactoryPropertiesMap {
        self.prop_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_properties_builder() {
        gst::init().unwrap();

        let props_map = ElementFactoryPropertiesMap::builder("vp8enc")
            .field("cq-level", 13)
            .field("resize-allowed", false)
            .build();
        let props_map_s = props_map.clone().into_inner();
        assert_eq!(props_map_s.n_fields(), 2);
        assert_eq!(props_map_s.name(), "vp8enc");
        assert_eq!(props_map_s.get::<i32>("cq-level").unwrap(), 13);
        assert!(!props_map_s.get::<bool>("resize-allowed").unwrap());

        let elem_props = ElementProperties::builder().item(props_map.clone()).build();
        assert_eq!(elem_props.0.n_fields(), 1);

        let list = elem_props.0.get::<gst::List>("map").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list.get(0).unwrap().get::<gst::Structure>().unwrap(),
            props_map.into_inner()
        );
    }

    #[test]
    fn element_factory_properties_map_field_from_str() {
        gst::init().unwrap();

        let prop_map_s = ElementFactoryPropertiesMap::builder("vp8enc")
            .field("threads", 16)
            .field_from_str("keyframe-mode", "disabled")
            .build()
            .into_inner();
        assert_eq!(prop_map_s.n_fields(), 2);
        assert_eq!(prop_map_s.name(), "vp8enc");
        assert_eq!(prop_map_s.get::<i32>("threads").unwrap(), 16);

        let keyframe_mode_value = prop_map_s.value("keyframe-mode").unwrap();
        assert!(format!("{:?}", keyframe_mode_value).starts_with("(GstVPXEncKfMode)"));
    }
}
