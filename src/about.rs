use gettextrs::gettext;
use gtk::{glib::IsA, prelude::*};

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
        .release_notes(release_notes())
        .build();

    win.add_link(
        &gettext("Donate (Liberapay)"),
        "https://liberapay.com/SeaDve",
    );
    win.add_link(
        &gettext("Donate (PayPal)"),
        "https://www.paypal.com/paypalme/sedve",
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
      <li>Remember previously selected video sources</li>
      <li>Added ability to cancel while flushing the recording</li>
      <li>Use different icons for settings toggle button to discern state more clearly</li>
      <li>Added 3 seconds delay option</li>
      <li>Fixed x264 encoder failing to initialize on uneven resolutions</li>
      <li>Fixed minutes stuck on 00 if time is equal or greater than an hour</li>
      <li>Recordings are now stored by default in `~/Videos/Kooha` (This won't affect existing settings)</li>
      <li>"Show in Files" button in notifications now highlights the file in the file manager</li>
      <li>Improved support information in the new about window</li>
      <li>Improved error handling</li>
      <li>Improved codebase and stability</li>
    </ul>"#
}

fn debug_info() -> String {
    let is_flatpak = utils::is_flatpak();
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

    format!(
        r#"- {APP_ID} {VERSION}
- Flatpak: {is_flatpak}

- Distribution: {distribution}
- Desktop Session: {desktop_session}
- Display Server: {display_server}

- GTK {gtk_version}
- Libadwaita {adw_version}
- {gst_version_string}"#
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
