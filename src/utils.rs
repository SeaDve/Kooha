use gtk::gio::glib;

use std::{cmp, path::Path};

const MAX_THREAD_COUNT: u32 = 64;

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(fut);
}

/// Whether the application is running in a flatpak sandbox.
pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Ideal thread count to use for `GStreamer` processing.
pub fn ideal_thread_count() -> u32 {
    cmp::min(glib::num_processors(), MAX_THREAD_COUNT)
}
