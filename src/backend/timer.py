from gi.repository import GObject, GLib


class Timer(GObject.GObject):
    __gtype_name__ = 'Timer'
    __gsignals__ = {'delay-done': (GObject.SignalFlags.RUN_FIRST, None, ())}

    time = GObject.Property(type=int)

    def __init__(self):
        super().__init__()
        self.ongoing = False
        GLib.timeout_add_seconds(1, self._refresh_time, priority=GLib.PRIORITY_LOW)

    def _refresh_time(self):
        if not self.ongoing:
            return True

        if self.time == 0:
            self.delay = None
            self.emit('delay-done')
        self.time += -1 if self.delay else 1

        return True

    def start(self, delay):
        if not delay:
            self.emit('delay-done')
        self.delay = delay
        self.time = delay
        self.ongoing = True

    def pause(self):
        self.ongoing = False

    def resume(self):
        self.ongoing = True

    def stop(self):
        self.ongoing = False