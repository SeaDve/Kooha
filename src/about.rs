use std::{
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

use adw::prelude::*;
use anyhow::anyhow;
use anyhow::Result;
use gettextrs::gettext;
use gst::prelude::*;
use gtk::glib;

use crate::{
    config::{APP_ID, VERSION},
    experimental,
};

pub fn present_dialog(parent: &impl IsA<gtk::Widget>) {
    let dialog = adw::AboutDialog::builder()
        .application_icon(APP_ID)
        .application_name(gettext("Kooha"))
        .developer_name("Dave Patrick Caberto")
        .version(VERSION)
        .copyright("© 2024 Dave Patrick Caberto")
        .license_type(gtk::License::Gpl30)
        .developers(vec![
            "Dave Patrick Caberto",
            "Mathiascode",
            "Felix Weilbach",
        ])
        // Translators: Replace "translator-credits" with your names. Put a comma between.
        .translator_credits(gettext("translator-credits"))
        .issue_url("https://github.com/SeaDve/Kooha/issues")
        .support_url("https://github.com/SeaDve/Kooha/discussions")
        .debug_info(debug_info())
        .debug_info_filename("kooha-debug-info")
        .release_notes_version("2.3.0")
        .release_notes(release_notes())
        .build();

    dialog.add_link(
        &gettext("Donate (Buy Me a Coffee)"),
        "https://www.buymeacoffee.com/seadve",
    );
    dialog.add_link(&gettext("GitHub"), "https://github.com/SeaDve/Kooha");
    dialog.add_link(
        &gettext("Translate"),
        "https://hosted.weblate.org/projects/seadve/kooha",
    );

    dialog.present(parent);
}

fn release_notes() -> &'static str {
    r#"<p>This release contains new features and fixes:</p>
    <ul>
      <li>Area selector window is now resizable</li>
      <li>Previous selected area is now remembered</li>
      <li>Logout and idle are now inhibited while recording</li>
      <li>Video format and FPS are now shown in the main view</li>
      <li>Notifications now show the duration and size of the recording</li>
      <li>Notification actions now work even when the application is closed</li>
      <li>Progress is now shown when flushing the recording</li>
      <li>It is now much easier to pick from frame rate options</li>
      <li>Actually fixed audio from stuttering and being cut on long recordings</li>
      <li>Record audio in stereo rather than mono when possible</li>
      <li>Recordings are no longer deleted when flushing is cancelled</li>
      <li>Significant improvements in recording performance</li>
      <li>Improved preferences dialog UI</li>
      <li>Fixed incorrect output video orientation on certain compositors</li>
      <li>Fixed incorrect focus on area selector</li>
      <li>Fixed too small area selector window default size on HiDPI monitors</li>
      <li>Updated translations</li>
    </ul>"#
}

fn cpu_model() -> Result<String> {
    let output = Command::new("lscpu")
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    for res in output.stdout.lines() {
        let line = res?;

        if line.contains("Model name:") {
            if let Some((_, value)) = line.split_once(':') {
                return Ok(value.trim().to_string());
            }
        }
    }

    Ok("<unknown>".into())
}

fn gpu_model() -> Result<String> {
    let output = Command::new("lspci")
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    for res in output.stdout.lines() {
        let line = res?;

        if line.contains("VGA") {
            if let Some(value) = line.splitn(3, ':').last() {
                return Ok(value.trim().to_string());
            }
        }
    }

    Ok("<unknown>".into())
}

fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn debug_info() -> String {
    let is_flatpak = is_flatpak();
    let experimental_features = experimental::enabled_features();

    let language_names = glib::language_names().join(", ");

    let cpu_model = cpu_model().unwrap_or_else(|e| format!("<{}>", e));
    let gpu_model = gpu_model().unwrap_or_else(|e| format!("<{}>", e));

    let distribution = os_info("PRETTY_NAME").unwrap_or_else(|e| format!("<{}>", e));
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
- Experimental Features: {experimental_features:?}

- Language: {language_names}

- CPU: {cpu_model}
- GPU: {gpu_model}

- Distribution: {distribution}
- Desktop Session: {desktop_session}
- Display Server: {display_server}

- GTK {gtk_version}
- Libadwaita {adw_version}
- {gst_version_string}
- Pipewire {pipewire_version}"#
    )
}

fn os_info(key_name: &str) -> Result<String> {
    let os_release_path = if is_flatpak() {
        "/run/host/etc/os-release"
    } else {
        "/etc/os-release"
    };
    let file = File::open(os_release_path)?;

    for line in BufReader::new(file).lines() {
        let line = line?;
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if key == key_name {
            return Ok(value.trim_matches('\"').to_string());
        }
    }

    Err(anyhow!("unknown"))
}
