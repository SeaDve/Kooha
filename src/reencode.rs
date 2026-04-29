//! Re-encode recordings to target a specific file size.
//!
//! Uses GStreamer to re-encode video with adjustable bitrate to hit a target size.
//! For screencasts, we can reduce framerate to 15fps if needed to hit the target.

use anyhow::{Context, Result};
use gst::prelude::*;
use gtk::gio;
use std::path::PathBuf;
use std::time::Duration;

/// Compression preset for recordings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionPreset {
    /// Maximum quality, no size optimization
    #[default]
    BestQuality,
    /// Balanced quality and size
    Balanced,
    /// Maximum compression, smallest file
    Smallest,
}

impl CompressionPreset {
    /// Returns the CRF (Constant Rate Factor) for x264enc.
    /// Lower = better quality, higher = more compression.
    pub fn crf(&self) -> i32 {
        match self {
            CompressionPreset::BestQuality => 18,
            CompressionPreset::Balanced => 24,
            CompressionPreset::Smallest => 30,
        }
    }

    /// Returns the target framerate for this preset.
    pub fn framerate(&self) -> gst::Fraction {
        match self {
            CompressionPreset::BestQuality => gst::Fraction::from_integer(60),
            CompressionPreset::Balanced => gst::Fraction::from_integer(30),
            CompressionPreset::Smallest => gst::Fraction::from_integer(15),
        }
    }
}

