//! Webcam device detection via PipeWire and GStreamer pipeline elements for webcam overlay.
//!
//! Uses `pipewiresrc` for the video source and `wpctl` to enumerate camera nodes.

use anyhow::{Context, Result, anyhow};
use gst::prelude::*;
use std::process::Command;
use std::str;

/// Represents a webcam video capture device exposed by PipeWire.
#[derive(Debug, Clone)]
pub struct WebcamDevice {
    /// Human-readable name (e.g. "Integrated Camera: Integrated Camera")
    pub name: String,
    /// PipeWire node ID (e.g. "64")
    pub node_id: String,
}

/// Find all available webcam devices via PipeWire using `wpctl`.
///
/// Enumerates devices of type `Camera` and returns their names and node IDs.
pub fn find_webcams() -> Result<Vec<WebcamDevice>> {
    let output = Command::new("wpctl")
        .args(["list", "devices", "--json"])
        .output()
        .context("Failed to execute `wpctl list devices --json`. Make sure PipeWire is running.")?;

    if !output.status.success() {
        let stderr = str::from_utf8(&output.stderr).unwrap_or("<binary>");
        return Err(anyhow!("`wpctl` failed: {}", stderr));
    }

    let stdout = str::from_utf8(&output.stdout).context("`wpctl` output is not valid UTF-8")?;

    let devices: serde_json::Value =
        serde_json::from_str(stdout).context("Failed to parse `wpctl` JSON output")?;

    let mut webcams = Vec::new();

    if let Some(arr) = devices.as_array() {
        for device in arr {
            let device_type = device.get("device.api").and_then(|v| v.as_str());
            let media_class = device.get("media.class").and_then(|v| v.as_str());

            // Match PipeWire camera devices
            if device_type == Some("api.vidcap") && media_class == Some("Video/Source/Camera") {
                let name = device
                    .get("description.name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown Camera")
                    .to_string();

                let node_id = device
                    .get("node.id")
                    .and_then(|v| v.as_u64())
                    .map(|id| id.to_string())
                    .unwrap_or_default();

                if !node_id.is_empty() {
                    webcams.push(WebcamDevice { name, node_id });
                }
            }
        }
    }

    Ok(webcams)
}

/// Returns the default webcam device (first available).
pub fn find_default_webcam() -> Result<WebcamDevice> {
    let webcams = find_webcams()?;
    webcams
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No webcam device found via PipeWire"))
}

/// Create a pipewiresrc element for the given webcam node ID.
pub fn make_pipewiresrc(node_id: &str, element_name: &str) -> Result<gst::Element> {
    let src = gst::ElementFactory::make("pipewiresrc")
        .name(element_name)
        .property("path", node_id)
        .property("do-timestamp", true)
        .property("provide-clock", false)
        .build()?;

    Ok(src)
}

/// Creates a bin that produces a circular webcam video feed via PipeWire.
///
/// Pipeline inside the bin:
///
/// pipewiresrc -> videoscale -> capsfilter -> circle (if available)
///
/// The circle element from gst-plugins-bad applies a circular mask to the video.
/// If unavailable, the overlay falls back to square.
///
/// `webcam_size` is the target square dimension in pixels.
pub fn make_webcam_bin(node_id: &str, webcam_size: i32) -> Result<gst::Bin> {
    let bin = gst::Bin::builder().name("kooha-webcam-bin").build();

    let pipewiresrc = make_pipewiresrc(node_id, "kooha-webcam-src")?;
    let queue = gst::ElementFactory::make("queue")
        .name("kooha-webcam-queue")
        .build()?;
    let videoscale = gst::ElementFactory::make("videoscale")
        .name("kooha-webcam-scale")
        .build()?;

    // Request square RGBA frames for the webcam overlay
    let caps = gst::Caps::builder("video/x-raw")
        .field("width", webcam_size)
        .field("height", webcam_size)
        .field("format", "RGBA")
        .build();

    let capsfilter = gst::ElementFactory::make("capsfilter")
        .name("kooha-webcam-caps")
        .property("caps", &caps)
        .build()?;

    bin.add_many([&pipewiresrc, &queue, &videoscale, &capsfilter])?;
    pipewiresrc.link(&queue)?;
    queue.link(&videoscale)?;
    videoscale.link_filtered(&capsfilter, &caps)?;

    // Try to use the circle element for circular masking (gst-plugins-bad).
    // Falls back to square if unavailable.
    let src_pad = if let Ok(circle) = gst::ElementFactory::make("circle")
        .name("kooha-webcam-circle")
        .build()
    {
        bin.add(&circle)?;
        capsfilter.link(&circle)?;
        circle.static_pad("src").unwrap()
    } else {
        tracing::info!(
            "Circle element not available, using square webcam overlay. \
             Install gstreamer1.0-plugins-bad for circular webcam."
        );
        capsfilter.static_pad("src").unwrap()
    };

    bin.add_pad(&gst::GhostPad::with_target(&src_pad)?)?;

    Ok(bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires PipeWire and a webcam
    fn find_webcams_returns_devices() {
        gst::init().unwrap();
        let webcams = find_webcams().unwrap();
        assert!(!webcams.is_empty());
    }
}
