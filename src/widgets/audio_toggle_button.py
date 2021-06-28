from gi.repository import Gtk, GObject, GLib


class AudioToggleButton(Gtk.ToggleButton):
    __gtype_name__ = 'AudioToggleButton'

    _action_enabled = False

    def __init__(self):
        super().__init__()

    @GObject.Property(type=bool, default=_action_enabled)
    def action_enabled(self):
        return self._action_enabled

    @action_enabled.setter  # type: ignore
    def action_enabled(self, is_action_enabled):
        self._action_enabled = is_action_enabled
        GLib.timeout_add(5, lambda: self.set_sensitive(is_action_enabled))