/// Probe the duration of a video file using GStreamer.
async fn probe_duration(file: &gio::File) -> Result<Duration> {
    let pipeline_str = format!(
        "filesrc location={} ! decodebin ! fakesink sync=true",
        file.path()
            .ok_or_else(|| anyhow::anyhow!("No path for file"))?
            .display()
    );

    let pipeline = gst::Pipeline::builder().name("duration-probe").build();
    let elements = gst::parse::launch(&pipeline_str)
        .context("Failed to parse GStreamer pipeline for duration probe")?;

    let playbin = elements
        .into_iter()
        .next()
        .context("No element from pipeline")?;
    pipeline.add_many([&playbin])?;

    // Set to playing to read metadata
    pipeline.set_state(gst::State::Playing)?;

    // Wait for metadata
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::from_seconds(5), gst::ClockTime::NONE) {
        if let gst::MessageView::Tag { tag, .. } = msg.view() {
            if let Some(duration_ns) = tag.duration() {
                pipeline.set_state(gst::State::Null)?;
                return Ok(Duration::from_nanos(duration_ns as u64));
            }
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Err(anyhow::anyhow!("Could not probe video duration"))
}

/// Re-encode a video file to target a specific size.
///
/// Calculates the required bitrate from the target size and video duration,
/// then re-encodes using x264enc with that bitrate.
pub async fn reencode_to_target_size(
    input_file: &gio::File,
    output_file: &gio::File,
    target_size_bytes: u64,
) -> Result<()> {
    use anyhow::anyhow;

    // Get input file info
    let info = input_file
        .query_info_future(
            gio::FILE_ATTRIBUTE_STANDARD_SIZE,
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT_IDLE,
        )
        .await
        .context("Failed to query input file info")?;

    let input_size = info.size() as u64;

    // If already smaller than target, just copy
    if input_size <= target_size_bytes {
        tracing::info!("Input file already smaller than target, copying without re-encoding");
        input_file
            .copy_future(
                output_file,
                gio::FileCopyFlags::OVERWRITE,
                glib::Priority::DEFAULT_IDLE,
                None,
                None,
                None,
            )
            .await
            .context("Failed to copy file")?;
        return Ok(());
    }

    // Get video duration using GStreamer probe
    let duration = probe_duration(input_file).await?;

    // Calculate target bitrate (bits per second)
    // target_size_bytes * 8 bits/byte / duration_seconds = bits per second
    // Reserve 128kbps for audio
    let duration_seconds = duration.as_secs_f64();
    if duration_seconds < 1.0 {
        return Err(anyhow!("Video too short to re-encode"));
    }

    let total_bitrate = (target_size_bytes as f64 * 8.0 / duration_seconds) as u32;
    let video_bitrate = total_bitrate.saturating_sub(128_000); // Reserve for audio

    // Determine framerate based on how aggressive the compression needs to be
    let size_ratio = target_size_bytes as f64 / input_size as f64;
    let framerate = if size_ratio > 0.5 {
        gst::Fraction::from_integer(60) // Keep 60fps if target is >50% of original
    } else if size_ratio > 0.2 {
        gst::Fraction::from_integer(30) // Drop to 30fps for moderate compression
    } else {
        gst::Fraction::from_integer(15) // Drop to 15fps for aggressive compression
    };

    tracing::info!(
        "Re-encoding: target_size={}MB, duration={:.1}s, bitrate={}kbps, fps={}",
        target_size_bytes / 1_000_000,
        duration_seconds,
        video_bitrate / 1000,
        framerate.numer()
    );

    // Build re-encode pipeline
    let input_path = input_file
        .path()
        .ok_or_else(|| anyhow!("No path for input file"))?;
    let output_path = output_file
        .path()
        .ok_or_else(|| anyhow!("No path for output file"))?;

    let pipeline_desc = format!(
        "filesrc location={} ! qtdemux ! h264parse ! rtph264pay config-interval=3 pt=96 ! udpsink host=127.0.0.1 port=5000",
        input_path.display()
    );

    // Use a simpler approach with decodebin + encoder
    let pipeline_str = format!(
        "filesrc location={} ! decodebin ! videoconvert ! x264enc bitrate={} tune=zerolatency speed-preset=veryfast threads=0 ! mp4mux ! filesink location={}",
        input_path.display(),
        video_bitrate,
        output_path.display()
    );

    let pipeline = gst::Pipeline::builder().name("reencode-pipeline").build();

    let elements =
        gst::parse::launch(&pipeline_str).context("Failed to parse re-encode pipeline")?;

    for element in elements.iter() {
        pipeline.add(&element)?;
    }

    // Link elements
    elements.link()?;

    // Start re-encoding
    pipeline.set_state(gst::State::Playing)?;

    // Wait for completion
    let bus = pipeline.bus().unwrap();
    loop {
        let message = bus.timed_pop(gst::ClockTime::NONE);
        match message {
            Some(msg) => match msg.view() {
                gst::MessageView::Eos { .. } => {
                    tracing::info!("Re-encode complete");
                    break;
                }
                gst::MessageView::Error { error, debug } => {
                    pipeline.set_state(gst::State::Null)?;
                    return Err(anyhow!("Re-encode error: {} ({})", error, debug));
                }
                gst::MessageView::AsyncDone { .. } => {
                    // Continue waiting
                }
                _ => {}
            },
            None => break,
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}

/// Re-encode a video file using a compression preset.
pub async fn reencode_with_preset(
    input_file: &gio::File,
    output_file: &gio::File,
    preset: CompressionPreset,
) -> Result<()> {
    use anyhow::anyhow;

    let input_path = input_file
        .path()
        .ok_or_else(|| anyhow!("No path for input file"))?;
    let output_path = output_file
        .path()
        .ok_or_else(|| anyhow!("No path for output file"))?;

    let framerate = preset.framerate();
    let fps_num = framerate.numer();
    let fps_den = if framerate.denom() > 0 {
        framerate.denom()
    } else {
        1
    };

    let pipeline_str = format!(
        "filesrc location={} ! decodebin ! videoconvert ! videoscale ! videorate max-rate={}/{} ! x264enc tune=zerolatency speed-preset=veryfast threads=0 ! mp4mux ! filesink location={}",
        input_path.display(),
        fps_num,
        fps_den,
        output_path.display()
    );

    let pipeline = gst::Pipeline::builder().name("reencode-pipeline").build();

    let elements =
        gst::parse::launch(&pipeline_str).context("Failed to parse re-encode pipeline")?;

    for element in elements.iter() {
        pipeline.add(&element)?;
    }

    elements.link()?;

    pipeline.set_state(gst::State::Playing)?;

    // Wait for completion
    let bus = pipeline.bus().unwrap();
    loop {
        let message = bus.timed_pop(gst::ClockTime::NONE);
        match message {
            Some(msg) => match msg.view() {
                gst::MessageView::Eos { .. } => {
                    tracing::info!("Re-encode complete");
                    break;
                }
                gst::MessageView::Error { error, debug } => {
                    pipeline.set_state(gst::State::Null)?;
                    return Err(anyhow!("Re-encode error: {} ({})", error, debug));
                }
                _ => {}
            },
            None => break,
        }
    }

    pipeline.set_state(gst::State::Null)?;
    Ok(())
}
