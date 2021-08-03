use ashpd::desktop::screencast::Stream;
use gtk::glib;

use std::{
    env,
    path::{Path, PathBuf},
};

use crate::{
    data_types::{Rectangle, Screen},
    utils,
};

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

#[derive(Debug, Default, Clone)]
pub struct PipelineBuilder {
    streams: Vec<Stream>,
    fd: i32,
    speaker_source: Option<String>,
    mic_source: Option<String>,
    is_record_speaker: bool,
    is_record_mic: bool,
    coordinates: Option<Rectangle>,
    actual_screen: Option<Screen>,
    framerate: u32,
    file_path: PathBuf,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn streams(mut self, streams: Vec<Stream>) -> Self {
        self.streams = streams;
        self
    }

    pub fn fd(mut self, fd: i32) -> Self {
        self.fd = fd;
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

    pub fn record_speaker(mut self, is_record_speaker: bool) -> Self {
        self.is_record_speaker = is_record_speaker;
        self
    }

    pub fn record_mic(mut self, is_record_mic: bool) -> Self {
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
            Some("videorate name=videorate".to_string()),
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

        let mut pipeline_string = pipeline_elements
            .into_iter()
            .flatten()
            .collect::<Vec<String>>()
            .join(" ! ");

        pipeline_string = format!("{} {}", pipeline_string, self.pipewiresrc());
        pipeline_string = match self.audio_source_type() {
            AudioSourceType::Both(speaker_source, mic_source) => format!("{} pulsesrc device=\"{}\" ! queue ! audiomixer name=mix ! {} ! queue ! mux. pulsesrc device=\"{}\" ! queue ! mix.", pipeline_string, speaker_source, self.audioenc().unwrap(), mic_source),
            AudioSourceType::SpeakerOnly(speaker_source) => format!("{} pulsesrc device=\"{}\" ! {} ! queue ! mux.", pipeline_string, speaker_source, self.audioenc().unwrap()),
            AudioSourceType::MicOnly(mic_source) => format!("{} pulsesrc device=\"{}\" ! {} ! queue ! mux.", pipeline_string, mic_source, self.audioenc().unwrap()),
            AudioSourceType::None => pipeline_string,
        };

        pipeline_string.replace("%T", &utils::ideal_thread_count().to_string())
    }

    fn compositor(&self) -> Option<String> {
        if self.is_single_stream() {
            return None;
        }

        let mut current_res = 0;
        let mut compositor_elements = vec!["compositor".to_string(), "name=comp".to_string()];

        for (sink_num, stream) in self.streams().iter().enumerate() {
            let pad = format!("sink_{}::xpos={}", sink_num, current_res);
            compositor_elements.push(pad);

            // This allows us to place the videos size by size with each other, without overlaps.
            let stream_width = stream.size().unwrap().0;
            current_res += stream_width;
        }

        Some(compositor_elements.join(" "))
    }

    fn pipewiresrc(&self) -> String {
        if self.is_single_stream() {
            let node_id = self.streams()[0].pipe_wire_node_id();

            // If there is a single stream, connect pipewiresrc directly to videorate.
            return format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate={}/1 ! queue ! videorate.", self.fd(), node_id, self.framerate());
        }

        let mut pipewiresrc_list = Vec::new();
        for stream in self.streams().iter() {
            let node_id = stream.pipe_wire_node_id();
            pipewiresrc_list.push(format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate={}/1 ! queue ! comp.", self.fd(), node_id, self.framerate()));
        }

        pipewiresrc_list.join(" ")
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

    fn videoenc(&self) -> Option<String> {
        let is_use_vaapi = env::var("GST_VAAPI_ALL_DRIVERS").is_ok();
        log::debug!("is_use_vaapi: {}", is_use_vaapi);

        if is_use_vaapi {
            match self.video_format() {
                VideoFormat::Webm | VideoFormat::Mkv => Some("vaapivp8enc"), // FIXME Improve pipelines
                VideoFormat::Mp4 => Some("vaapih264enc ! h264parse"),
                VideoFormat::Gif => Some("gifenc speed=30 qos=true"), // FIXME This doesn't really use vaapi
            }
        } else {
            match self.video_format() {
                VideoFormat::Webm | VideoFormat::Mkv => Some("vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T"),
                VideoFormat::Mp4 => Some("x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline"),
                VideoFormat::Gif => Some("gifenc speed=30 qos=true"),
            }
        }
        .map(str::to_string)
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

        if video_format == VideoFormat::Gif {
            return None;
        }

        let muxer = match video_format {
            VideoFormat::Webm => "webmmux",
            VideoFormat::Mkv => "matroskamux",
            VideoFormat::Mp4 => "mp4mux",
            VideoFormat::Gif => unreachable!(),
        };

        Some(format!("{} name=mux", muxer))
    }

    fn video_format(&self) -> VideoFormat {
        match self.file_path().extension().unwrap().to_str().unwrap() {
            "webm" => VideoFormat::Webm,
            "mkv" => VideoFormat::Mkv,
            "mp4" => VideoFormat::Mp4,
            "gif" => VideoFormat::Gif,
            other => unreachable!("Invalid video format: {}", other),
        }
    }

    fn audio_source_type(&self) -> AudioSourceType {
        if self.video_format() == VideoFormat::Gif {
            return AudioSourceType::None;
        }

        let speaker_source = self.speaker_source();
        let mic_source = self.mic_source();

        let is_record_speaker = self.builder.is_record_speaker && speaker_source.is_some();
        let is_record_mic = self.builder.is_record_mic && mic_source.is_some();

        match (is_record_speaker, is_record_mic) {
            (true, true) => AudioSourceType::Both(speaker_source.unwrap(), mic_source.unwrap()),
            (true, false) => AudioSourceType::SpeakerOnly(speaker_source.unwrap()),
            (false, true) => AudioSourceType::MicOnly(mic_source.unwrap()),
            (false, false) => AudioSourceType::None,
        }
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

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn no_coordinates() {
//         let stream = Stream {
//             fd: 1,
//             node_id: 32,
//             screen: Screen::new(1680, 1050),
//         };
//         let framerate = 60;
//         let file_path = PathBuf::from("/home/someone/Videos/Kooha 1-1.mp4");
//         let is_record_speaker = true;
//         let is_record_mic = true;
//         let speaker = Some("speaker_device_123".to_string());
//         let mic = Some("microphone_device_123".to_string());

//         let output = PipelineBuilder::new()
//             .pipewire_stream(stream)
//             .framerate(framerate)
//             .file_path(file_path)
//             .record_speaker(is_record_speaker)
//             .record_mic(is_record_mic)
//             .speaker_source(speaker)
//             .mic_source(mic)
//             .parse_into_string();

//         let expected_output = "pipewiresrc fd=1 path=32 do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate=60/1 ! videorate ! video/x-raw, framerate=60/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline ! queue ! mp4mux name=mux ! filesink location=\"/home/someone/Videos/Kooha 1-1.mp4\" pulsesrc device=\"speaker_device_123\" ! queue ! audiomixer name=mix ! opusenc ! queue ! mux. pulsesrc device=\"microphone_device_123\" ! queue ! mix."
//             .replace("%T", &utils::ideal_thread_count().to_string());
//         assert_eq!(output, expected_output);
//     }

//     #[test]
//     fn with_coordinates() {
//         let stream = Stream {
//             fd: 1,
//             node_id: 32,
//             screen: Screen::new(1680, 1050),
//         };
//         let framerate = 60;
//         let file_path = PathBuf::from("/home/someone/Videos/Kooha 1-1.mp4");
//         let is_record_speaker = true;
//         let is_record_mic = true;
//         let speaker = Some("speaker_device_123".to_string());
//         let mic = Some("microphone_device_123".to_string());
//         let coordinates = Rectangle {
//             x: 99_f64,
//             y: 100_f64,
//             width: 20_f64,
//             height: 30_f64,
//         };
//         let actual_screen = Screen::new(30, 40);

//         let output = PipelineBuilder::new()
//             .pipewire_stream(stream)
//             .framerate(framerate)
//             .file_path(file_path)
//             .record_speaker(is_record_speaker)
//             .record_mic(is_record_mic)
//             .speaker_source(speaker)
//             .mic_source(mic)
//             .coordinates(coordinates)
//             .actual_screen(actual_screen)
//             .parse_into_string();

//         let expected_output = "pipewiresrc fd=1 path=32 do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate=60/1 ! videorate ! video/x-raw, framerate=60/1 ! videoscale ! video/x-raw, width=1680, height=1050 ! videocrop top=5600 left=5544 right=-4984 bottom=-6230 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline ! queue ! mp4mux name=mux ! filesink location=\"/home/someone/Videos/Kooha 1-1.mp4\" pulsesrc device=\"speaker_device_123\" ! queue ! audiomixer name=mix ! opusenc ! queue ! mux. pulsesrc device=\"microphone_device_123\" ! queue ! mix."
//             .replace("%T", &utils::ideal_thread_count().to_string());
//         assert_eq!(output, expected_output);
//     }

//     #[test]
//     fn no_both_sources_but_both_true() {
//         let stream = Stream {
//             fd: 1,
//             node_id: 32,
//             screen: Screen::new(1680, 1050),
//         };
//         let framerate = 60;
//         let file_path = PathBuf::from("/home/someone/Videos/Kooha 1-1.mp4");
//         let is_record_speaker = true;
//         let is_record_mic = true;
//         let speaker = None;
//         let mic = None;

//         let output = PipelineBuilder::new()
//             .pipewire_stream(stream)
//             .framerate(framerate)
//             .file_path(file_path)
//             .record_speaker(is_record_speaker)
//             .record_mic(is_record_mic)
//             .speaker_source(speaker)
//             .mic_source(mic)
//             .parse_into_string();

//         let expected_output = "pipewiresrc fd=1 path=32 do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate=60/1 ! videorate ! video/x-raw, framerate=60/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline ! queue ! mp4mux name=mux ! filesink location=\"/home/someone/Videos/Kooha 1-1.mp4\""
//             .replace("%T", &utils::ideal_thread_count().to_string());
//         assert_eq!(output, expected_output);
//     }

//     #[test]
//     fn both_false_but_has_both_sources() {
//         let stream = Stream {
//             fd: 1,
//             node_id: 32,
//             screen: Screen::new(1680, 1050),
//         };
//         let framerate = 60;
//         let file_path = PathBuf::from("/home/someone/Videos/Kooha 1-1.mp4");
//         let is_record_speaker = true;
//         let is_record_mic = true;
//         let speaker = None;
//         let mic = None;

//         let output = PipelineBuilder::new()
//             .pipewire_stream(stream)
//             .framerate(framerate)
//             .file_path(file_path)
//             .record_speaker(is_record_speaker)
//             .record_mic(is_record_mic)
//             .speaker_source(speaker)
//             .mic_source(mic)
//             .parse_into_string();

//         let expected_output = "pipewiresrc fd=1 path=32 do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate=60/1 ! videorate ! video/x-raw, framerate=60/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! x264enc qp-max=17 speed-preset=superfast threads=%T ! video/x-h264, profile=baseline ! queue ! mp4mux name=mux ! filesink location=\"/home/someone/Videos/Kooha 1-1.mp4\""
//             .replace("%T", &utils::ideal_thread_count().to_string());
//         assert_eq!(output, expected_output);
//     }

//     #[test]
//     fn gif_but_audio_enabled_and_60_framerate() {
//         let stream = Stream {
//             fd: 1,
//             node_id: 32,
//             screen: Screen::new(1680, 1050),
//         };
//         let framerate = 60;
//         let file_path = PathBuf::from("/home/someone/Videos/Kooha 1-1.gif");
//         let is_record_speaker = true;
//         let is_record_mic = true;
//         let speaker = Some("speaker_device_123".to_string());
//         let mic = Some("microphone_device_123".to_string());

//         let output = PipelineBuilder::new()
//             .pipewire_stream(stream)
//             .framerate(framerate)
//             .file_path(file_path)
//             .record_speaker(is_record_speaker)
//             .record_mic(is_record_mic)
//             .speaker_source(speaker)
//             .mic_source(mic)
//             .parse_into_string();

//         let expected_output = "pipewiresrc fd=1 path=32 do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw, max-framerate=15/1 ! videorate ! video/x-raw, framerate=15/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! gifenc speed=30 qos=true ! queue ! filesink location=\"/home/someone/Videos/Kooha 1-1.gif\""
//             .replace("%T", &utils::ideal_thread_count().to_string());
//         assert_eq!(output, expected_output);
//     }
// }
