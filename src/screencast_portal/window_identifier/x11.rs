use gtk::{gdk::Surface, prelude::*};

use std::fmt;

#[derive(Debug)]
pub struct X11Identifier(pub gdk_x11::XWindow);

impl fmt::Display for X11Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "x11:{:#x}", self.0)
    }
}

pub fn try_downcast(surface: &Surface) -> Option<super::WindowIdentifier> {
    surface
        .downcast_ref::<gdk_x11::X11Surface>()
        .map(|surface| super::WindowIdentifier::X11(X11Identifier(surface.xid())))
}
