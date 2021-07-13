use ashpd::zbus::Connection;

use std::error::Error;

pub struct Utils;

impl Utils {
    pub fn set_raise_active_window_request(is_raised: bool) -> Result<(), Box<dyn Error>> {
        shell_window_eval("make_above", is_raised)?;
        shell_window_eval("stick", is_raised)?;

        Ok(())
    }

    pub fn default_audio_sources() {}
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
