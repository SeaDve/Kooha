# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

import logging
import subprocess
from collections import namedtuple

from gi.repository import GObject, Gst

from kooha.backend.screencast_portal import ScreencastPortal
from kooha.backend.settings import Settings
from kooha.backend.pipeline_builder import PipelineBuilder
from kooha.widgets.area_selector import AreaSelector

logger = logging.getLogger(__name__)
Screen = namedtuple('Screen', 'w h')
Selection = namedtuple('Selection', 'x y w h')


class Recorder(GObject.GObject):
    __gtype_name__ = 'Recorder'
    __gsignals__ = {'ready': (GObject.SIGNAL_RUN_FIRST, None, ()),
                    'record-success': (GObject.SIGNAL_RUN_FIRST, None, (str, )),
                    'record-failed': (GObject.SIGNAL_RUN_FIRST, None, (str, ))}

    is_readying = GObject.Property(type=bool, default=False)
    _state = Gst.State.NULL

    def __init__(self):

        self.settings = Settings()
        self.area_selector = AreaSelector()

        self.portal = ScreencastPortal()
        self.portal.connect('ready', self._on_portal_ready)
        self.portal.connect('cancelled', self._on_portal_cancelled)

    @GObject.Property(type=Gst.State, default=_state)
    def state(self):
        return self._state

    @state.setter
    def state(self, pipeline_state):
        self._state = pipeline_state
        self.pipeline.set_state(pipeline_state)
        logger.info(f"Pipeline set to {pipeline_state} ")

    def _on_portal_ready(self, portal, fd, node_id,
                         stream_screen_w, stream_screen_h,
                         is_selection_mode):
        framerate = self.settings.get_video_framerate()
        file_path = self.settings.get_file_path().replace(" ", r"\ ")
        video_format = self.settings.get_video_format()
        audio_source_type = self.settings.get_audio_option()
        default_audio_sources = self._get_default_audio_sources()

        logger.info(f"fd, node_id: {fd}, {node_id}")
        logger.info(f"framerate: {framerate}")
        logger.info(f"file_path: {file_path}")
        logger.info(f"audio_source_type: {audio_source_type}")
        logger.info(f"audio_sources: {default_audio_sources}")

        pipeline_builder = PipelineBuilder(fd, node_id, framerate, file_path,
                                           video_format, audio_source_type)
        pipeline_builder.set_audio_source(*default_audio_sources)

        def emit_ready():
            self.is_readying = False
            self.pipeline = pipeline_builder.build()
            self.emit('ready')

        def on_area_selector_captured(area_selector, x, y, w, h, scr_w, scr_h):
            stream_screen = Screen(stream_screen_w, stream_screen_h)
            actual_screen = Screen(scr_w, scr_h)

            logger.info(f"selected_coordinates: {x, y, w, h}")
            logger.info(f"stream screen_info: {stream_screen.w} {stream_screen.h}")
            logger.info(f"actual screen_info: {actual_screen.w} {actual_screen.h}")

            selection = (x, y, w, h)
            pipeline_builder.set_coordinates(selection, stream_screen, actual_screen)

            self._clean_area_selector()
            emit_ready()

        def on_area_selector_cancelled(area_selector):
            self.is_readying = False
            self._clean_area_selector()
            self.portal.close()

        if is_selection_mode:
            self.captured_id = self.area_selector.connect('captured', on_area_selector_captured)
            self.cancelled_id = self.area_selector.connect('cancelled', on_area_selector_cancelled)
            self.area_selector.select_area()
            return

        emit_ready()

    def _on_portal_cancelled(self, portal):
        self.is_readying = False

    def _on_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self._clean_pipeline()
            self.emit('record-success', self.settings.get_saving_location())
        elif t == Gst.MessageType.ERROR:
            error, debug = message.parse_error()
            self._clean_pipeline()
            self.emit('record-failed', error)
            logger.error(f"{error} {debug}")

    def _clean_pipeline(self):
        self.state = Gst.State.NULL
        self.record_bus.remove_watch()
        self.record_bus.disconnect(self.handler_id)
        self.portal.close()

    def _clean_area_selector(self):
        self.area_selector.disconnect(self.captured_id)
        self.area_selector.disconnect(self.cancelled_id)
        self.area_selector.hide()

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

        logger.info(f"is_show_pointer: {is_show_pointer}")
        logger.info(f"is_selection_mode: {is_selection_mode}")

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
