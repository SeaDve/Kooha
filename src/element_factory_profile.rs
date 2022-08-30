use anyhow::{anyhow, Context, Result};
use gst::prelude::*;
use gtk::glib::{
    self,
    translate::{ToGlibPtr, UnsafeFrom},
    ToSendValue,
};

pub trait EncodingProfileExtManual {
    fn set_element_properties(&self, element_properties: gst::Structure);
}

impl<P: IsA<gst_pbutils::EncodingProfile>> EncodingProfileExtManual for P {
    fn set_element_properties(&self, element_properties: gst::Structure) {
        unsafe {
            gst_pbutils::ffi::gst_encoding_profile_set_element_properties(
                self.as_ref().to_glib_none().0,
                element_properties.to_glib_full(),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, glib::Boxed)]
#[boxed_type(name = "KoohaElementFactoryProfile", nullable)]
pub struct ElementFactoryProfile(gst::Structure);

impl ElementFactoryProfile {
    pub fn new(factory_name: &str) -> Self {
        Self::builder(factory_name).build()
    }

    pub fn builder(factory_name: &str) -> ElementFactoryProfileBuilder {
        ElementFactoryProfileBuilder {
            structure: gst::Structure::new_empty(factory_name),
        }
    }

    pub fn factory_name(&self) -> &str {
        self.0.name()
    }

    pub fn into_element_properties(self) -> gst::Structure {
        gst::Structure::builder("element-properties-map")
            .field("map", gst::List::from(vec![self.0.to_send_value()]))
            .build()
    }
}

pub struct ElementFactoryProfileBuilder {
    structure: gst::Structure,
}

impl ElementFactoryProfileBuilder {
    pub fn field<T>(mut self, property_name: &str, value: T) -> Self
    where
        T: ToSendValue + Sync,
    {
        self.structure.set(property_name, value);
        self
    }

    /// Parses the given string into a property of element from the
    /// given `factory_name` with type based on the property's param spec.
    ///
    /// This works similar to `GObjectExtManualGst::set_property_from_str`.
    ///
    /// Note: The property will not be set if any of `factory_name`, `property_name`
    /// or `string` is invalid.
    pub fn field_from_str(mut self, property_name: &str, value_string: &str) -> Self {
        let factory_name = self.structure.name();

        match value_from_str(factory_name, property_name, value_string) {
            Ok(value) => self.structure.set_value(property_name, value),
            Err(err) => tracing::warn!(
                "Failed to set property `{}` to `{}`: {:?}",
                property_name,
                value_string,
                err
            ),
        }

        self
    }

    pub fn build(self) -> ElementFactoryProfile {
        ElementFactoryProfile(self.structure)
    }
}

fn value_from_str(
    factory_name: &str,
    property_name: &str,
    value_string: &str,
) -> Result<glib::SendValue> {
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
            glib::Value::deserialize_with_pspec(value_string, &pspec)?.into_raw(),
        )
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_element_properties() {
        let profile = ElementFactoryProfile::new("vp8enc");
        let element_properties = profile.clone().into_element_properties();
        assert_eq!(element_properties.name(), "element-properties-map");
        assert_eq!(
            element_properties
                .get::<gst::List>("map")
                .unwrap()
                .get(0)
                .unwrap()
                .get::<gst::Structure>(),
            Ok(profile.0)
        );
    }

    #[test]
    fn builder() {
        gst::init().unwrap();

        let profile = ElementFactoryProfile::builder("vp8enc")
            .field("cq-level", 13)
            .field("resize-allowed", false)
            .build();
        assert_eq!(profile.0.n_fields(), 2);
        assert_eq!(profile.0.name(), "vp8enc");
        assert_eq!(profile.0.get::<i32>("cq-level").unwrap(), 13);
        assert!(!profile.0.get::<bool>("resize-allowed").unwrap());
    }

    #[test]
    fn builder_field_from_str() {
        gst::init().unwrap();

        let profile = ElementFactoryProfile::builder("vp8enc")
            .field("threads", 16)
            .field_from_str("keyframe-mode", "disabled")
            .build();
        assert_eq!(profile.0.n_fields(), 2);
        assert_eq!(profile.0.name(), "vp8enc");
        assert_eq!(profile.0.get::<i32>("threads").unwrap(), 16);

        let keyframe_mode_value = profile.0.value("keyframe-mode").unwrap();
        assert!(format!("{:?}", keyframe_mode_value).starts_with("(GstVPXEncKfMode)"));
    }
}
