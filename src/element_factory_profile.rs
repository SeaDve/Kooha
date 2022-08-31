use anyhow::{anyhow, ensure, Context, Result};
use gst::prelude::*;
use gtk::glib::{
    self,
    translate::{ToGlibPtr, UnsafeFrom},
    ToSendValue,
};
use once_cell::unsync::OnceCell;

use std::cell::RefCell;

pub trait EncodingProfileExtManual {
    fn set_element_properties(&self, element_properties: gst::Structure);
}

impl<P: IsA<gst_pbutils::EncodingProfile>> EncodingProfileExtManual for P {
    fn set_element_properties(&self, element_properties: gst::Structure) {
        unsafe {
            gst_pbutils::ffi::gst_encoding_profile_set_element_properties(
                self.as_ref().to_glib_none().0,
                element_properties.into_ptr(),
            );
        }
    }
}

#[derive(Debug, Clone, glib::Boxed)]
#[boxed_type(name = "KoohaElementFactoryProfile", nullable)]
pub struct ElementFactoryProfile {
    structure: gst::Structure,
    factory: OnceCell<gst::ElementFactory>,
    format: OnceCell<gst::Caps>,
    format_fields: RefCell<Option<Vec<(String, glib::SendValue)>>>,
}

impl PartialEq for ElementFactoryProfile {
    fn eq(&self, other: &Self) -> bool {
        self.structure == other.structure && self.format == other.format
    }
}

impl Eq for ElementFactoryProfile {}

impl ElementFactoryProfile {
    pub fn new(factory_name: &str) -> Self {
        Self::builder(factory_name).build()
    }

    pub fn builder(factory_name: &str) -> ElementFactoryProfileBuilder<'_> {
        ElementFactoryProfileBuilder::new(factory_name)
    }

    pub fn factory_name(&self) -> &str {
        self.structure.name()
    }

    pub fn factory(&self) -> Result<&gst::ElementFactory> {
        self.factory
            .get_or_try_init(|| find_element_factory(self.factory_name()))
    }

    pub fn format(&self) -> Result<&gst::Caps> {
        if let Some(caps) = self.format.get() {
            return Ok(caps);
        }

        let factory = self.factory()?;
        let format = profile_format_from_factory(factory, self.format_fields.take().unwrap())?;
        Ok(self.format.try_insert(format).unwrap())
    }

    pub fn element_properties(&self) -> gst::Structure {
        gst::Structure::builder("element-properties-map")
            .field("map", gst::List::from(vec![self.structure.to_send_value()]))
            .build()
    }
}

fn profile_format_from_factory(
    factory: &gst::ElementFactory,
    values: Vec<(String, glib::SendValue)>,
) -> Result<gst::Caps> {
    let factory_name = factory.name();

    ensure!(
        factory.has_type(gst::ElementFactoryType::ENCODER | gst::ElementFactoryType::MUXER),
        "Factory `{}` must be an encoder or muxer to be used in a profile",
        factory_name
    );

    for template in factory.static_pad_templates() {
        if template.direction() == gst::PadDirection::Src {
            let template_caps = template.caps();
            if let Some(structure) = template_caps.structure(0) {
                let mut structure = structure.to_owned();

                for (f, v) in values {
                    structure.set_value(&f, v);
                }

                let mut caps = gst::Caps::new_empty();
                caps.get_mut().unwrap().append_structure(structure);
                return Ok(caps);
            }
        }
    }

    Err(anyhow!(
        "Failed to find profile format for factory `{}`",
        factory_name
    ))
}

fn find_element_factory(factory_name: &str) -> Result<gst::ElementFactory> {
    gst::ElementFactory::find(factory_name)
        .ok_or_else(|| anyhow!("`{}` factory not found", factory_name))
}

