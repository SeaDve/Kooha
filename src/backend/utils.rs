use ashpd::zbus::Connection;

use std::{error::Error, option::Option, process};

pub struct Utils;

impl Utils {
    pub fn set_raise_active_window_request(is_raised: bool) -> Result<(), Box<dyn Error>> {
        shell_window_eval("make_above", is_raised)?;
        shell_window_eval("stick", is_raised)?;
        Ok(())
    }

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
            .unwrap();

        if default_source == default_sink {
            (Some(default_sink), None)
        } else {
            (Some(default_sink), Some(default_source.to_string()))
        }
    }
}

fn shell_window_eval(method: &str, is_enabled: bool) -> Result<(), Box<dyn Error>> {
    let reverse_keyword = if is_enabled { "" } else { "un" };
    let command = format!(
        "global.display.focus_window.{}{}()",
        reverse_keyword, method
    );

    let connection = Connection::new_session()?;

    // FIXME properly handle errors
    connection.call_method(
        Some("org.gnome.Shell"),
        "/org/gnome/Shell",
        Some("org.gnome.Shell"),
        "Eval",
        &(command),
    )?;

    Ok(())
}
