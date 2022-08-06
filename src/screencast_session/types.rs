use gtk::glib::{self, bitflags::bitflags};

use std::collections::HashMap;

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

#[allow(dead_code)]
#[derive(Debug)]
pub struct Stream {
    node_id: u32,
    id: Option<String>,
    position: Option<(i32, i32)>,
    size: Option<(i32, i32)>,
    source_type: Option<SourceType>,
}

#[allow(dead_code)]
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

impl glib::FromVariant for Stream {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let (node_id, props) = variant.get::<(u32, HashMap<String, glib::Variant>)>()?;
        Some(Self {
            node_id,
            id: props.get("id").and_then(|id| id.get()),
            position: props.get("position").and_then(|id| id.get()),
            size: props.get("size").and_then(|id| id.get()),
            source_type: props
                .get("source_type")
                .and_then(|source_type| source_type.get::<u32>())
                .and_then(SourceType::from_bits),
        })
    }
}

impl glib::StaticVariantType for Stream {
    fn static_variant_type() -> std::borrow::Cow<'static, glib::VariantTy> {
        <(u32, HashMap<String, glib::Variant>)>::static_variant_type()
    }
}
