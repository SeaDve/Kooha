use ashpd::desktop::screencast::Stream;
use gtk::{glib, prelude::*};

use std::{
    cmp, env,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::data_types::{Rectangle, Screen};

const MAX_THREAD_COUNT: u32 = 64;
const GIF_DEFAULT_FRAMERATE: u32 = 15;

#[derive(Debug, PartialEq, strum_macros::EnumString)]
#[strum(serialize_all = "snake_case")]
enum VideoFormat {
    Webm,
    Mkv,
    Mp4,
    Gif,
}

#[derive(Debug, Default)]
pub struct PipelineBuilder {
    framerate: u32,
    file_path: PathBuf,
    fd: i32,
    streams: Vec<Stream>,
    speaker_source: Option<String>,
    mic_source: Option<String>,
    coordinates: Option<Rectangle>,
    actual_screen: Option<Screen>,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn framerate(&mut self, framerate: u32) -> &mut Self {
        self.framerate = framerate;
        self
    }

    pub fn file_path(&mut self, file_path: PathBuf) -> &mut Self {
        self.file_path = file_path;
        self
    }

    pub fn fd(&mut self, fd: i32) -> &mut Self {
        self.fd = fd;
        self
    }

    pub fn streams(&mut self, streams: Vec<Stream>) -> &mut Self {
        self.streams = streams;
        self
    }

    pub fn speaker_source(&mut self, speaker_source: String) -> &mut Self {
        self.speaker_source = Some(speaker_source);
        self
    }

    pub fn mic_source(&mut self, mic_source: String) -> &mut Self {
        self.mic_source = Some(mic_source);
        self
    }

    pub fn coordinates(&mut self, coordinates: Rectangle) -> &mut Self {
        self.coordinates = Some(coordinates);
        self
    }

    pub fn actual_screen(&mut self, actual_screen: Screen) -> &mut Self {
        self.actual_screen = Some(actual_screen);
        self
    }

    pub fn build(&mut self) -> Result<gst::Pipeline, glib::Error> {
        use std::mem;

        let pipeline_string = PipelineAssembler::from_builder(mem::take(self)).assemble();
        log::debug!("pipeline_string: {}", &pipeline_string);

        gst::parse_launch_full(&pipeline_string, None, gst::ParseFlags::FATAL_ERRORS)
            .map(|element| element.downcast().unwrap())
    }
}

struct PipelineAssembler {
    builder: PipelineBuilder,
}

impl PipelineAssembler {
    pub const fn from_builder(builder: PipelineBuilder) -> Self {
        Self { builder }
    }

    pub fn assemble(&self) -> String {
        let pipeline_elements = vec![
            self.compositor(),
            Some("queue name=queue0".to_string()),
            Some("videorate".to_string()),
            Some(format!("video/x-raw, framerate={}/1", self.framerate())),
            self.videoscale(),
            self.videocrop(),
            Some("videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T".to_string()),
            Some("queue".to_string()),
            Some(self.videoenc()),
            Some("queue".to_string()),
            self.muxer(),
            Some(format!("filesink name=filesink location=\"{}\"", self.file_path().display())),
        ];

        let pipeline_string = pipeline_elements
            .into_iter()
            .flatten()
            .collect::<Vec<String>>()
            .join(" ! ");

        [pipeline_string, self.pipewiresrc(), self.pulsesrc()]
            .join(" ")
            .replace("%T", &ideal_thread_count().to_string())
    }

    fn compositor(&self) -> Option<String> {
        if self.has_single_stream() {
            return None;
        }

        // This allows us to place the videos side by side with each other, without overlaps.
        let mut current_pos = 0;
        let compositor_pads: Vec<String> = self
            .streams()
            .iter()
            .enumerate()
            .map(|(sink_num, stream)| {
                let pad = format!("sink_{}::xpos={}", sink_num, current_pos);
                let stream_width = stream.size().unwrap().0;
                current_pos += stream_width;
                pad
            })
            .collect();

        Some(format!(
            "compositor name=comp {}",
            compositor_pads.join(" ")
        ))
    }

    fn pipewiresrc(&self) -> String {
        if self.has_single_stream() {
            // If there is a single stream, connect pipewiresrc directly to queue0.
            let node_id = self.streams()[0].pipe_wire_node_id();
            return format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate={}/1 ! queue0.", self.fd(), node_id, self.framerate());
        }

        let pipewiresrc_list: Vec<String> = self.streams().iter().map(|stream| {
            let node_id = stream.pipe_wire_node_id();
            format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate={}/1 ! comp.", self.fd(), node_id, self.framerate())
        }).collect();

        pipewiresrc_list.join(" ")
    }

    fn pulsesrc(&self) -> String {
        if self.video_format() == VideoFormat::Gif {
            return "".to_string();
        }

        let audioenc = self.audioenc().unwrap();

        match (self.speaker_source(), self.mic_source()) {
            (Some(speaker_source), Some(mic_source)) => {
                format!("pulsesrc device=\"{}\" ! queue ! audiomixer name=mix ! {} ! queue ! mux. pulsesrc device=\"{}\" ! queue ! mix.",
                    speaker_source,
                    audioenc,
                    mic_source,
                )
            }
            (Some(speaker_source), None) => {
                format!(
                    "pulsesrc device=\"{}\" ! {} ! queue ! mux.",
                    speaker_source, audioenc
                )
            }
            (None, Some(mic_source)) => {
                format!(
                    "pulsesrc device=\"{}\" ! {} ! queue ! mux.",
                    mic_source, audioenc
                )
            }
            (None, None) => "".to_string(),
        }
    }

    fn videoscale(&self) -> Option<String> {
        if self.builder.coordinates.is_some() {
            // We could freely get the first stream because screencast portal won't allow multiple
            // sources selection if it is selection mode. Thus, there will be always single stream
            // present when we have coordinates. (The same applies with videocrop).
            let stream = &self.streams()[0];
            let width = stream.size().unwrap().0;
            let height = stream.size().unwrap().1;

            Some(format!(
                "videoscale ! video/x-raw, width={}, height={}",
                width, height
            ))
        } else {
            None
        }
    }

    fn videocrop(&self) -> Option<String> {
        self.builder.coordinates.map(|ref coords| {
            let stream = &self.streams()[0];

            let actual_screen = self.builder.actual_screen.as_ref().unwrap();
            let (stream_width, stream_height) = stream.size().unwrap();

            let scale_factor = (stream_width / actual_screen.width) as f64;
            let coords = coords.rescale(scale_factor);

            let top_crop = coords.y;
            let left_crop = coords.x;
            let right_crop = stream_width as f64 - (coords.width + coords.x);
            let bottom_crop = stream_height as f64 - (coords.height + coords.y);

            // It is a requirement for x264enc to have even resolution.
            format!(
                "videocrop top={} left={} right={} bottom={}",
                round_to_even(top_crop),
                round_to_even(left_crop),
                round_to_even(right_crop),
                round_to_even(bottom_crop)
            )
        })
    }

    fn videoenc(&self) -> String {
        let value = env::var("KOOHA_VAAPI").unwrap_or_default();
        let is_use_vaapi = value == "1";
        log::debug!("is_use_vaapi: {}", is_use_vaapi);

        if is_use_vaapi {
            match self.video_format() {
                VideoFormat::Webm | VideoFormat::Mkv => "vaapivp8enc", // FIXME Improve pipelines
                VideoFormat::Mp4 => "vaapih264enc max-qp=17 ! h264parse",
                VideoFormat::Gif => "gifenc repeat=-1 speed=30", // FIXME This doesn't really use vaapi
            }
        } else {
            match self.video_format() {
                VideoFormat::Webm | VideoFormat::Mkv => "vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T",
                VideoFormat::Mp4 => "x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline",
                VideoFormat::Gif => "gifenc repeat=-1 speed=30",
            }
        }.to_string()
    }

    fn audioenc(&self) -> Option<String> {
        match self.video_format() {
            VideoFormat::Webm | VideoFormat::Mkv | VideoFormat::Mp4 => Some("opusenc"),
            VideoFormat::Gif => None,
        }
        .map(str::to_string)
    }

    fn muxer(&self) -> Option<String> {
        let video_format = self.video_format();

        let muxer = match video_format {
            VideoFormat::Webm => "webmmux",
            VideoFormat::Mkv => "matroskamux",
            VideoFormat::Mp4 => "mp4mux",
            VideoFormat::Gif => return None,
        };

        Some(format!("{} name=mux", muxer))
    }

    fn video_format(&self) -> VideoFormat {
        let file_extension = self.file_path().extension().unwrap().to_str().unwrap();

        VideoFormat::from_str(file_extension).expect("Invalid video format.")
    }

    fn framerate(&self) -> u32 {
        if self.video_format() == VideoFormat::Gif {
            return GIF_DEFAULT_FRAMERATE;
        }

        self.builder.framerate
    }

    fn file_path(&self) -> &Path {
        self.builder.file_path.as_path()
    }

    fn speaker_source(&self) -> Option<&str> {
        self.builder.speaker_source.as_deref()
    }

    fn mic_source(&self) -> Option<&str> {
        self.builder.mic_source.as_deref()
    }

    const fn fd(&self) -> i32 {
        self.builder.fd
    }

    const fn streams(&self) -> &Vec<Stream> {
        &self.builder.streams
    }

    fn has_single_stream(&self) -> bool {
        self.streams().len() == 1
    }
}

pub const fn round_to_even(number: f64) -> i32 {
    number as i32 / 2 * 2
}

pub fn ideal_thread_count() -> u32 {
    let num_processors = glib::num_processors();
    cmp::min(num_processors, MAX_THREAD_COUNT)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn odd_round_to_even() {
        assert_eq!(round_to_even(3.0), 2);
        assert_eq!(round_to_even(99.0), 98);
    }

    #[test]
    fn even_round_to_even() {
        assert_eq!(round_to_even(50.0), 50);
        assert_eq!(round_to_even(4.0), 4);
    }

    #[test]
    fn float_round_to_even() {
        assert_eq!(round_to_even(5.3), 4);
        assert_eq!(round_to_even(2.9), 2);
    }

    #[test]
    fn video_format_from_str() {
        assert_eq!(VideoFormat::Webm, VideoFormat::from_str("webm").unwrap());
        assert_eq!(VideoFormat::Mkv, VideoFormat::from_str("mkv").unwrap());
        assert_eq!(VideoFormat::Mp4, VideoFormat::from_str("mp4").unwrap());
        assert_eq!(VideoFormat::Gif, VideoFormat::from_str("gif").unwrap());
    }
}
