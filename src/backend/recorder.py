# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from subprocess import PIPE, Popen

from gi.repository import GObject, Gst

from kooha.backend.portal import Portal
from kooha.backend.settings import Settings
from kooha.backend.pipeline_builder import PipelineBuilder

Gst.init(None)

# TODO implement area recording
# TODO fix pause and resume with pipewire
# TODO test if saving_location exists before recording
# TODO autoadjust resolution in window mode


class Recorder(GObject.GObject):
    __gtype_name__ = 'Recorder'
    __gsignals__ = {'ready': (GObject.SIGNAL_RUN_FIRST, None, ())}

    _state = Gst.State.NULL

    def __init__(self):

        self.portal = Portal()
        self.portal.connect('ready', self._on_portal_ready)
        self.settings = Settings()

    @GObject.Property(type=Gst.State, default=_state)
    def state(self):
        return self._state

    @state.setter
    def state(self, pipeline_state):
        self._state = pipeline_state
        self.pipeline.set_state(pipeline_state)

    def _on_portal_ready(self, portal, fd, node_id):
        framerate = self.settings.get_video_framerate()
        file_path = self.settings.get_file_path().replace(" ", r"\ ")
        video_format = self.settings.get_video_format()
        audio_source_type = self.settings.get_audio_option()
        default_audio_sources = self._get_default_audio_sources()

        pipeline_builder = PipelineBuilder(fd, node_id, framerate, file_path,
                                           video_format, audio_source_type)
        pipeline_builder.set_audio_source(*default_audio_sources)
        self.pipeline = pipeline_builder.build()
        self.emit('ready')

    def _on_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.state = Gst.State.NULL
            self.record_bus.remove_watch()
            self.record_bus.disconnect(self.handler_id)
            self.portal.close()
        elif t == Gst.MessageType.ERROR:
            err, debug = message.parse_error()
            print("Error: %s" % err, debug)

    def _get_default_audio_sources(self):
        pactl_output = Popen(
            'pactl info | tail -n +13 | cut -d" " -f3',
            shell=True,
            text=True,
            stdout=PIPE
        ).stdout.read().rstrip()
        device_list = pactl_output.split("\n")
        default_sink = f'{device_list[0]}.monitor'
        default_source = device_list[1]
        if default_sink == default_source:
            return default_sink, None
        return default_sink, default_source

    def ready(self):
        draw_pointer = self.settings.get_is_show_pointer()
        self.portal.open(draw_pointer)

    def start(self):
        self.record_bus = self.pipeline.get_bus()
        self.record_bus.add_signal_watch()
        self.handler_id = self.record_bus.connect('message', self._on_gst_message)
        self.state = Gst.State.PLAYING

    def pause(self):
        self.state = Gst.State.PAUSED

    def resume(self):
        self.state = Gst.State.PLAYING

    def stop(self):
        self.pipeline.send_event(Gst.Event.new_eos())
