# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import Gst, GLib

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
        'video_enc': 'x264enc qp-max=17 speed-preset=superfast threads=%T '
                     '! video/x-h264, profile=baseline',
        'audio_enc': 'opusenc',
    },
    'gif': {
        'muxer': None,
        'video_enc': 'gifenc qos=true'
    },
}


class PipelineBuilder:

    def __init__(self, pipewire_stream):
        self.fd = pipewire_stream.fd
        self.node_id = pipewire_stream.node_id
        self.stream_screen = pipewire_stream.screen
        self.speaker_source = None
        self.mic_source = None
        self.coordinates = None

    def _even_out(self, *numbers):
        return (int(number // 2 * 2) for number in numbers)

    def _rescale(self, coordinates):
        scale_factor = self.stream_screen.w / self.actual_screen.w
        return (coordinate * scale_factor for coordinate in coordinates)

    def _get_thread_count(self):
        num_processors = GLib.get_num_processors()
        return min(num_processors, 64)

    def _get_video_enc(self):
        return ENCODING_PROFILES[self.video_format]['video_enc']

    def _get_audio_enc(self):
        return ENCODING_PROFILES[self.video_format]['audio_enc']

    def _get_muxer(self):
        muxer_name = ENCODING_PROFILES[self.video_format]['muxer']
        if not muxer_name:
            return ''

        return f' {muxer_name} name=mux !'

    def _get_scaler(self):
        if not self.coordinates:
            return ''

        return (f' videoscale ! video/x-raw, width={self.stream_screen.w},'
                f' height={self.stream_screen.h} !')

    def _get_cropper(self):
        if not self.coordinates:
            return ''

        x, y, width, height = self._rescale(self.coordinates)
        right_crop = (self.stream_screen.w - (width + x))
        bottom_crop = (self.stream_screen.h - (height + y))
        x, y, right_crop, bottom_crop = self._even_out(x, y, right_crop, bottom_crop)

        return (f' videocrop top={y} left={x}'
                f' right={right_crop} bottom={bottom_crop} !')

    def _get_proper_framerate(self):
        if self.video_format == 'gif':
            return 15

        return self.framerate

    def _get_audio_configuration(self):
        if self.video_format == 'gif':
            return False, False

        is_record_speaker = self.audio_source_type.record_speaker and self.speaker_source
        is_record_mic = self.audio_source_type.record_mic and self.mic_source
        return is_record_speaker, is_record_mic

    def set_settings(self, framerate, file_path, video_format, audio_source_type):
        self.framerate = framerate
        self.file_path = file_path
        self.video_format = video_format
        self.audio_source_type = audio_source_type

    def set_audio_source(self, speaker_source, mic_source):
        self.speaker_source = speaker_source
        self.mic_source = mic_source

    def set_coordinates(self, coordinates, actual_screen):
        self.coordinates = coordinates
        self.actual_screen = actual_screen

    def build(self):
        is_record_speaker, is_record_mic = self._get_audio_configuration()
        if is_record_speaker:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self._get_proper_framerate()}/1 !{self._get_scaler()} videorate ! video/x-raw,framerate={self._get_proper_framerate()}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue !{self._get_muxer()} filesink location={self.file_path} pulsesrc device="{self.speaker_source}" ! {self._get_audio_enc()} ! queue ! mux.'  # noqa: E501
        elif is_record_mic:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self._get_proper_framerate()}/1 !{self._get_scaler()} videorate ! video/x-raw,framerate={self._get_proper_framerate()}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue !{self._get_muxer()} filesink location={self.file_path} pulsesrc device="{self.mic_source}" ! {self._get_audio_enc()} ! queue ! mux.'  # noqa: E501
        else:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self._get_proper_framerate()}/1 !{self._get_scaler()} videorate ! video/x-raw,framerate={self._get_proper_framerate()}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue !{self._get_muxer()} filesink location={self.file_path}'  # noqa: E501
        if is_record_speaker and is_record_mic:
            pipeline_string = f'pipewiresrc fd={self.fd} path={self.node_id} do-timestamp=true keepalive-time=1000 resend-last=true ! video/x-raw,max-framerate={self._get_proper_framerate()}/1 !{self._get_scaler()} videorate ! video/x-raw,framerate={self._get_proper_framerate()}/1 ! videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=%T !{self._get_cropper()} queue ! {self._get_video_enc()} ! queue !{self._get_muxer()} filesink location={self.file_path} pulsesrc device="{self.mic_source}" ! queue ! audiomixer name=mix ! {self._get_audio_enc()} ! queue ! mux. pulsesrc device="{self.speaker_source}" ! queue ! mix.'  # noqa: E501
        pipeline_string = pipeline_string.replace('%T', str(self._get_thread_count()))
        return Gst.parse_launch(pipeline_string)
