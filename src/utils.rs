use anyhow::{anyhow, Result};
use gtk::glib;
use once_cell::sync::Lazy;

use std::env;

const MAX_THREAD_COUNT: u32 = 64;

pub static IS_EXPERIMENTAL_MODE: Lazy<bool> =
    Lazy::new(|| env::var("KOOHA_EXPERIMENTAL").is_ok_and(|value| value == "1"));

/// Ideal thread count to use for `GStreamer` processing.
pub fn ideal_thread_count() -> u32 {
    glib::num_processors().min(MAX_THREAD_COUNT)
}

/// Helper function for more helpful error messages when failed to find
/// an element factory.
pub fn find_element_factory(factory_name: &str) -> Result<gst::ElementFactory> {
    gst::ElementFactory::find(factory_name)
        .ok_or_else(|| anyhow!("Factory `{}` not found", factory_name))
}
