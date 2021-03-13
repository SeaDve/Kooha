# timers.py
#
# Copyright 2021 SeaDve
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from gi.repository import GLib


class Timer:
    def __init__(self, label):
        self.label = label
        self.ongoing = False
        GLib.timeout_add_seconds(1, self.refresh_time, priority=GLib.PRIORITY_LOW)

    def refresh_time(self):
        if self.ongoing:
            self.time += 1
            self.label.set_text("%02d∶%02d" % divmod(self.time, 60))
        return True

    def start(self):
        self.time = 0
        self.label.set_text("00∶00")
        self.ongoing = True

    def stop(self):
        self.ongoing = False


class DelayTimer:
    def __init__(self, label, function):
        self.label = label
        self.function = function

    def displaydelay(self):
        if self.time_delay == 10 or self.delaycancel:
            if not self.delaycancel:
                self.function()
            return False
        self.time_delay -= 10
        self.label.set_text(str(self.time_delay // 100 + 1))
        return True

    def start(self, time_delay):
        if time_delay > 0:
            self.time_delay = time_delay * 100
            self.delaycancel = False
            self.label.set_text(str(time_delay))
            GLib.timeout_add(100, self.displaydelay)
        else:
            self.function()

    def cancel(self):
        self.delaycancel = True
