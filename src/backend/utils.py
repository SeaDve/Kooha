import logging

from gi.repository import Gio, GLib

logger = logging.getLogger(__name__)

shell_proxy = Gio.DBusProxy.new_for_bus_sync(
    Gio.BusType.SESSION,
    Gio.DBusProxyFlags.NONE,
    None,
    'org.gnome.Shell',
    '/org/gnome/Shell',
    'org.gnome.Shell',
    None
)


class Utils:

    @staticmethod
    def shell_window_eval(function, is_enabled):
        reverse_kw = '' if is_enabled else 'un'

        try:
            shell_proxy.Eval('(s)', f'global.display.focus_window.{reverse_kw}{function}()')
        except GLib.Error as error:
            logger.error(error)
            logger.error(f"Failed to set {function} to {is_enabled}")
        else:
            logger.info(f"Sucessfully set {function} to {is_enabled}")

    @staticmethod
    def raise_active_window():
        Utils.shell_window_eval('make_above', True)
        Utils.shell_window_eval('stick', True)

    @staticmethod
    def unraise_active_window():
        Utils.shell_window_eval('make_above', False)
        Utils.shell_window_eval('stick', False)
