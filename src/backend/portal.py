import logging
import dbus
from dbus.mainloop.glib import DBusGMainLoop

from gi.repository import GObject

# TODO Close the session after use
# TODO Use Giodbus and remove dbus dep


class Portal(GObject.GObject):
    __gsignals__ = {'ready': (GObject.SIGNAL_RUN_FIRST, None, (int, int))}

    def __init__(self):
        super().__init__()

        DBusGMainLoop(set_as_default=True)
        self.bus = dbus.SessionBus()
        self.sender_name = self.bus.get_unique_name()[1:].replace('.', '_')
        self.proxy = dbus.Interface(
            self.bus.get_object(
                'org.freedesktop.portal.Desktop',
                '/org/freedesktop/portal/desktop'
            ),
            'org.freedesktop.portal.ScreenCast',
        )

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

    def _screencast_call(self, method, callback, *args, options={}):
        request_path, request_token = self._new_request_path()
        self.bus.add_signal_receiver(
            callback,
            'Response',
            'org.freedesktop.portal.Request',
            'org.freedesktop.portal.Desktop',
            request_path
        )
        options['handle_token'] = request_token
        method(*(args + (options, )))

    def _on_create_session_response(self, response, results):
        if response:
            logging.error(f"Failed to create session: {response}")
        else:
            self.session_handle = results['session_handle']
            logging.info("Session created")
            self._screencast_call(
                self.proxy.SelectSources,
                self._on_select_sources_response,
                self.session_handle,
                options={
                    'types': dbus.UInt32(1 | 2),  # Which source
                    'cursor_mode': dbus.UInt32(2 if self.draw_pointer else 1)
                }
            )

    def _on_select_sources_response(self, response, results):
        if response:
            logging.error(f"Failed to select sources: {response}")
        else:
            logging.info("Sources selected")
            self._screencast_call(
                self.proxy.Start,
                self._on_start_response,
                self.session_handle,
                ''
            )

    def _on_start_response(self, response, results):
        if response:
            logging.error(f"Failed to start: {response}")
        else:
            logging.info("Ready for pipewire stream")
            for node_id, _ in results['streams']:
                logging.info(f"stream {node_id}")

                fd = self.proxy.OpenPipeWireRemote(
                    self.session_handle,
                    dbus.Dictionary(signature='sv'),
                ).take()

                self.emit('ready', fd, node_id)

    def open(self, draw_pointer):
        self.draw_pointer = draw_pointer
        _, session_token = self._new_session_path()
        self._screencast_call(
            self.proxy.CreateSession,
            self._on_create_session_response,
            options={
                'session_handle_token': session_token
            }
        )

    def close(self):
        pass
