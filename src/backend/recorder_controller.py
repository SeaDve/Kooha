# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from enum import IntEnum

from gi.repository import GObject, Gst

from kooha.backend.timer import Timer
from kooha.backend.recorder import Recorder


class RecorderController(GObject.GObject):
    """Controls the states of Timer and Recorder"""

    __gtype_name__ = 'RecorderController'
    __gsignals__ = {'record-success': (GObject.SignalFlags.RUN_FIRST, None, (str, )),
                    'record-failed': (GObject.SignalFlags.RUN_FIRST, None, (str, ))}

    time = GObject.Property(type=int)
    state = GObject.Property(type=int)
    is_readying = GObject.Property(type=bool, default=False)

    class State(IntEnum):
        PLAYING = 1
        PAUSED = 2
        NULL = 3
        DELAYED = 4

    def __init__(self):
        super().__init__()

        self.timer = Timer()
        self.recorder = Recorder()
        self._connect_signals()

    def _connect_signals(self):
        self.timer.bind_property('time', self, 'time')
        self.timer.connect('notify::state', self._on_timer_state_notify)
        self.timer.connect('delay-done', self._on_timer_delay_done)

        self.recorder.bind_property('is-readying', self, 'is-readying')
        self.recorder.connect('notify::state', self._on_recorder_state_notify)
        self.recorder.connect('ready', self._on_recorder_ready)
        self.recorder.connect('record-success', self._on_recorder_record_success)
        self.recorder.connect('record-failed', self._on_recorder_record_failed)

    def _on_timer_state_notify(self, timer, pspec):
        if timer.state == Timer.State.DELAYED:
            self.state = RecorderController.State.DELAYED
        elif timer.state == Timer.State.STOPPED:
            self.state = RecorderController.State.NULL

    def _on_recorder_state_notify(self, recorder, pspec):
        if recorder.state == Gst.State.NULL:
            self.timer.stop()
            # No need to set state of RecorderController to NULL because that
            # is already handled by _on_timer_state_notify. This is to avoid
            # double emission of notify::state of this class.
        elif recorder.state == Gst.State.PLAYING:
            self.timer.resume()
            self.state = RecorderController.State.PLAYING
        elif recorder.state == Gst.State.PAUSED:
            self.timer.pause()
            self.state = RecorderController.State.PAUSED

    def _on_timer_delay_done(self, timer):
        self.recorder.start()

    def _on_recorder_ready(self, recorder):
        self.timer.start(self.record_delay)

    def _on_recorder_record_success(self, recorder, recording_file_path):
        self.emit('record-success', recording_file_path)

    def _on_recorder_record_failed(self, recorder, error_message):
        self.emit('record-failed', error_message)

    def start(self, record_delay):
        self.record_delay = record_delay
        self.recorder.ready()

    def cancel_delay(self):
        self.recorder.portal.close()
        self.timer.stop()

    def stop(self):
        self.recorder.stop()

    def pause(self):
        self.recorder.pause()

    def resume(self):
        self.recorder.resume()
