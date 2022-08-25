use gst::prelude::*;
use gtk::glib::{
    self,
    translate::{ToGlibPtr, UnsafeFrom},
};

use std::ops::Deref;

pub trait ElementPropertiesEncodingProfileExt {
    fn set_element_properties(&self, element_properties: ElementProperties);
}

impl<P: IsA<gst_pbutils::EncodingProfile>> ElementPropertiesEncodingProfileExt for P {
    fn set_element_properties(&self, element_properties: ElementProperties) {
        unsafe {
            gst_pbutils::ffi::gst_encoding_profile_set_element_properties(
                self.as_ref().to_glib_none().0,
                element_properties.into_inner().to_glib_full(),
            );
        }
    }
}

/// Wrapper around `gst::Structure` for `element-properties`
/// property of `EncodingProfile`.
///
/// # Examples
///
/// ```rust
/// ElementProperties::builder_general()
///     .field("threads", 16)
///     .build();
/// ```
///
/// ```rust
/// ElementProperties::builder_map()
///     .field(
///         ElementFactoryPropertiesMap::new("vp8enc")
///             .field("max-quantizer", 17)
///             .field_from_str("keyframe-mode", "disabled")
///             .field("buffer-size", 20000)
///             .field("threads", 16),
///     )
///     .build()
/// ```
#[derive(Debug, Clone)]
pub struct ElementProperties(gst::Structure);

impl Deref for ElementProperties {
    type Target = gst::StructureRef;

    fn deref(&self) -> &gst::StructureRef {
        self.0.as_ref()
    }
}

impl From<ElementProperties> for gst::Structure {
    fn from(e: ElementProperties) -> Self {
        e.into_inner()
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
    pub fn builder_map() -> ElementPropertiesMapBuilder {
        ElementPropertiesMapBuilder::new()
    }

    /// Creates an `ElementProperties` builder that build into
    /// something similar to the following:
    ///
    /// [element-properties, boolean-prop=true, string-prop="hi"]
    pub fn builder_general() -> ElementPropertiesGeneralBuilder {
        ElementPropertiesGeneralBuilder::new()
    }

    pub fn into_inner(self) -> gst::Structure {
        self.0
    }
}

#[must_use = "The builder must be built to be used"]
#[derive(Debug, Clone)]
pub struct ElementPropertiesGeneralBuilder {
    structure: gst::Structure,
}

impl Default for ElementPropertiesGeneralBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementPropertiesGeneralBuilder {
    pub fn new() -> Self {
        Self {
            structure: gst::Structure::new_empty("element-properties"),
        }
    }

    pub fn field<T>(mut self, property_name: &str, value: T) -> Self
    where
        T: ToSendValue + Sync,
    {
        self.structure.set(property_name, value);
        self
    }

    pub fn build(self) -> ElementProperties {
        ElementProperties(self.structure)
    }
}

#[must_use = "The builder must be built to be used"]
#[derive(Debug, Clone)]
pub struct ElementPropertiesMapBuilder {
    map: Vec<glib::SendValue>,
}

impl Default for ElementPropertiesMapBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementPropertiesMapBuilder {
    pub fn new() -> Self {
        Self { map: Vec::new() }
    }

    /// Insert a new `element-properties-map` map entry.
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

impl From<ElementFactoryPropertiesMap> for gst::Structure {
    fn from(e: ElementFactoryPropertiesMap) -> Self {
        e.into_inner()
    }
}

impl ElementFactoryPropertiesMap {
    pub fn new(factory_name: &str) -> Self {
        Self(gst::Structure::new_empty(factory_name))
    }

    pub fn field<T>(mut self, property_name: &str, value: T) -> Self
    where
        T: ToSendValue + Sync,
    {
        self.0.set(property_name, value);
        self
    }

    /// Parses the given string into a property of element from the
    /// given `factory_name` with type based on the property's param spec.
    ///
    /// This works similar to `GObjectExtManualGst::try_set_property_from_str`.
    pub fn field_try_from_str(
        mut self,
        property_name: &str,
        string: &str,
    ) -> Result<Self, glib::BoolError> {
        let factory_name = self.0.name();
        let element = gst::ElementFactory::make(factory_name, None).map_err(|_| {
            glib::bool_error!(
                "Failed to create element from factory name `{}`",
                factory_name
            )
        })?;
        let pspec = element.find_property(property_name).ok_or_else(|| {
            glib::bool_error!(
                "Property `{}` not found on type `{}`",
                property_name,
                element.type_()
            )
        })?;
        let value = unsafe {
            glib::SendValue::unsafe_from(
                glib::Value::deserialize_with_pspec(string, &pspec)?.into_raw(),
            )
        };
        self.0.set_value(property_name, value);
        Ok(self)
    }

    /// Parses the given string into a property of element from the
    /// given `factory_name` with type based on the property's param spec.
    ///
    /// This works similar to `GObjectExtManualGst::set_property_from_str`.
    pub fn field_from_str(self, property_name: &str, string: &str) -> Self {
        self.field_try_from_str(property_name, string).unwrap()
    }

    pub fn into_inner(self) -> gst::Structure {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_properties_general_builder() {
        let elem_props = ElementProperties::builder_general()
            .field("string-prop", "hi")
            .field("boolean-prop", true)
            .build();
        assert_eq!(elem_props.n_fields(), 2);
        assert_eq!(elem_props.name(), "element-properties");
        assert_eq!(elem_props.get::<String>("string-prop").unwrap(), "hi");
        assert!(elem_props.get::<bool>("boolean-prop").unwrap());
    }

    #[test]
    fn element_properties_map_builder() {
        let props_map = ElementFactoryPropertiesMap::new("vp8enc")
            .field("cq-level", 13)
            .field("resize-allowed", false);
        let props_map_s = props_map.clone().into_inner();
        assert_eq!(props_map_s.n_fields(), 2);
        assert_eq!(props_map_s.name(), "vp8enc");
        assert_eq!(props_map_s.get::<i32>("cq-level").unwrap(), 13);
        assert!(!props_map_s.get::<bool>("resize-allowed").unwrap());

        let elem_props = ElementProperties::builder_map()
            .item(props_map.clone())
            .build();
        assert_eq!(elem_props.n_fields(), 1);

        let list = elem_props.get::<gst::List>("map").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(
            list.get(0).unwrap().get::<gst::Structure>().unwrap(),
            gst::Structure::from(props_map)
        );
    }

    #[test]
    fn element_factory_properties_map_field_from_str() {
        let prop_map_s = ElementFactoryPropertiesMap::new("vp8enc")
            .field("threads", 16)
            .field_from_str("keyframe-mode", "disabled")
            .into_inner();
        assert_eq!(prop_map_s.n_fields(), 2);
        assert_eq!(prop_map_s.name(), "vp8enc");
        assert_eq!(prop_map_s.get::<i32>("threads").unwrap(), 16);

        let keyframe_mode_value = prop_map_s.value("keyframe-mode").unwrap();
        assert!(format!("{:?}", keyframe_mode_value).starts_with("(GstVPXEncKfMode)"));
    }
}