pub struct ElementFactoryProfileBuilder<'a> {
    factory_name: &'a str,
    element_properties: Vec<(&'a str, glib::SendValue)>,
    format_fields: Vec<(&'a str, glib::SendValue)>,
}

impl<'a> ElementFactoryProfileBuilder<'a> {
    pub fn new(factory_name: &'a str) -> Self {
        Self {
            factory_name,
            element_properties: Vec::new(),
            format_fields: Vec::new(),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn format_field<T>(mut self, field: &'a str, value: T) -> Self
    where
        T: ToSendValue + Sync,
    {
        self.format_fields.push((field, value.to_send_value()));
        self
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn property<T>(mut self, property_name: &'a str, value: T) -> Self
    where
        T: ToSendValue + Sync,
    {
        self.element_properties
            .push((property_name, value.to_send_value()));
        self
    }

    /// Parses the given string into a property of element from the
    /// given `factory_name` with type based on the property's param spec.
    ///
    /// This works similar to `GObjectExtManualGst::set_property_from_str`.
    ///
    /// Note: The property will not be set if any of `factory_name`, `property_name`
    /// or `string` is invalid.
    pub fn property_from_str(mut self, property_name: &'a str, value_string: &str) -> Self {
        match value_from_str(self.factory_name, property_name, value_string) {
            Ok(value) => self.element_properties.push((property_name, value)),
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
        ElementFactoryProfile {
            structure: gst::Structure::from_iter(self.factory_name, self.element_properties),
            factory: OnceCell::new(),
            format: OnceCell::new(),
            format_fields: RefCell::new(Some(
                self.format_fields
                    .iter()
                    .map(|(k, v)| (str::to_string(k), v.to_send_value()))
                    .collect(),
            )),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_properties() {
        let profile = ElementFactoryProfile::new("vp8enc");
        let element_properties = profile.element_properties();
        assert_eq!(element_properties.name(), "element-properties-map");
        assert_eq!(
            element_properties
                .get::<gst::List>("map")
                .unwrap()
                .get(0)
                .unwrap()
                .get::<gst::Structure>(),
            Ok(profile.structure)
        );
    }

    #[test]
    fn test_profile_format_from_factory() {
        #[track_caller]
        fn profile_format_from_factory_name(factory_name: &str) -> Result<gst::Caps> {
            profile_format_from_factory(&find_element_factory(factory_name).unwrap(), Vec::new())
        }

        assert!(profile_format_from_factory_name("vp8enc")
            .unwrap()
            .can_intersect(&gst::Caps::builder("video/x-vp8").build()));
        assert!(profile_format_from_factory_name("opusenc")
            .unwrap()
            .can_intersect(&gst::Caps::builder("audio/x-opus").build()));
        assert!(profile_format_from_factory_name("matroskamux")
            .unwrap()
            .can_intersect(&gst::Caps::builder("video/x-matroska").build()));
        assert!(!profile_format_from_factory_name("matroskamux")
            .unwrap()
            .can_intersect(&gst::Caps::builder("video/x-vp8").build()),);
        assert_eq!(
            profile_format_from_factory_name("audioconvert")
                .unwrap_err()
                .to_string(),
            "Factory `audioconvert` must be an encoder or muxer to be used in a profile"
        );
    }

    #[test]
    fn builder() {
        gst::init().unwrap();

        let profile = ElementFactoryProfile::builder("vp8enc")
            .property("cq-level", 13)
            .property("resize-allowed", false)
            .build();
        assert_eq!(profile.structure.n_fields(), 2);
        assert_eq!(profile.structure.name(), "vp8enc");
        assert_eq!(profile.structure.get::<i32>("cq-level").unwrap(), 13);
        assert!(!profile.structure.get::<bool>("resize-allowed").unwrap());
    }

    #[test]
    fn builder_field_from_str() {
        gst::init().unwrap();

        let profile = ElementFactoryProfile::builder("vp8enc")
            .property("threads", 16)
            .property_from_str("keyframe-mode", "disabled")
            .build();
        assert_eq!(profile.structure.n_fields(), 2);
        assert_eq!(profile.structure.name(), "vp8enc");
        assert_eq!(profile.structure.get::<i32>("threads").unwrap(), 16);

        let keyframe_mode_value = profile.structure.value("keyframe-mode").unwrap();
        assert!(format!("{:?}", keyframe_mode_value).starts_with("(GstVPXEncKfMode)"));
    }
}
