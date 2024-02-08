use anyhow::{anyhow, Context, Result};
use gst_pbutils::{
    element_properties::ElementPropertiesMapItemBuilder, prelude::*, ElementProperties,
    ElementPropertiesMapItem,
};
use gtk::glib::{self, translate::UnsafeFrom};

use crate::utils;

pub struct ElementConfig {
    factory_name: String,
    properties: ElementProperties,
}

impl ElementConfig {
    pub fn builder(factory_name: &str) -> ElementConfigBuilder {
        ElementConfigBuilder::new(factory_name)
    }

    pub fn factory_name(&self) -> &str {
        &self.factory_name
    }

    pub fn properties(&self) -> &ElementProperties {
        &self.properties
    }
}

pub struct ElementConfigBuilder {
    factory_name: String,
    inner: ElementPropertiesMapItemBuilder,
}

impl ElementConfigBuilder {
    fn new(factory_name: &str) -> Self {
        Self {
            factory_name: factory_name.to_string(),
            inner: ElementPropertiesMapItem::builder(factory_name),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn field(self, name: &str, value: impl ToSendValue) -> Self {
        Self {
            inner: self.inner.field(name, value.to_send_value()),
            ..self
        }
    }

    pub fn field_from_str(self, property_name: &str, value_string: &str) -> Self {
        match value_from_str(&self.factory_name, property_name, value_string) {
            Ok(value) => Self {
                inner: self.inner.field_value(property_name, value),
                ..self
            },
            Err(err) => {
                tracing::warn!(
                    "Failed to set property `{}` to `{}`: {:?}",
                    property_name,
                    value_string,
                    err
                );
                self
            }
        }
    }

    pub fn build(self) -> ElementConfig {
        ElementConfig {
            factory_name: self.factory_name,
            properties: ElementProperties::builder_map()
                .item(self.inner.build())
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
            .first()
            .unwrap()
            .get::<gst::Structure>()
            .unwrap()
    }

    #[test]
    fn element_properties() {
        gst::init().unwrap();

        let element_properties = ElementConfigBuilder::new("vp8enc")
            .build()
            .properties()
            .clone();
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

        let element_properties = ElementConfigBuilder::new("vp8enc")
            .field("cq-level", 13)
            .field("resize-allowed", false)
            .build()
            .properties()
            .clone();
        let inner_item = element_properties_inner_item(element_properties);

        assert_eq!(inner_item.n_fields(), 2);
        assert_eq!(inner_item.name(), "vp8enc");
        assert_eq!(inner_item.get::<i32>("cq-level").unwrap(), 13);
        assert!(!inner_item.get::<bool>("resize-allowed").unwrap());
    }

    #[test]
    fn builder_field_from_str() {
        gst::init().unwrap();

        let element_properties = ElementConfigBuilder::new("vp8enc")
            .field("threads", 16)
            .field_from_str("keyframe-mode", "disabled")
            .build()
            .properties()
            .clone();
        let inner_item = element_properties_inner_item(element_properties);
        assert_eq!(inner_item.n_fields(), 2);
        assert_eq!(inner_item.name(), "vp8enc");
        assert_eq!(inner_item.get::<i32>("threads").unwrap(), 16);

        let keyframe_mode_value = inner_item.value("keyframe-mode").unwrap();
        assert!(format!("{:?}", keyframe_mode_value).starts_with("(GstVPXEncKfMode)"));
    }
}
