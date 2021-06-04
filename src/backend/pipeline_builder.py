from gi.repository import GObject, Gst, GLib

ENCODING_PROFILES = {
    'webm': {
        'muxer': 'webmmux',
        'video_enc': 'vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T',
        'audio_enc': 'opusenc',
    },
    'mkv': {
        'muxer': 'matroskamux',
        'video_enc': 'vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T',
        'audio_enc': 'opusenc',
    },
    'mp4': {
        'muxer': 'mp4mux',
        'video_enc': 'x264enc qp-min=17 qp-max=17 speed-preset=1 threads=%T ! video/x-h264, profile=baseline',
        'audio_enc': 'opusenc',
    },
}


class PipelineBuilder(GObject.GObject):

    def __init__(self, fd, node_id, framerate, file_path, video_format, audio_source_type):
        self.fd = fd
        self.node_id = node_id
        self.framerate = framerate
        self.file_path = file_path
        self.video_format = video_format
        self.audio_source_type = audio_source_type
        self.speaker_source = None
        self.mic_source = None

    def _get_muxer(self):
        return ENCODING_PROFILES[self.video_format]['muxer']

    def _get_video_enc(self):
        return ENCODING_PROFILES[self.video_format]['video_enc']

    def _get_audio_enc(self):
        return ENCODING_PROFILES[self.video_format]['audio_enc']

    def _get_thread_count(self):
        num_processors = GLib.get_num_processors()
        num_threads = min(max(1, num_processors), 64)
        return num_threads

    def set_audio_source(self, speaker_source, mic_source):
        self.speaker_source = speaker_source
        self.mic_source = mic_source

    def build(self):
        is_record_speaker = self.audio_source_type.record_speaker and self.speaker_source
        is_record_mic = self.audio_source_type.record_mic and self.mic_source
        if is_record_speaker:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path} pulsesrc device="{self.speaker_source}" ! {self._get_audio_enc()} ! queue ! mux.'
        elif is_record_mic:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path} pulsesrc device="{self.mic_source}" ! {self._get_audio_enc()} ! queue ! mux.'
        else:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path}'
        if is_record_speaker and is_record_mic:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T ! queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path} pulsesrc device="{self.mic_source}" ! queue ! audiomixer name=mix ! {self._get_audio_enc()} ! queue ! mux. pulsesrc device="{self.speaker_source}" ! queue ! mix.'
        pipeline_string = pipeline_string.replace('%T', str(self._get_thread_count()))
        return Gst.parse_launch(pipeline_string)
