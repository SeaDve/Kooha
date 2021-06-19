# SPDX-FileCopyrightText: Copyright 2018-2021 Jonas Adahl
# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

import logging

from gi.repository import GObject, GLib, Gio

logger = logging.getLogger(__name__)


class ScreencastPortal(GObject.GObject):
    __gsignals__ = {'ready': (GObject.SIGNAL_RUN_FIRST, None, (int, int, int, int, bool)),
                    'cancelled': (GObject.SIGNAL_RUN_FIRST, None, ())}

    def __init__(self):
        super().__init__()

        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.proxy = Gio.DBusProxy.new_sync(
            self.bus,
            Gio.DBusProxyFlags.NONE,
            None,
            'org.freedesktop.portal.Desktop',
            '/org/freedesktop/portal/desktop',
            'org.freedesktop.portal.ScreenCast',
            None,
        )

        self.sender_name = self.bus.get_unique_name()[1:].replace('.', '_')
        self.request_counter = 0
        self.session_counter = 0

    def _new_session_path(self):
        self.session_counter += 1
        token = f'u{self.session_counter}'
        path = f'/org/freedesktop/portal/desktop/session/{self.sender_name}/{token}'
        return path, token

    def _new_request_path(self):
        self.request_counter += 1
        token = f'u{self.request_counter}'
        path = f'/org/freedesktop/portal/desktop/request/{self.sender_name}/{token}'
        return path, token

    def _screencast_call(self, method, callback, signature, *args, options={}):
        request_path, request_token = self._new_request_path()
        self.bus.signal_subscribe(
            'org.freedesktop.portal.Desktop',
            'org.freedesktop.portal.Request',
            'Response',
            request_path,
            None,
            Gio.DBusSignalFlags.NONE,
            callback,
        )
        options['handle_token'] = GLib.Variant('s', request_token)
        method(signature, *args, options)

    def _on_create_session_response(self, bus, sender, path, request_path, node, output):
        response, results = output
        if response != 0:
            logger.warning(f"Failed to create session: {response}")
            return

        self.session_handle = results['session_handle']
        logger.info("Session created")
        self._screencast_call(
            self.proxy.SelectSources,
            self._on_select_sources_response,
            '(oa{sv})',
            self.session_handle,
            options={
                'types': GLib.Variant('u', 1 if self.is_selection_mode else 1 | 2),
                'cursor_mode': GLib.Variant('u', 2 if self.is_show_pointer else 1)
            }
        )

    def _on_select_sources_response(self, bus, sender, path, request_path, node, output):
        response, results = output
        if response != 0:
            logger.warning(f"Failed to select sources: {response}")
            return

        logger.info("Sources selected")
        self._screencast_call(
            self.proxy.Start,
            self._on_start_response,
            '(osa{sv})',
            self.session_handle,
            '',
        )

    def _on_start_response(self, bus, sender, path, request_path, node, output):
        response, results = output
        if response != 0:
            self.emit('cancelled')
            logger.warning(f"Failed to start: {response}")
            return

        logger.info("Ready for pipewire stream")
        for node_id, stream_info in results['streams']:
            logger.info(f"stream {node_id}")
            response, results = self.proxy.call_with_unix_fd_list_sync(
                'OpenPipeWireRemote',
                GLib.Variant('(oa{sv})', (self.session_handle, {})),
                Gio.DBusCallFlags.NONE,
                -1,
                None,
                None,
            )
            fd = results.get(0)
            screen_width, screen_height = stream_info['size']
            self.emit('ready', fd, node_id, screen_width, screen_height, self.is_selection_mode)

    def open(self, is_show_pointer, is_selection_mode):
        self.is_show_pointer = is_show_pointer
        self.is_selection_mode = is_selection_mode

        _, session_token = self._new_session_path()
        self._screencast_call(
            self.proxy.CreateSession,
            self._on_create_session_response,
            '(a{sv})',
            options={
                'session_handle_token': GLib.Variant('s', session_token),
            }
        )

    def close(self):
        Gio.DBusProxy.new_sync(
            self.bus,
            Gio.DBusProxyFlags.NONE,
            None,
            'org.freedesktop.portal.Desktop',
            self.session_handle,
            'org.freedesktop.portal.Session',
            None,
        ).Close()

        logger.info("Portal closed")
