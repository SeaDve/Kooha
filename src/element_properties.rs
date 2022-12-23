use anyhow::{anyhow, Context, Result};
use gst_pbutils::prelude::*;
use gtk::glib::{
    self,
    translate::{IntoGlibPtr, ToGlibPtr, UnsafeFrom},
};

use crate::utils;

pub trait EncodingProfileExtManual {
    fn set_element_properties(&self, element_properties: ElementProperties);
}

impl<P: IsA<gst_pbutils::EncodingProfile>> EncodingProfileExtManual for P {
    fn set_element_properties(&self, element_properties: ElementProperties) {
        unsafe {
            gst_pbutils::ffi::gst_encoding_profile_set_element_properties(
                self.as_ref().to_glib_none().0,
                element_properties.into_inner().into_glib_ptr(),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct ElementProperties {
    factory_name: String,
    raw: gst::Structure,
}

impl ElementProperties {
    pub fn builder(factory_name: &str) -> ElementPropertiesBuilder {
        ElementPropertiesBuilder::new(factory_name)
    }

    pub fn factory_name(&self) -> &str {
        &self.factory_name
    }

    pub fn into_inner(self) -> gst::Structure {
        self.raw
    }
}

pub struct ElementPropertiesBuilder {
    s: gst::Structure,
}

impl ElementPropertiesBuilder {
    pub fn new(factory_name: &str) -> Self {
        Self {
            s: gst::Structure::new_empty(factory_name),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn field(mut self, name: &str, value: impl ToSendValue) -> Self {
        self.s.set_value(name, value.to_send_value());
        self
    }

    pub fn field_from_str(mut self, property_name: &str, value_string: &str) -> Self {
        let factory_name = self.s.name();
        match value_from_str(factory_name, property_name, value_string) {
            Ok(value) => self.s.set_value(property_name, value),
            Err(err) => tracing::warn!(
                "Failed to set property `{}` to `{}`: {:?}",
                property_name,
                value_string,
                err
            ),
        }

        self
    }

    pub fn build(self) -> ElementProperties {
        ElementProperties {
            factory_name: self.s.name().to_string(),
            raw: gst::Structure::builder("element-properties-map")
                .field("map", gst::List::from(vec![self.s.to_send_value()]))
                .build(),
        }
    }
}

fn value_from_str(
    factory_name: &str,
    property_name: &str,
    value_string: &str,
) -> Result<glib::SendValue> {
    let element_type = utils::find_element_factory(factory_name)?
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

    #[track_caller]
    fn element_properties_inner_item(element_properties: ElementProperties) -> gst::Structure {
        element_properties
            .into_inner()
            .get::<gst::List>("map")
            .unwrap()
            .get(0)
            .unwrap()
            .get::<gst::Structure>()
            .unwrap()
    }

    #[test]
    fn element_properties() {
        let element_properties = ElementProperties::builder("vp8enc").build();
        let inner_item = element_properties_inner_item(element_properties.clone());
        assert_eq!(
            element_properties.into_inner().name(),
            "element-properties-map"
        );
        assert_eq!(inner_item.name(), "vp8enc");
    }

    #[test]
    fn builder() {
        gst::init().unwrap();

        let element_properties = ElementProperties::builder("vp8enc")
            .field("cq-level", 13)
            .field("resize-allowed", false)
            .build();
        let inner_item = element_properties_inner_item(element_properties);

        assert_eq!(inner_item.n_fields(), 2);
        assert_eq!(inner_item.name(), "vp8enc");
        assert_eq!(inner_item.get::<i32>("cq-level").unwrap(), 13);
        assert!(!inner_item.get::<bool>("resize-allowed").unwrap());
    }

    #[test]
    fn builder_field_from_str() {
        gst::init().unwrap();

        let element_properties = ElementProperties::builder("vp8enc")
            .field("threads", 16)
            .field_from_str("keyframe-mode", "disabled")
            .build();
        let inner_item = element_properties_inner_item(element_properties);
        assert_eq!(inner_item.n_fields(), 2);
        assert_eq!(inner_item.name(), "vp8enc");
        assert_eq!(inner_item.get::<i32>("threads").unwrap(), 16);

        let keyframe_mode_value = inner_item.value("keyframe-mode").unwrap();
        assert!(format!("{:?}", keyframe_mode_value).starts_with("(GstVPXEncKfMode)"));
    }
}
