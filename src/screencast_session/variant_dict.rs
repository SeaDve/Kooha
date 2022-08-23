use anyhow::{anyhow, Result};
use gtk::glib::{self, FromVariant, StaticVariantType, ToVariant};

use std::{borrow::Cow, collections::HashMap, fmt};

#[derive(Debug)]
#[must_use]
pub struct VariantDict(HashMap<String, glib::Variant>);

impl fmt::Display for VariantDict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.to_variant(), f)
    }
}

impl VariantDict {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get<T: FromVariant>(&self, key: &str) -> Result<T> {
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

    pub fn get_optional<T: FromVariant>(&self, key: &str) -> Result<Option<T>> {
        let variant = match self.0.get(key) {
            Some(variant) => variant,
            None => return Ok(None),
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

impl<'a> FromIterator<(&'a str, glib::Variant)> for VariantDict {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = (&'a str, glib::Variant)>,
    {
        Self(HashMap::from_iter(
            iter.into_iter()
                .map(|(key, variant)| (key.to_string(), variant)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let var_dict_a = VariantDict::new();
        assert!(var_dict_a.is_empty());

        let var_dict_b = VariantDict::from_iter([("test", "value".to_variant())]);
        assert!(!var_dict_b.is_empty());
    }

    #[test]
    fn get_ok() {
        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(var_dict.get::<String>("test").unwrap(), "value");
    }

    #[test]
    fn get_missing() {
        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(
            var_dict.get::<String>("test2").unwrap_err().to_string(),
            "Key `test2` not found"
        );
    }

    #[test]
    fn get_wrong_type() {
        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(
            var_dict.get::<u32>("test").unwrap_err().to_string(),
            "Expected key `test` of type `u`; got `s` with value `'value'`"
        );
    }

    #[test]
    fn get_optional_ok() {
        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(
            var_dict.get_optional::<String>("test").unwrap().as_deref(),
            Some("value")
        );
    }

    #[test]
    fn get_optional_missing() {
        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(var_dict.get_optional::<String>("test2").unwrap(), None);
    }

    #[test]
    fn get_optional_wrong_type() {
        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(
            var_dict
                .get_optional::<u32>("test")
                .unwrap_err()
                .to_string(),
            "Expected key `test` of type `u`; got `s` with value `'value'`"
        );
    }

    #[test]
    fn static_variant_type() {
        assert_eq!(
            VariantDict::new().to_variant().type_(),
            glib::VariantTy::VARDICT
        );
    }

    #[test]
    fn from_variant() {
        let var = glib::Variant::parse(None, "{'test': <'value'>}").unwrap();
        let var_dict = VariantDict::from_variant(&var).unwrap();
        assert_eq!(var_dict.get::<String>("test").unwrap(), "value");
    }

    #[test]
    fn to_variant() {
        assert_eq!(VariantDict::static_variant_type(), glib::VariantTy::VARDICT);

        let var_dict = VariantDict::from_iter([("test", "value".to_variant())]);
        assert_eq!(var_dict.to_string(), "{'test': <'value'>}");
    }

    #[test]
    fn from_iter() {
        let var_dict = VariantDict::from_iter([
            ("test", "value".to_variant()),
            ("test2", "value2".to_variant()),
        ]);
        assert_eq!(var_dict.get::<String>("test").unwrap(), "value");
        assert_eq!(var_dict.get::<String>("test2").unwrap(), "value2");
    }
}
