from gi.repository import GObject, GLib


class Timer(GObject.GObject):
    __gsignals__ = {'delay-done': (GObject.SignalFlags.RUN_FIRST, None, ())}

    time = GObject.Property(type=int)

    def __init__(self):
        super().__init__()
        self.ongoing = False
        GLib.timeout_add_seconds(1, self._refresh_time, priority=GLib.PRIORITY_LOW)

    def _refresh_time(self):
        if not self.ongoing:
            return True

        self.time += 1
        if self.time == self.delay:
            self.delay = -1
            self.time = 0
            self.emit('delay-done')
        return True

    def start(self, delay):
        self.delay = delay
        self.time = 0
        self.ongoing = True

    def pause(self):
        self.ongoing = False

    def resume(self):
        self.ongoing = True

    def stop(self):
        self.time = 0
        self.ongoing = False
