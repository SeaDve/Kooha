#![allow(clippy::new_without_default)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::match_wildcard_for_single_variants)]
#![warn(clippy::inefficient_to_string)]
#![warn(clippy::needless_pass_by_value)]
#![warn(clippy::await_holding_refcell_ref)]
#![warn(clippy::map_unwrap_or)]
#![warn(clippy::implicit_clone)]
#![warn(clippy::redundant_closure_for_method_calls)]
#![warn(clippy::struct_excessive_bools)]
#![warn(clippy::trivially_copy_pass_by_ref)]
#![warn(clippy::option_if_let_else)]

mod application;
mod backend;
mod config;
mod data_types;
mod i18n;
mod utils;
mod widgets;

use application::Application;
use config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};
use gettextrs::LocaleCategory;
use gtk::{gio, glib};

fn main() {
    pretty_env_logger::init();

    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    gettextrs::textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&i18n!("Kooha"));

    gst::init().expect("Unable to start gstreamer");
    gtk::init().expect("Unable to start GTK4");
    adw::init();

    gstgif::plugin_register_static().expect("Failed to register gif plugin");

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    let app = Application::new();
    app.run();
}
