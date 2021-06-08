# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import GObject, Gst, GLib

ENCODING_PROFILES = {
    'webm': {
        'muxer': 'webmmux',
        'video_enc': 'vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 '
                     'static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T',
        'audio_enc': 'opusenc',
    },
    'mkv': {
        'muxer': 'matroskamux',
        'video_enc': 'vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 '
                     'static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=%T',
        'audio_enc': 'opusenc',
    },
    'mp4': {
        'muxer': 'mp4mux',
        'video_enc': 'x264enc qp-min=17 qp-max=17 speed-preset=1 threads=%T '
                     '! video/x-h264, profile=baseline',
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
        self.coordinates = None

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

    def _get_cropper(self):
        def even(num):
            return num // 2 * 2

        if self.coordinates:
            x, y, width, height = self.coordinates
            right_crop = self.screen_width - (width + x)
            bottom_crop = self.screen_height - (height + y)
            return (f' videocrop top={even(y)} left={even(x)}'
                    f' right={even(right_crop)} bottom={even(bottom_crop)} !')
        return ''

    def set_audio_source(self, speaker_source, mic_source):
        self.speaker_source = speaker_source
        self.mic_source = mic_source

    def set_coordinates(self, coordinates, screen_width, screen_height):
        self.coordinates = coordinates
        self.screen_width = screen_width
        self.screen_height = screen_height

    def build(self):
        is_record_speaker = self.audio_source_type.record_speaker and self.speaker_source
        is_record_mic = self.audio_source_type.record_mic and self.mic_source
        if is_record_speaker:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path} pulsesrc device="{self.speaker_source}" ! {self._get_audio_enc()} ! queue ! mux.'  # noqa: E501
        elif is_record_mic:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path} pulsesrc device="{self.mic_source}" ! {self._get_audio_enc()} ! queue ! mux.'  # noqa: E501
        else:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path}'  # noqa: E501
        if is_record_speaker and is_record_mic:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self.framerate}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue ! {self._get_muxer()} name=mux ! filesink location={self.file_path} pulsesrc device="{self.mic_source}" ! queue ! audiomixer name=mix ! {self._get_audio_enc()} ! queue ! mux. pulsesrc device="{self.speaker_source}" ! queue ! mix.'  # noqa: E501
        pipeline_string = pipeline_string.replace('%T', str(self._get_thread_count()))
        return Gst.parse_launch(pipeline_string)
