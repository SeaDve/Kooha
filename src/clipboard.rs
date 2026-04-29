//! Clipboard helper for copying recorded videos.

use anyhow::{Context, Result};
use gst::prelude::*;
use gtk::gdk;

/// Copy a video file to the clipboard as a file URI.
pub fn copy_file_to_clipboard(file: &gio::File) -> Result<()> {
    let display = gdk::Display::default().context("No GDK display")?;
    let clipboard = display.clipboard();

    // Set the file URI on the clipboard
    clipboard.set(&gio::File::for_uri(&file.uri()));

    Ok(())
}

/// Copy a text string to the clipboard.
pub fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let display = gdk::Display::default().context("No GDK display")?;
    let clipboard = display.clipboard();
    clipboard.set_text(text);

    Ok(())
}
