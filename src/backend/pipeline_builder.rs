use gtk::glib;

use std::cmp::min;
use std::path::PathBuf;

use crate::backend::{Screen, Stream};
use crate::widgets::Rectangle;

const GIF_DEFAULT_FRAMERATE: u32 = 15;

enum AudioSourceType<'a> {
    Both(&'a str, &'a str),
    SpeakerOnly(&'a str),
    MicOnly(&'a str),
    None,
}

#[derive(PartialEq)]
enum VideoFormat {
    Webm,
    Mkv,
    Mp4,
    Gif,
}

#[derive(Default)]
pub struct KhaPipelineBuilder {
    pipewire_stream: Stream,
    speaker_source: Option<String>,
    mic_source: Option<String>,
    is_record_speaker: bool,
    is_record_mic: bool,
    coordinates: Option<Rectangle>,
    actual_screen: Option<Screen>,
    framerate: u32,
    file_path: PathBuf,
}

impl KhaPipelineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pipewire_stream(mut self, pipewire_stream: Stream) -> Self {
        self.pipewire_stream = pipewire_stream;
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

    pub fn speaker_source(mut self, speaker_source: Option<String>) -> Self {
        self.speaker_source = speaker_source;
        self
    }

    pub fn mic_source(mut self, mic_source: Option<String>) -> Self {
        self.mic_source = mic_source;
        self
    }

    pub fn is_record_speaker(mut self, is_record_speaker: bool) -> Self {
        self.is_record_speaker = is_record_speaker;
        self
    }

    pub fn is_record_mic(mut self, is_record_mic: bool) -> Self {
        self.is_record_mic = is_record_mic;
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
        gst::parse_launch(&pipeline_string)
    }
}

struct PipelineParser {
    builder: KhaPipelineBuilder,
}

impl PipelineParser {
    pub fn from_builder(builder: KhaPipelineBuilder) -> Self {
        Self { builder }
    }

    pub fn parse(&self) -> String {
        let pipeline_elements = vec![
            Some(format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true", self.fd() , self.node_id())),
            Some(format!("video/x-raw, max-framerate={}/1", self.framerate())),
            Some("videorate".to_string()),
            Some(format!("video/x-raw, framerate={}/1", self.framerate())),
            self.videoscale(),
            self.videocrop(),
            Some("videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T".to_string()),
            Some("queue".to_string()),
            self.videoenc(),
            Some("queue".to_string()),
            self.muxer(),
            Some(format!("filesink location=\"{}\"", self.file_path().display())),
        ];

        let pipeline_string = pipeline_elements
            .into_iter()
            .flatten()
            .collect::<Vec<String>>()
            .join(" ! ");

        let pipeline_string = match self.audio_source_type() {
            AudioSourceType::Both(speaker_source, mic_source) => format!("{} pulsesrc device=\"{}\" ! queue ! audiomixer name=mix ! {} ! queue ! mux. pulsesrc device=\"{}\" ! queue ! mix.", pipeline_string, speaker_source, self.audioenc().unwrap(), mic_source),
            AudioSourceType::SpeakerOnly(speaker_source) => format!("{} pulsesrc device=\"{}\" ! {} ! queue ! mux.", pipeline_string, speaker_source, self.audioenc().unwrap()),
            AudioSourceType::MicOnly(mic_source) => format!("{} pulsesrc device=\"{}\" ! {} ! queue ! mux.", pipeline_string, mic_source, self.audioenc().unwrap()),
            AudioSourceType::None => pipeline_string,
        };

        pipeline_string.replace("%T", self.ideal_thread_count().to_string().as_ref())
    }

    fn videoscale(&self) -> Option<String> {
        if self.builder.coordinates.is_some() {
            let width = self.builder.pipewire_stream.screen.width;
            let height = self.builder.pipewire_stream.screen.height;

            Some(format!(
                "videoscale ! video/x-raw, width={}, height={}",
                width, height
            ))
        } else {
            None
        }
    }

    fn videocrop(&self) -> Option<String> {
        if let Some(coords) = &self.builder.coordinates {
            let actual_screen_width = self.builder.actual_screen.as_ref().unwrap().width as f64;
            let stream_screen_width = self.builder.pipewire_stream.screen.width as f64;
            let stream_screen_height = self.builder.pipewire_stream.screen.height as f64;

            let scale_factor = stream_screen_width / actual_screen_width;
            let (x, y, width, height) = coords.as_rescaled_tuple(scale_factor);
            let right_crop = stream_screen_width - (width + x);
            let bottom_crop = stream_screen_height - (height + y);

            Some(format!(
                "videocrop top={} left={} right={} bottom={}",
                self.round_to_even(y),
                self.round_to_even(x),
                self.round_to_even(right_crop),
                self.round_to_even(bottom_crop)
            ))
        } else {
            None
        }
    }

    fn videoenc(&self) -> Option<String> {
        match self.video_format() {
            VideoFormat::Webm | VideoFormat::Mkv => Some("vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T".to_string()),
            VideoFormat::Mp4 => Some("x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline".to_string()),
            VideoFormat::Gif => Some("gifenc speed=30 qos=true".to_string()),
        }
    }

    fn audioenc(&self) -> Option<String> {
        match self.video_format() {
            VideoFormat::Webm | VideoFormat::Mkv | VideoFormat::Mp4 => Some("opusenc".to_string()),
            VideoFormat::Gif => None,
        }
    }

    fn muxer(&self) -> Option<String> {
        match self.video_format() {
            VideoFormat::Webm => Some("webmmux".to_string()),
            VideoFormat::Mkv => Some("matroskamux".to_string()),
            VideoFormat::Mp4 => Some("mp4mux".to_string()),
            VideoFormat::Gif => None,
        }
    }

    fn audio_source_type(&self) -> AudioSourceType {
        if self.video_format() == VideoFormat::Gif {
            return AudioSourceType::None;
        }

        let is_record_speaker = self.builder.is_record_speaker && self.speaker_source().is_some();
        let is_record_mic = self.builder.is_record_mic && self.mic_source().is_some();

        let speaker_source = self.speaker_source();
        let mic_source = self.mic_source();

        match (is_record_speaker, is_record_mic) {
            (true, true) => AudioSourceType::Both(speaker_source.unwrap(), mic_source.unwrap()),
            (true, false) => AudioSourceType::SpeakerOnly(speaker_source.unwrap()),
            (false, true) => AudioSourceType::MicOnly(mic_source.unwrap()),
            (false, false) => AudioSourceType::None,
        }
    }

    fn framerate(&self) -> u32 {
        match self.video_format() {
            VideoFormat::Gif => GIF_DEFAULT_FRAMERATE,
            _ => self.builder.framerate,
        }
    }

    fn fd(&self) -> i32 {
        self.builder.pipewire_stream.fd
    }

    fn node_id(&self) -> u32 {
        self.builder.pipewire_stream.node_id
    }

    fn file_path(&self) -> &PathBuf {
        &self.builder.file_path
    }

    fn speaker_source(&self) -> Option<&String> {
        self.builder.speaker_source.as_ref()
    }

    fn mic_source(&self) -> Option<&String> {
        self.builder.mic_source.as_ref()
    }

    fn video_format(&self) -> VideoFormat {
        match self.file_path().extension().unwrap().to_str().unwrap() {
            "webm" => VideoFormat::Webm,
            "mkv" => VideoFormat::Mkv,
            "mp4" => VideoFormat::Mp4,
            "gif" => VideoFormat::Gif,
            _ => unimplemented!(),
        }
    }

    fn round_to_even(&self, number: f64) -> i32 {
        number as i32 / 2 * 2
    }

    fn ideal_thread_count(&self) -> u32 {
        let num_processors = glib::num_processors();
        min(num_processors, 64)
    }
}
