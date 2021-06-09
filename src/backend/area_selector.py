# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later


from gi.repository import Gio


class AreaSelector():

    def __init__(self):
        self.proxy = Gio.DBusProxy.new_for_bus_sync(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            None,
            'org.gnome.Shell.Screenshot',
            '/org/gnome/Shell/Screenshot',
            'org.gnome.Shell.Screenshot',
            None
        )

    def select_area(self):
        return self.proxy.SelectArea()
