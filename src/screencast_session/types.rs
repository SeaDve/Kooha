use gtk::glib::{self, bitflags::bitflags, FromVariant, StaticVariantType};

use std::borrow::Cow;

use super::VariantDict;

bitflags! {
    pub struct CursorMode: u32 {
        const HIDDEN = 1;
        const EMBEDDED = 2;
        const METADATA = 4;
    }
}

bitflags! {
    pub struct SourceType: u32 {
        const MONITOR = 1;
        const WINDOW = 2;
        const VIRTUAL = 4;
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum PersistMode {
    DoNot = 0,
    Application = 1,
    ExplicitlyRevoked = 2,
}

type StreamVariantType = (u32, VariantDict);

#[derive(Debug, Clone)]
pub struct Stream {
    node_id: u32,
    id: Option<String>,
    position: Option<(i32, i32)>,
    size: Option<(i32, i32)>,
    source_type: Option<SourceType>,
}

impl Stream {
    pub fn node_id(&self) -> u32 {
        self.node_id
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn position(&self) -> Option<(i32, i32)> {
        self.position
    }

    pub fn size(&self) -> Option<(i32, i32)> {
        self.size
    }

    pub fn source_type(&self) -> Option<SourceType> {
        self.source_type
    }
}

impl StaticVariantType for Stream {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        <StreamVariantType>::static_variant_type()
    }
}

impl FromVariant for Stream {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let (node_id, props) = variant.get::<StreamVariantType>()?;
        Some(Self {
            node_id,
            id: props.get_flatten("id").ok(),
            position: props.get_flatten("position").ok(),
            size: props.get_flatten("size").ok(),
            source_type: props
                .get_flatten::<u32>("source_type")
                .ok()
                .and_then(SourceType::from_bits),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_static_variant_type() {
        assert_eq!(
            Stream::static_variant_type(),
            glib::VariantTy::new("(ua{sv})").unwrap()
        );
    }

    #[test]
    fn stream_from_variant() {
        let variant = glib::Variant::parse(None, "(uint32 63, {'id': <'0'>, 'source_type': <uint32 1>, 'position': <(2, 2)>, 'size': <(1680, 1050)>})").unwrap();
        assert_eq!(variant.type_(), Stream::static_variant_type());

        let stream = variant.get::<Stream>().unwrap();
        assert_eq!(stream.node_id(), 63);
        assert_eq!(stream.id(), Some("0"));
        assert_eq!(stream.position(), Some((2, 2)));
        assert_eq!(stream.size(), Some((1680, 1050)));
        assert_eq!(stream.source_type(), Some(SourceType::MONITOR));
    }

    #[test]
    fn stream_from_variant_optional() {
        let variant =
            glib::Variant::parse(Some(&Stream::static_variant_type()), "(uint32 63, {})").unwrap();

        let stream = variant.get::<Stream>().unwrap();
        assert_eq!(stream.node_id(), 63);
        assert_eq!(stream.id(), None);
        assert_eq!(stream.position(), None);
        assert_eq!(stream.size(), None);
        assert_eq!(stream.source_type(), None);
    }
}
