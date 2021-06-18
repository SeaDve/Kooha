# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import GObject, GLib


class TimerState:
    RUNNING = 1
    PAUSED = 2
    STOPPED = 3
    DELAYED = 4


class Timer(GObject.GObject):
    __gtype_name__ = 'Timer'
    __gsignals__ = {'delay-done': (GObject.SignalFlags.RUN_FIRST, None, ())}

    time = GObject.Property(type=int)
    state = GObject.Property(type=int, default=TimerState.STOPPED)

    def __init__(self):
        super().__init__()

    def _refresh_time(self):
        if self.state == TimerState.STOPPED:
            return False

        if self.state != TimerState.PAUSED:
            self.time += -1 if self.state == TimerState.DELAYED else 1

        if self.time == 0 and self.state == TimerState.DELAYED:
            self.state = TimerState.RUNNING
            self.emit('delay-done')
        return True

    def start(self, delay):
        self.time = delay

        GLib.timeout_add_seconds(1, self._refresh_time, priority=GLib.PRIORITY_LOW)
        if not delay:
            self.state = TimerState.RUNNING
            self.emit('delay-done')
        else:
            self.state = TimerState.DELAYED

    def pause(self):
        self.state = TimerState.PAUSED

    def resume(self):
        self.state = TimerState.RUNNING

    def stop(self):
        self.state = TimerState.STOPPED
