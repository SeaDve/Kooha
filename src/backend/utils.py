import logging

from gi.repository import Gio, GLib

logger = logging.getLogger(__name__)

shell_proxy = Gio.DBusProxy.new_for_bus_sync(
    Gio.BusType.SESSION,
    Gio.DBusProxyFlags.DO_NOT_AUTO_START_AT_CONSTRUCTION
    | Gio.DBusProxyFlags.DO_NOT_CONNECT_SIGNALS,
    None,
    'org.gnome.Shell',
    '/org/gnome/Shell',
    'org.gnome.Shell',
    None
)


class Utils:

    @staticmethod
    def shell_window_eval(method, is_enabled):
        reverse_keyword = '' if is_enabled else 'un'

        success, result = shell_proxy.Eval(
            '(s)',
            f'global.display.focus_window.{reverse_keyword}{method}()'
        )

        if not success:
            raise GLib.Error(result)

    @staticmethod
    def set_raise_active_window_request(is_enabled):
        try:
            Utils.shell_window_eval('make_above', is_enabled)
            Utils.shell_window_eval('stick', is_enabled)
        except GLib.Error as error:
            logging.warning(error)
        else:
            logger.info(f"Sucessfully raised the active window")
