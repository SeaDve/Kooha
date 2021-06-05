import logging

from gi.repository import GObject, GLib, Gio

# TODO Close the session after use


class Portal(GObject.GObject):
    __gsignals__ = {'ready': (GObject.SIGNAL_RUN_FIRST, None, (int, int))}

    def __init__(self):
        super().__init__()

        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.sender_name = self.bus.get_unique_name()[1:].replace('.', '_')
        self.proxy = Gio.DBusProxy.new_sync(
            self.bus,
            Gio.DBusProxyFlags.GET_INVALIDATED_PROPERTIES,
            None,
            'org.freedesktop.portal.Desktop',
            '/org/freedesktop/portal/desktop',
            'org.freedesktop.portal.ScreenCast',
            None,
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

        self.bus.signal_subscribe(
            'org.freedesktop.portal.Desktop',
            'org.freedesktop.portal.Request',
            'Response',
            request_path,
            None,
            Gio.DBusSignalFlags.NONE,
            callback
        )

        self.proxy.call_sync(
            method,
            GLib.Variant.new_tuple(
                *args,
                GLib.Variant('a{sv}', {
                    'handle_token': GLib.Variant('s', request_token),
                    **options
                })
            ),
            Gio.DBusCallFlags.NONE,
            -1,
            None
        )

    def _on_create_session_response(self, bus, sender, path, i_face_name, node, results):
        if not results[1]:
            return

        self.session_handle = results[1]['session_handle']
        logging.info("Session created")
        self._screencast_call(
            'SelectSources',
            self._on_select_sources_response,
            GLib.Variant('o', self.session_handle),
            options={
                'types': GLib.Variant('u', 1 | 2),  # Which source
                'cursor_mode': GLib.Variant('u', 2 if self.draw_pointer else 1)
            }
        )

    def _on_select_sources_response(self, bus, sender, path, i_face_name, node, results):
        logging.info("Sources selected")
        self._screencast_call(
            'Start',
            self._on_start_response,
            GLib.Variant('o', self.session_handle),
            GLib.Variant('s', ''),
        )

    def _on_start_response(self, bus, sender, path, i_face_name, node, results):
        if not results[1]:
            return


        def testtest(self, *args):
            print(args)
            test = args[0].call_finish(x)
            print(test)

        logging.info("Ready for pipewire stream")
        for node_id, _ in results[1]['streams']:
            logging.info(f"stream {node_id}")

            res = self.proxy.call(
                'OpenPipeWireRemote',
                GLib.Variant.new_tuple(
                    GLib.Variant('o', self.session_handle),
                    GLib.Variant('a{sv}', {}),
                ),
                Gio.DBusCallFlags.NONE,
                -1,
                None,
                testtest,
            )

            # print(res.unpack())

            # self.emit('ready', fd, node_id)

    def open(self, draw_pointer):
        self.draw_pointer = draw_pointer
        session_path, self.session_token = self._new_session_path()
        self._screencast_call(
            'CreateSession',
            self._on_create_session_response,
            options={
                'session_handle_token': GLib.Variant('s', self.session_token),
            }
        )

    def close(self):
        pass
