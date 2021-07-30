#![allow(clippy::new_without_default)]

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
use i18n::i18n;

fn main() {
    pretty_env_logger::init();

    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(GETTEXT_PACKAGE, LOCALEDIR).expect("Unable to bind the text domain");
    gettextrs::textdomain(GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&i18n("Kooha"));

    gst::init().expect("Unable to start gstreamer");
    gtk::init().expect("Unable to start GTK4");
    adw::init();

    gstgif::plugin_register_static().expect("Failed to register gif plugin");

    let res = gio::Resource::load(RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    let app = Application::new();
    app.run();
}
