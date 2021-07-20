use anyhow::bail;
use ashpd::zbus::Connection;

use std::process;

pub struct Utils;

impl Utils {
    pub fn set_raise_active_window_request(is_raised: bool) -> anyhow::Result<()> {
        Utils::shell_window_eval("make_above", is_raised)?;
        Utils::shell_window_eval("stick", is_raised)?;
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
            .unwrap()
            .to_string();

        if default_source == default_sink {
            (Some(default_sink), None)
        } else {
            (Some(default_sink), Some(default_source))
        }
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
}
