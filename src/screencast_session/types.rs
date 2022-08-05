use gtk::glib::{self, bitflags::bitflags};

use std::{borrow::Cow, collections::HashMap};

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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u32)]
pub enum PersistMode {
    DoNot = 0,
    // Application = 1,
    // ExplicitlyRevoked = 2,
}

#[derive(Debug)]
pub struct Stream {
    pub node_id: u32,
    pub id: String,
    pub position: (i32, i32),
    pub size: (i32, i32),
    pub source_type: SourceType,
}

impl glib::FromVariant for Stream {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let (node_id, props) = variant.get::<(u32, HashMap<String, glib::Variant>)>()?;

        Some(Self {
            node_id,
            id: props.get("id")?.get()?,
            position: props.get("position")?.get::<(i32, i32)>()?,
            size: props.get("size")?.get::<(i32, i32)>()?,
            source_type: props
                .get("source_type")?
                .get::<u32>()
                .and_then(SourceType::from_bits)?,
        })
    }
}

impl glib::StaticVariantType for Stream {
    fn static_variant_type() -> std::borrow::Cow<'static, glib::VariantTy> {
        Cow::Borrowed(glib::VariantTy::new("(ua{sv})").unwrap())
    }
}
