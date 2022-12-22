use anyhow::{anyhow, Result};
use gtk::glib::{self, FromVariant, StaticVariantType, ToVariant};

use std::{borrow::Cow, collections::HashMap, fmt};

#[derive(Default)]
pub struct VariantDict(HashMap<String, glib::Variant>);

impl fmt::Debug for VariantDict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.to_variant(), f)
    }
}

impl VariantDict {
    pub fn builder() -> VariantDictBuilder {
        VariantDictBuilder { h: HashMap::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn insert(&mut self, key: &str, value: impl ToVariant) {
        self.0.insert(key.to_string(), value.to_variant());
    }

    pub fn get_flatten<T: FromVariant>(&self, key: &str) -> Result<T> {
        let variant = self
            .0
            .get(key)
            .ok_or_else(|| anyhow!("Key `{}` not found", key))?;

        variant.get::<T>().ok_or_else(|| {
            anyhow!(
                "Expected key `{}` of type `{}`; got `{}` with value `{}`",
                key,
                T::static_variant_type(),
                variant.type_(),
                variant
            )
        })
    }

    pub fn get<T: FromVariant>(&self, key: &str) -> Result<Option<T>> {
        let Some(variant) = self.0.get(key) else {
            return Ok(None);
        };

        let value = variant.get::<T>().ok_or_else(|| {
            anyhow!(
                "Expected key `{}` of type `{}`; got `{}` with value `{}`",
                key,
                T::static_variant_type(),
                variant.type_(),
                variant
            )
        })?;

        Ok(Some(value))
    }
}

impl StaticVariantType for VariantDict {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        Cow::Borrowed(glib::VariantTy::VARDICT)
    }
}

impl FromVariant for VariantDict {
    fn from_variant(value: &glib::Variant) -> Option<Self> {
        Some(Self(value.get::<HashMap<String, glib::Variant>>()?))
    }
}

impl ToVariant for VariantDict {
    fn to_variant(&self) -> glib::Variant {
        self.0.to_variant()
    }
}

pub struct VariantDictBuilder {
    h: HashMap<String, glib::Variant>,
}

impl VariantDictBuilder {
    pub fn entry(mut self, key: &str, value: impl ToVariant) -> Self {
        self.h.insert(key.into(), value.to_variant());
        self
    }

    pub fn build(self) -> VariantDict {
        VariantDict(self.h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let var_dict_a = VariantDict::default();
        assert!(var_dict_a.is_empty());

        let var_dict_b = VariantDict::builder().entry("test", "value").build();
        assert!(!var_dict_b.is_empty());
    }

    #[test]
    fn get_flatten_ok() {
        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(var_dict.get_flatten::<String>("test").unwrap(), "value");
    }

    #[test]
    fn get_flatten_missing() {
        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(
            var_dict
                .get_flatten::<String>("test2")
                .unwrap_err()
                .to_string(),
            "Key `test2` not found"
        );
    }

    #[test]
    fn get_flatten_wrong_type() {
        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(
            var_dict.get_flatten::<u32>("test").unwrap_err().to_string(),
            "Expected key `test` of type `u`; got `s` with value `'value'`"
        );
    }

    #[test]
    fn get_ok() {
        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(
            var_dict.get::<String>("test").unwrap().as_deref(),
            Some("value")
        );
    }

    #[test]
    fn get_missing() {
        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(var_dict.get::<String>("test2").unwrap(), None);
    }

    #[test]
    fn get_wrong_type() {
        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(
            var_dict.get::<u32>("test").unwrap_err().to_string(),
            "Expected key `test` of type `u`; got `s` with value `'value'`"
        );
    }

    #[test]
    fn static_variant_type() {
        assert_eq!(
            VariantDict::default().to_variant().type_(),
            glib::VariantTy::VARDICT
        );
    }

    #[test]
    fn from_variant() {
        let var = glib::Variant::parse(None, "{'test': <'value'>}").unwrap();
        let var_dict = VariantDict::from_variant(&var).unwrap();
        assert_eq!(var_dict.get_flatten::<String>("test").unwrap(), "value");
    }

    #[test]
    fn to_variant() {
        assert_eq!(VariantDict::static_variant_type(), glib::VariantTy::VARDICT);

        let var_dict = VariantDict::builder().entry("test", "value").build();
        assert_eq!(var_dict.to_variant().to_string(), "{'test': <'value'>}");
    }

    #[test]
    fn builder() {
        let var_dict = VariantDict::builder()
            .entry("test", "value")
            .entry("test2", "value2")
            .build();
        assert_eq!(var_dict.get_flatten::<String>("test").unwrap(), "value");
        assert_eq!(var_dict.get_flatten::<String>("test2").unwrap(), "value2");
    }
}
