# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

import subprocess

from gi.repository import GObject, Gst

from kooha.logger import Logger
from kooha.backend.screencast_portal import ScreencastPortal
from kooha.backend.settings import Settings
from kooha.backend.pipeline_builder import PipelineBuilder
from kooha.widgets.area_selector import AreaSelector


class Recorder(GObject.GObject):
    __gtype_name__ = 'Recorder'
    __gsignals__ = {'ready': (GObject.SignalFlags.RUN_FIRST, None, ()),
                    'record-success': (GObject.SignalFlags.RUN_FIRST, None, (str, )),
                    'record-failed': (GObject.SignalFlags.RUN_FIRST, None, (str, ))}

    is_readying = GObject.Property(type=bool, default=False)
    _state = Gst.State.NULL

    def __init__(self):

        self.settings = Settings()

        self.area_selector = AreaSelector()
        self.area_selector.connect('captured', self._on_area_selector_captured)
        self.area_selector.connect('cancelled', self._on_area_selector_cancelled)

        self.portal = ScreencastPortal()
        self.portal.connect('ready', self._on_portal_ready)
        self.portal.connect('cancelled', self._on_portal_cancelled)

    @GObject.Property(type=Gst.State, default=_state)
    def state(self):
        return self._state

    @state.setter  # type: ignore
    def state(self, pipeline_state):
        self._state = pipeline_state
        self.pipeline.set_state(pipeline_state)
        Logger.debug(f"Pipeline set to {pipeline_state}")

    def _on_portal_ready(self, portal, pipewire_stream, is_selection_mode):
        framerate = self.settings.get_video_framerate()
        file_path = self.settings.get_file_path()
        video_format = self.settings.get_video_format()
        audio_source_type = self.settings.get_audio_option()
        default_audio_sources = self._get_default_audio_sources()

        self.pipeline_builder = PipelineBuilder(pipewire_stream)
        self.pipeline_builder.set_settings(framerate, file_path, video_format, audio_source_type)
        self.pipeline_builder.set_audio_source(*default_audio_sources)

        if is_selection_mode:
            self.area_selector.select_area()
            return

        self._build_pipeline()

    def _on_portal_cancelled(self, portal, error_message):
        self.is_readying = False
        if error_message:
            self.emit('record-failed', error_message)

    def _on_area_selector_captured(self, area_selector, selection, actual_screen):
        self.pipeline_builder.set_coordinates(selection, actual_screen)
        self._build_pipeline()

    def _on_area_selector_cancelled(self, area_selector):
        self.is_readying = False
        self.portal.close()

    def _on_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self._clean_pipeline()
            self.emit('record-success', self.settings.get_saving_location())
        elif t == Gst.MessageType.ERROR:
            error, debug = message.parse_error()
            self._clean_pipeline()
            self.emit('record-failed', error)
            Logger.debug(f"{error} {debug}")

    def _build_pipeline(self):
        self.pipeline = self.pipeline_builder.build()
        self.is_readying = False
        self.emit('ready')

        Logger.debug(self.pipeline_builder)

    def _clean_pipeline(self):
        self.state = Gst.State.NULL
        self.record_bus.remove_signal_watch()
        self.record_bus.disconnect(self.handler_id)
        self.portal.close()

    def _get_default_audio_sources(self):
        pactl_output = subprocess.run(
            ['/usr/bin/pactl', 'info'],
            stdout=subprocess.PIPE,
            text=True
        ).stdout.splitlines()
        default_sink = f'{pactl_output[12].split()[2]}.monitor'
        default_source = pactl_output[13].split()[2]
        if default_sink == default_source:
            return default_sink, None
        return default_sink, default_source

    def ready(self):
        self.is_readying = True
        is_show_pointer = self.settings.get_is_show_pointer()
        is_selection_mode = self.settings.get_is_selection_mode()
        self.portal.open(is_show_pointer, is_selection_mode)

        Logger.debug(f"is_show_pointer: {is_show_pointer}")
        Logger.debug(f"is_selection_mode: {is_selection_mode}")

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
