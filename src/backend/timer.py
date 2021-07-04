# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from enum import IntEnum

from gi.repository import GObject, GLib


class Timer(GObject.GObject):
    __gtype_name__ = 'Timer'
    __gsignals__ = {'delay-done': (GObject.SignalFlags.RUN_FIRST, None, ())}

    time = GObject.Property(type=int)
    state = GObject.Property(type=int)

    class State(IntEnum):
        RUNNING = 1
        PAUSED = 2
        STOPPED = 3
        DELAYED = 4

    def __init__(self):
        super().__init__()

        self.state = Timer.State.STOPPED

    def _update_time_value(self):
        if self.state == Timer.State.DELAYED:
            self.time -= 1
        else:
            self.time += 1

    def _on_refresh_time(self):
        if self.state == Timer.State.STOPPED:
            return False

        if self.state != Timer.State.PAUSED:
            self._update_time_value()

        if self.time == 0 and self.state == Timer.State.DELAYED:
            self.state = Timer.State.RUNNING
            self.emit('delay-done')
        return True

    def start(self, delay):
        self.time = delay

        GLib.timeout_add_seconds(1, self._on_refresh_time,
                                 priority=GLib.PRIORITY_LOW)
        if not delay:
            self.state = Timer.State.RUNNING
            self.emit('delay-done')
        else:
            self.state = Timer.State.DELAYED

    def pause(self):
        self.state = Timer.State.PAUSED

    def resume(self):
        self.state = Timer.State.RUNNING

    def stop(self):
        self.state = Timer.State.STOPPED
