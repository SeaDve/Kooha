use gettextrs::gettext;
use gst::prelude::*;
use gtk::prelude::*;

use std::{
    env,
    fs::File,
    io::{prelude::*, BufReader},
};

use crate::{
    config::{APP_ID, VERSION},
    utils,
};

pub fn present_window(transient_for: Option<&impl IsA<gtk::Window>>) {
    let win = adw::AboutWindow::builder()
        .modal(true)
        .application_icon(APP_ID)
        .application_name(&gettext("Kooha"))
        .developer_name(&gettext("Dave Patrick Caberto"))
        .version(VERSION)
        .copyright(&gettext("© 2022 Dave Patrick Caberto"))
        .license_type(gtk::License::Gpl30)
        .developers(vec![
            "Dave Patrick Caberto".into(),
            "Mathiascode".into(),
            "Felix Weilbach".into(),
        ])
        // Translators: Replace "translator-credits" with your names. Put a comma between.
        .translator_credits(&gettext("translator-credits"))
        .issue_url("https://github.com/SeaDve/Kooha/issues")
        .support_url("https://github.com/SeaDve/Kooha/discussions")
        .debug_info(&debug_info())
        .debug_info_filename("kooha-debug-info")
        .release_notes_version("2.2.0")
        .release_notes(release_notes())
        .build();

    win.add_link(
        &gettext("Donate (Liberapay)"),
        "https://liberapay.com/SeaDve",
    );
    win.add_link(
        &gettext("Donate (PayPal)"),
        "https://www.paypal.com/paypalme/davecaberto",
    );

    win.add_link(&gettext("GitHub"), "https://github.com/SeaDve/Kooha");
    win.add_link(
        &gettext("Translate"),
        "https://hosted.weblate.org/projects/kooha/pot-file",
    );

    win.set_transient_for(transient_for);
    win.present();
}

fn release_notes() -> &'static str {
    r#"<p>This release contains new features and fixes:</p>
    <ul>
      <li>New area selection UI</li>
      <li>Added option to change the frame rate through the UI</li>
      <li>Improved delay settings flexibility</li>
      <li>Added preferences window for easier configuration</li>
      <li>Added `KOOHA_EXPERIMENTAL` env var to show experimental (unsupported) encoders like VAAPI-VP8 and VAAPI-H264</li>
      <li>Added the following experimental (unsupported) encoders: VP9, AV1, and VAAPI-VP9</li>
      <li>Unavailable formats/encoders are now hidden from the UI</li>
      <li>Fixed broken audio on long recordings</li>
      <li>Only show None profile when it is active</li>
      <li>Guard window selection behind `KOOHA_EXPERIMENTAL` env var</li>
      <li>Updated translations</li>
    </ul>"#
}

fn debug_info() -> String {
    let is_flatpak = utils::is_flatpak();
    let is_experimental_mode = utils::is_experimental_mode();
    let distribution = distribution_info().unwrap_or_else(|| "<unknown>".into());
    let desktop_session = env::var("DESKTOP_SESSION").unwrap_or_else(|_| "<unknown>".into());
    let display_server = env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "<unknown>".into());

    let gtk_version = format!(
        "{}.{}.{}",
        gtk::major_version(),
        gtk::minor_version(),
        gtk::micro_version()
    );
    let adw_version = format!(
        "{}.{}.{}",
        adw::major_version(),
        adw::minor_version(),
        adw::micro_version()
    );
    let gst_version_string = gst::version_string();
    let pipewire_version = gst::Registry::get()
        .find_feature("pipewiresrc", gst::ElementFactory::static_type())
        .map_or("<Feature Not Found>".into(), |feature| {
            feature
                .plugin()
                .map_or("<Plugin Not Found>".into(), |plugin| plugin.version())
        });

    format!(
        r#"- {APP_ID} {VERSION}
- Flatpak: {is_flatpak}
- Experimental: {is_experimental_mode}

- Distribution: {distribution}
- Desktop Session: {desktop_session}
- Display Server: {display_server}

- GTK {gtk_version}
- Libadwaita {adw_version}
- {gst_version_string}
- Pipewire {pipewire_version}"#
    )
}

fn distribution_info() -> Option<String> {
    let file = File::open("/etc/os-release").ok()?;

    BufReader::new(file).lines().find_map(|line| {
        let line = line.ok()?;
        let (key, value) = line.split_once('=')?;
        if key == "PRETTY_NAME" {
            Some(value.trim_matches('\"').to_string())
        } else {
            None
        }
    })
}
