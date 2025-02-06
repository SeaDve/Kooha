// Based on ashpd (MIT)
// Source: https://github.com/bilelmoussaoui/ashpd/blob/49aca6ff0f20c68fc2ddb09763ed9937b002ded6/src/window_identifier/gtk4.rs

#[cfg(feature = "x11")]
mod x11;

#[cfg(feature = "wayland")]
mod wayland;

use gtk::{glib, prelude::*};

use std::fmt;

#[derive(Debug)]
pub enum WindowIdentifier {
    #[cfg(feature = "wayland")]
    Wayland(wayland::WaylandIdentifier),
    #[cfg(feature = "x11")]
    X11(x11::X11Identifier),
    None,
}

impl fmt::Display for WindowIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "wayland")]
            WindowIdentifier::Wayland(identifier) => identifier.fmt(f),
            #[cfg(feature = "x11")]
            WindowIdentifier::X11(identifier) => identifier.fmt(f),
            WindowIdentifier::None => f.write_str(""),
        }
    }
}

impl ToVariant for WindowIdentifier {
    fn to_variant(&self) -> glib::Variant {
        self.to_string().to_variant()
    }
}

impl WindowIdentifier {
    pub async fn new(native: &impl IsA<gtk::Native>) -> Self {
        let Some(surface) = native.surface() else {
            return Self::None;
        };

        #[cfg(feature = "wayland")]
        if let Some(identifier) = wayland::try_downcast(&surface).await {
            return identifier;
        }

        #[cfg(feature = "x11")]
        if let Some(identifier) = x11::try_downcast(&surface) {
            return identifier;
        }

        tracing::warn!(
            "Unhandled surface backend type: {:?}",
            surface.display().backend()
        );

        Self::None
    }
}

#[cfg(feature = "wayland")]
impl Drop for WindowIdentifier {
    fn drop(&mut self) {
        if let WindowIdentifier::Wayland(identifier) = self {
            identifier.drop();
        }
    }
}
