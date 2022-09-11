use anyhow::{anyhow, Context, Result};
use gst_pbutils::prelude::*;
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
                element_properties.into_inner().into_ptr(),
            );
        }
    }
}

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
    pub fn field<T: ToSendValue>(mut self, name: &str, value: T) -> Self {
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
