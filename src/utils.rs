use anyhow::{anyhow, Result};
use gtk::{
    gio,
    glib::{self, prelude::*},
};

use std::{env, path::Path};

use crate::{settings::Settings, Application};

const MAX_THREAD_COUNT: u32 = 64;

/// Spawns a future in the default [`glib::MainContext`]
pub fn spawn<F: std::future::Future<Output = ()> + 'static>(fut: F) {
    let ctx = glib::MainContext::default();
    ctx.spawn_local(fut);
}

/// Get the global instance of `Application`.
///
/// # Panics
/// Panics if the application is not running or if this is
/// called on a non-main thread.
pub fn app_instance() -> Application {
    debug_assert!(
        gtk::is_initialized_main_thread(),
        "Application can only be accessed in the main thread"
    );

    gio::Application::default().unwrap().downcast().unwrap()
}

/// Get the global instance of `Settings`.
///
/// # Panics
/// Panics if the application is not running or if this is
/// called on a non-main thread.
pub fn app_settings() -> Settings {
    app_instance().settings().clone()
}

/// Whether the application is running in a flatpak sandbox.
pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Ideal thread count to use for `GStreamer` processing.
pub fn ideal_thread_count() -> u32 {
    glib::num_processors().min(MAX_THREAD_COUNT)
}

pub fn is_experimental_mode() -> bool {
    env::var("KOOHA_EXPERIMENTAL").map_or(false, |value| value == "1")
}

/// Helper function for more helpful error messages when failed to find
/// an element factory.
pub fn find_element_factory(factory_name: &str) -> Result<gst::ElementFactory> {
    gst::ElementFactory::find(factory_name)
        .ok_or_else(|| anyhow!("Factory `{}` not found", factory_name))
}
