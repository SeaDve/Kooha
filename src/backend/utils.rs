use anyhow::bail;
use ashpd::zbus::Connection;
use gtk::glib;

use std::{path::Path, process};

pub fn default_audio_sources() -> (Option<String>, Option<String>) {
    let output = process::Command::new("/usr/bin/pactl")
        .arg("info")
        .output()
        .expect("Failed to run pactl")
        .stdout;
    let output = String::from_utf8(output).expect("Failed to convert utf8 to String");

    let default_sink = format!(
        "{}.monitor",
        output
            .lines()
            .nth(12)
            .unwrap()
            .split_whitespace()
            .nth(2)
            .unwrap()
    );
    let default_source = output
        .lines()
        .nth(13)
        .unwrap()
        .split_whitespace()
        .nth(2)
        .unwrap()
        .to_string();

    if default_source == default_sink {
        (Some(default_sink), None)
    } else {
        (Some(default_sink), Some(default_source))
    }
}

pub fn check_if_accessible(path: &Path) -> bool {
    let home_folder = glib::home_dir();
    let is_in_home_folder = path.starts_with(&home_folder);

    is_in_home_folder && path != home_folder
}

pub fn set_raise_active_window_request(is_raised: bool) -> anyhow::Result<()> {
    shell_window_eval("make_above", is_raised)?;
    shell_window_eval("stick", is_raised)?;
    Ok(())
}

fn shell_window_eval(method: &str, is_enabled: bool) -> anyhow::Result<()> {
    let reverse_keyword = if is_enabled { "" } else { "un" };
    let command = format!(
        "global.display.focus_window.{}{}()",
        reverse_keyword, method
    );

    let connection = Connection::new_session()?;
    let message = connection.call_method(
        Some("org.gnome.Shell"),
        "/org/gnome/Shell",
        Some("org.gnome.Shell"),
        "Eval",
        &command,
    )?;
    let (is_success, result): (bool, String) = message.body()?;

    if !is_success {
        bail!(result);
    };

    Ok(())
}
