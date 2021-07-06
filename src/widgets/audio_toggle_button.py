# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import Gtk, GObject, GLib


class AudioToggleButton(Gtk.ToggleButton):
    """Wrapper for toggle button to disable action"""

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

        # This is a workaround. For some reason, sensitive property doesn't
        # get updated on the widget construction, so we have to add 5ms delay.
        GLib.timeout_add(5, lambda: self.set_sensitive(is_action_enabled))
