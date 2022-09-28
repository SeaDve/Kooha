// Based on ashpd (MIT)
// Source: https://github.com/bilelmoussaoui/ashpd/blob/49aca6ff0f20c68fc2ddb09763ed9937b002ded6/src/window_identifier/gtk4.rs

use futures_channel::oneshot;
use gtk::{
    glib::{self, WeakRef},
    prelude::*,
};

use std::{cell::RefCell, fmt};

const WINDOW_HANDLE_KEY: &str = "kooha-wayland-window-handle";

pub enum WindowIdentifier {
    Wayland {
        top_level: WeakRef<gdk_wayland::WaylandToplevel>,
        handle: Option<String>,
    },
    X11(gdk_x11::XWindow),
    None,
}

impl fmt::Display for WindowIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowIdentifier::Wayland { handle, .. } => {
                write!(f, "wayland:{}", handle.as_deref().unwrap_or_default())
            }
            WindowIdentifier::X11(xid) => write!(f, "x11:{:#x}", xid),
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
        let surface = native.surface();

        if let Some(top_level) = surface.downcast_ref::<gdk_wayland::WaylandToplevel>() {
            let handle = unsafe {
                if let Some(mut handle) = top_level.data(WINDOW_HANDLE_KEY) {
                    let (handle, ref_count): &mut (Option<String>, u8) = handle.as_mut();
                    *ref_count += 1;
                    handle.clone()
                } else {
                    let (sender, receiver) = oneshot::channel();
                    let sender = RefCell::new(Some(sender));

                    let result = top_level.export_handle(move |_, handle| {
                        if let Some(m) = sender.take() {
                            let _ = m.send(handle.to_string());
                        }
                    });

                    if !result {
                        return Self::None;
                    }

                    let handle = receiver.await.ok();
                    top_level.set_data(WINDOW_HANDLE_KEY, (handle.clone(), 1));
                    handle
                }
            };

            Self::Wayland {
                top_level: top_level.downgrade(),
                handle,
            }
        } else if let Some(surface) = surface.downcast_ref::<gdk_x11::X11Surface>() {
            Self::X11(surface.xid())
        } else {
            tracing::warn!(
                "Unhandled surface backend type: {:?}",
                surface.display().backend()
            );
            Self::None
        }
    }
}

impl Drop for WindowIdentifier {
    fn drop(&mut self) {
        if let WindowIdentifier::Wayland { top_level, .. } = self {
            if let Some(top_level) = top_level.upgrade() {
                unsafe {
                    let (_handle, ref_count): &mut (Option<String>, u8) =
                        top_level.data(WINDOW_HANDLE_KEY).unwrap().as_mut();
                    if *ref_count > 1 {
                        *ref_count -= 1;
                        return;
                    }
                    top_level.unexport_handle();
                    let _ = top_level.steal_data::<(Option<String>, u8)>(WINDOW_HANDLE_KEY);
                }
            }
        }
    }
}
