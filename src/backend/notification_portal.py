# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import GObject, Gio, GLib


class NotificationPortal(GObject.GObject):
    __gtype_name__ = 'NotificationPortal'

    def __init__(self):
        super().__init__()

        self.proxy = Gio.DBusProxy.new_for_bus_sync(
            Gio.BusType.SESSION,
            Gio.DBusProxyFlags.NONE,
            None,
            'org.freedesktop.portal.Desktop',
            '/org/freedesktop/portal/desktop',
            'org.freedesktop.portal.Notification',
            None
        )

    def send_notification(self, title, body, action):
        self.proxy.AddNotification(
            '(sa{sv})',
            'io.github.seadve.Kooha',
            {
                'title': GLib.Variant.new_string(title),
                'body': GLib.Variant.new_string(body),
                'default-action': GLib.Variant.new_string(action)
            }
        )
