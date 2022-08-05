use gtk::glib;

use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq)]
pub struct ObjectPath(String);

impl glib::FromVariant for ObjectPath {
    fn from_variant(value: &glib::Variant) -> Option<Self> {
        Self::new(value.get::<String>()?.as_str())
    }
}

impl glib::StaticVariantType for ObjectPath {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        Cow::Borrowed(glib::VariantTy::OBJECT_PATH)
    }
}

impl glib::ToVariant for ObjectPath {
    fn to_variant(&self) -> glib::Variant {
        unsafe {
            glib::translate::from_glib_none(glib::ffi::g_variant_new_object_path(
                glib::translate::ToGlibPtr::to_glib_none(&self.0).0,
            ))
        }
    }
}

impl ObjectPath {
    pub fn new(string: &str) -> Option<Self> {
        if !glib::Variant::is_object_path(string) {
            tracing::warn!("Invalid object path `{}`", string);
            return None;
        }

        Some(Self(string.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
