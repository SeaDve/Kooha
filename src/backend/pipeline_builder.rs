use ashpd::desktop::screencast::Stream;
use gtk::glib;

use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    data_types::{Rectangle, Screen},
    utils,
};

const GIF_DEFAULT_FRAMERATE: u32 = 15;

#[derive(Debug, PartialEq, strum_macros::EnumString)]
#[strum(serialize_all = "snake_case")]
enum VideoFormat {
    Webm,
    Mkv,
    Mp4,
    Gif,
}

#[derive(Debug, Default, Clone)]
pub struct PipelineBuilder {
    is_record_speaker: bool,
    is_record_mic: bool,
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

    pub fn record_speaker(mut self, is_record_speaker: bool) -> Self {
        self.is_record_speaker = is_record_speaker;
        self
    }

    pub fn record_mic(mut self, is_record_mic: bool) -> Self {
        self.is_record_mic = is_record_mic;
        self
    }

    pub fn framerate(mut self, framerate: u32) -> Self {
        self.framerate = framerate;
        self
    }

    pub fn file_path(mut self, file_path: PathBuf) -> Self {
        self.file_path = file_path;
        self
    }

    pub fn fd(mut self, fd: i32) -> Self {
        self.fd = fd;
        self
    }

    pub fn streams(mut self, streams: Vec<Stream>) -> Self {
        self.streams = streams;
        self
    }

    pub fn speaker_source(mut self, speaker_source: Option<String>) -> Self {
        self.speaker_source = speaker_source;
        self
    }

    pub fn mic_source(mut self, mic_source: Option<String>) -> Self {
        self.mic_source = mic_source;
        self
    }

    pub fn coordinates(mut self, coordinates: Rectangle) -> Self {
        self.coordinates = Some(coordinates);
        self
    }

    pub fn actual_screen(mut self, actual_screen: Screen) -> Self {
        self.actual_screen = Some(actual_screen);
        self
    }

    pub fn parse_into_string(self) -> String {
        PipelineParser::from_builder(self).parse()
    }

    pub fn build(self) -> Result<gst::Element, glib::Error> {
        let pipeline_string = self.parse_into_string();
        log::debug!("pipeline_string: {}", &pipeline_string);

        gst::parse_launch(&pipeline_string)
    }
}

struct PipelineParser {
    builder: PipelineBuilder,
}

impl PipelineParser {
    pub fn from_builder(builder: PipelineBuilder) -> Self {
        Self { builder }
    }

    pub fn parse(&self) -> String {
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

        let mut pipeline_string = pipeline_elements
            .into_iter()
            .flatten()
            .collect::<Vec<String>>()
            .join(" ! ");

        pipeline_string = format!(
            "{} {} {}",
            pipeline_string,
            self.pipewiresrc(),
            self.pulsesrc()
        );

        pipeline_string.replace("%T", &utils::ideal_thread_count().to_string())
    }

    fn compositor(&self) -> Option<String> {
        if self.is_single_stream() {
            return None;
        }

        let mut current_pos = 0;
        let mut compositor_elements = vec!["compositor name=comp".to_string()];

        for (sink_num, stream) in self.streams().iter().enumerate() {
            // This allows us to place the videos size by size with each other, without overlaps.
            let pad = format!("sink_{}::xpos={}", sink_num, current_pos);
            compositor_elements.push(pad);

            let stream_width = stream.size().unwrap().0;
            current_pos += stream_width;
        }

        Some(compositor_elements.join(" "))
    }

    fn pipewiresrc(&self) -> String {
        if self.is_single_stream() {
            let node_id = self.streams()[0].pipe_wire_node_id();

            // If there is a single stream, connect pipewiresrc directly to queue0.
            return format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate={}/1 ! queue0.", self.fd(), node_id, self.framerate());
        }

        let mut pipewiresrc_list = Vec::new();
        for stream in self.streams().iter() {
            let node_id = stream.pipe_wire_node_id();
            pipewiresrc_list.push(format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate={}/1 ! comp.", self.fd(), node_id, self.framerate()));
        }

        pipewiresrc_list.join(" ")
    }

    fn pulsesrc(&self) -> String {
        if self.video_format() == VideoFormat::Gif {
            return "".to_string();
        }

        let speaker_source = self.speaker_source();
        let mic_source = self.mic_source();

        let is_record_speaker = self.builder.is_record_speaker && speaker_source.is_some();
        let is_record_mic = self.builder.is_record_mic && mic_source.is_some();

        let audioenc = self.audioenc().unwrap();

        match (is_record_speaker, is_record_mic) {
            (true, true) => {
                format!("pulsesrc device=\"{}\" ! queue ! audiomixer name=mix ! {} ! queue ! mux. pulsesrc device=\"{}\" ! queue ! mix.",
                    speaker_source.unwrap(),
                    audioenc,
                    mic_source.unwrap()
                )
            }
            (true, false) => {
                format!(
                    "pulsesrc device=\"{}\" ! {} ! queue ! mux.",
                    speaker_source.unwrap(),
                    audioenc
                )
            }
            (false, true) => {
                format!(
                    "pulsesrc device=\"{}\" ! {} ! queue ! mux.",
                    mic_source.unwrap(),
                    audioenc
                )
            }
            (false, false) => "".to_string(),
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
        if let Some(ref coords) = self.builder.coordinates {
            let stream = &self.streams()[0];

            let actual_screen = self.builder.actual_screen.as_ref().unwrap();
            let stream_width = stream.size().unwrap().0;
            let stream_height = stream.size().unwrap().1;

            let scale_factor = (stream_width / actual_screen.width) as f64;
            let coords = coords.clone().rescale(scale_factor);

            let top_crop = coords.y;
            let left_crop = coords.x;
            let right_crop = stream_width as f64 - (coords.width + coords.x);
            let bottom_crop = stream_height as f64 - (coords.height + coords.y);

            // It is a requirement for x264enc to have even resolution.
            Some(format!(
                "videocrop top={} left={} right={} bottom={}",
                utils::round_to_even(top_crop),
                utils::round_to_even(left_crop),
                utils::round_to_even(right_crop),
                utils::round_to_even(bottom_crop)
            ))
        } else {
            None
        }
    }

    fn videoenc(&self) -> String {
        let value = env::var("GST_VAAPI_ALL_DRIVERS").unwrap_or_default();
        let is_use_vaapi = value == "1";
        log::debug!("is_use_vaapi: {}", is_use_vaapi);

        if is_use_vaapi {
            match self.video_format() {
                VideoFormat::Webm | VideoFormat::Mkv => "vaapivp8enc", // FIXME Improve pipelines
                VideoFormat::Mp4 => "vaapih264enc ! h264parse",
                VideoFormat::Gif => "gifenc speed=30", // FIXME This doesn't really use vaapi
            }
        } else {
            match self.video_format() {
                VideoFormat::Webm | VideoFormat::Mkv => "vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T",
                VideoFormat::Mp4 => "x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline",
                VideoFormat::Gif => "gifenc speed=30",
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

    fn fd(&self) -> i32 {
        self.builder.fd
    }

    fn streams(&self) -> &Vec<Stream> {
        &self.builder.streams
    }

    fn is_single_stream(&self) -> bool {
        self.streams().len() == 1
    }
}
