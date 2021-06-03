from gi.repository import GObject, GLib


class TimerState:
    RUNNING = 1
    STOPPED = 2
    DELAYED = 3


class Timer(GObject.GObject):
    __gtype_name__ = 'Timer'
    __gsignals__ = {'delay-done': (GObject.SignalFlags.RUN_FIRST, None, ())}

    time = GObject.Property(type=int)
    state = GObject.Property(type=int)

    def __init__(self):
        super().__init__()
        self.state = TimerState.STOPPED
        GLib.timeout_add_seconds(1, self._refresh_time, priority=GLib.PRIORITY_LOW)

    def _refresh_time(self):
        if self.state == TimerState.STOPPED:
            return True

        if self.time == 0:
            self.emit('delay-done')
            self.state = TimerState.RUNNING
        self.time += -1 if self.state == TimerState.DELAYED else 1

        return True

    def start(self, delay):
        if not delay:
            self.emit('delay-done')
            self.state = TimerState.RUNNING
        else:
            self.state = TimerState.DELAYED
        self.delay = delay
        self.time = delay

    def pause(self):
        self.state = TimerState.STOPPED

    def resume(self):
        self.state = TimerState.RUNNING

    def stop(self):
        self.state = TimerState.STOPPED
