use futures_channel::oneshot;
use gtk::{gdk::Surface, glib::WeakRef, prelude::*};

use std::{cell::RefCell, fmt};

const WINDOW_HANDLE_KEY: &str = "kooha-wayland-window-handle";

type WindowHandleData = (Option<String>, u8);

#[derive(Debug)]
pub struct WaylandIdentifier {
    top_level: WeakRef<gdk_wayland::WaylandToplevel>,
    handle: Option<String>,
}

impl fmt::Display for WaylandIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wayland:{}", self.handle.as_deref().unwrap_or_default())
    }
}

impl WaylandIdentifier {
    pub fn drop(&mut self) {
        if self.handle.is_none() {
            return;
        }

        if let Some(top_level) = self.top_level.upgrade() {
            unsafe {
                let (handle, ref_count) = top_level
                    .data::<WindowHandleData>(WINDOW_HANDLE_KEY)
                    .unwrap()
                    .as_mut();

                if *ref_count > 1 {
                    *ref_count -= 1;
                    return;
                }

                top_level.unexport_handle();
                tracing::trace!("Unexported handle: {:?}", handle);

                let _ = top_level.steal_data::<WindowHandleData>(WINDOW_HANDLE_KEY);
            }
        }
    }
}

pub async fn try_downcast(surface: &Surface) -> Option<super::WindowIdentifier> {
    if let Some(top_level) = surface.downcast_ref::<gdk_wayland::WaylandToplevel>() {
        let handle = unsafe {
            if let Some(mut handle) = top_level.data::<WindowHandleData>(WINDOW_HANDLE_KEY) {
                let (handle, ref_count) = handle.as_mut();
                *ref_count += 1;
                handle.clone()
            } else {
                let (tx, rx) = oneshot::channel();
                let tx = RefCell::new(Some(tx));

                let result = top_level.export_handle(move |_, handle| {
                    let tx = tx.take().expect("callback called twice");

                    match handle {
                        Ok(handle) => {
                            let _ = tx.send(Some(handle.to_string()));
                        }
                        Err(err) => {
                            tracing::warn!("Failed to export handle: {:?}", err);
                            let _ = tx.send(None);
                        }
                    }
                });

                if !result {
                    return None;
                }

                let handle = rx.await.unwrap();
                top_level.set_data::<WindowHandleData>(WINDOW_HANDLE_KEY, (handle.clone(), 1));
                handle
            }
        };

        return Some(super::WindowIdentifier::Wayland(WaylandIdentifier {
            top_level: top_level.downgrade(),
            handle,
        }));
    }
    None
}
