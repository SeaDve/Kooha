use gtk::gio::glib;

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
