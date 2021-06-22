import logging

from gi.repository import Gio, GLib

logger = logging.getLogger(__name__)

shell_proxy = Gio.DBusProxy.new_for_bus_sync(
    Gio.BusType.SESSION,
    Gio.DBusProxyFlags.DO_NOT_AUTO_START_AT_CONSTRUCTION |
    Gio.DBusProxyFlags.DO_NOT_CONNECT_SIGNALS,
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

        try:
            success, result = shell_proxy.Eval(
                '(s)',
                f'global.display.focus_window.{reverse_keyword}{method}()'
            )
        except GLib.Error as error:
            logger.error(error)
            return

        if success:
            logger.info(f"Sucessfully set {method} to {is_enabled}")
        else:
            logger.error(result)

    @staticmethod
    def raise_active_window():
        Utils.shell_window_eval('make_above', True)
        Utils.shell_window_eval('stick', True)

    @staticmethod
    def unraise_active_window():
        Utils.shell_window_eval('make_above', False)
        Utils.shell_window_eval('stick', False)
