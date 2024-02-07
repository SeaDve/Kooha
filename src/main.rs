#![warn(
    rust_2018_idioms,
    clippy::items_after_statements,
    clippy::needless_pass_by_value,
    clippy::explicit_iter_loop,
    clippy::semicolon_if_nothing_returned,
    clippy::match_wildcard_for_single_variants,
    clippy::inefficient_to_string,
    clippy::map_unwrap_or,
    clippy::implicit_clone,
    clippy::struct_excessive_bools,
    clippy::trivially_copy_pass_by_ref,
    clippy::unreadable_literal,
    clippy::if_not_else,
    clippy::doc_markdown,
    clippy::unused_async,
    clippy::default_trait_access,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::dbg_macro,
    clippy::todo,
    clippy::print_stdout
)]
#![allow(clippy::format_push_string)] // TODO remove once gettext-rs fixes macro issues

mod about;
mod application;
mod area_selector;
mod audio_device;
mod cancelled;
mod config;
mod element_properties;
mod help;
mod i18n;
mod pipeline;
mod preferences_window;
mod profile;
mod recording;
mod screencast_session;
mod settings;
mod timer;
mod toggle_button;
mod utils;
mod window;

use gettextrs::{gettext, LocaleCategory};
use gtk::{gio, glib};

use self::{
    application::Application,
    config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE},
};

#[cfg(test)]
#[macro_use]
extern crate ctor;

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt::init();

    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain.");
    gettextrs::textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain.");

    glib::set_application_name(&gettext("Kooha"));

    gst::init().expect("Unable to start gstreamer.");
    gstgif::plugin_register_static().expect("Failed to register gif plugin.");
    gstgtk4::plugin_register_static().expect("Failed to register gtk4 plugin.");

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file.");
    gio::resources_register(&res);

    let app = Application::new();
    app.run()
}

#[cfg(test)]
mod test {
    use ctor;
    use std::{env, process::Command};

    // Run once before tests are executed.
    #[ctor]
    fn setup_schema() {
        let schema_dir = &env::var("GSETTINGS_SCHEMA_DIR")
            .unwrap_or(concat!(env!("CARGO_MANIFEST_DIR"), "/data").into());

        let output = Command::new("glib-compile-schemas")
            .arg(schema_dir)
            .output()
            .unwrap();

        if !output.status.success() {
            panic!(
                "Failed to compile GSchema for tests; stdout: {}; stderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        env::set_var("GSETTINGS_SCHEMA_DIR", schema_dir);
        env::set_var("GSETTINGS_BACKEND", "memory");
    }
}
