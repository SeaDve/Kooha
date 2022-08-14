use anyhow::{Context, Result};
use gtk::gio::{self, glib, prelude::*};

use std::path::Path;

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(fut);
}

/// Whether the application is running in a flatpak sandbox.
pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Shows items in the default file manager.
pub async fn show_items(uris: &[&str], startup_id: &str) -> Result<()> {
    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .context("Failed to get session bus")?;

    connection
        .call_future(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&(uris, startup_id).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
        )
        .await
        .with_context(|| {
            format!(
                "Failed to invoke org.freedesktop.FileManager1.ShowItems with uris: {:?}",
                &uris
            )
        })?;

    Ok(())
}
