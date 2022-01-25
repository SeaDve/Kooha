#![allow(clippy::new_without_default)]
#![warn(clippy::doc_markdown)]
#![warn(clippy::or_fun_call)]
#![warn(clippy::needless_pass_by_value)]
#![warn(clippy::explicit_iter_loop)]
#![warn(clippy::semicolon_if_nothing_returned)]
#![warn(clippy::match_wildcard_for_single_variants)]
#![warn(clippy::inefficient_to_string)]
#![warn(clippy::await_holding_refcell_ref)]
#![warn(clippy::map_unwrap_or)]
#![warn(clippy::implicit_clone)]
#![warn(clippy::struct_excessive_bools)]
#![warn(clippy::trivially_copy_pass_by_ref)]
#![warn(clippy::option_if_let_else)]
#![warn(clippy::unreadable_literal)]
#![warn(clippy::if_not_else)]
#![warn(clippy::doc_markdown)]

mod application;
mod area_selector;
mod backend;
mod config;
mod data_types;
mod error;
mod pactl;
mod settings;
mod utils;
mod window;

use application::Application;
use config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};
use gettextrs::{gettext, LocaleCategory};
use gtk::{gio, glib};

fn main() {
    pretty_env_logger::init();

    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain.");
    gettextrs::textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain.");

    glib::set_application_name(&gettext("Kooha"));

    gst::init().expect("Unable to start gstreamer.");
    gtk::init().expect("Unable to start GTK4.");

    gstgif::plugin_register_static().expect("Failed to register gif plugin.");

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file.");
    gio::resources_register(&res);

    let app = Application::new();
    app.run();
}
