# main.py
#
# Copyright 2021 SeaDve
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

import logging
import sys

import gi
gi.require_version('Gst', '1.0')
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Gio, Gdk, GLib, Adw, Gst

from kooha.backend.settings import Settings
from kooha.ui.window import KoohaWindow
from kooha.ui.error_dialog import ErrorDialog

logging.basicConfig(level=logging.DEBUG)
Gst.init(None)

# TODO Implement proper loggers


class Application(Gtk.Application):
    def __init__(self, version):
        super().__init__(application_id='io.github.seadve.Kooha',
                         flags=Gio.ApplicationFlags.FLAGS_NONE)

        self.version = version

        GLib.set_application_name("Kooha")
        GLib.set_prgname('io.github.seadve.Kooha')

    def do_startup(self):
        Gtk.Application.do_startup(self)

        css_provider = Gtk.CssProvider()
        css_provider.load_from_resource('/io/github/seadve/Kooha/ui/style.css')
        display = Gdk.Display.get_default()
        Gtk.StyleContext.add_provider_for_display(
            display, css_provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )

        self.settings = Settings()
        self._setup_actions()

        Adw.init()

    def do_activate(self):
        self.window = self.props.active_window
        if not self.window:
            self.window = KoohaWindow(self.settings, application=self)
        self.window.present()

    def _setup_actions(self):
        simple_actions = [
            ('select-location', self._on_select_location),
            ('show-shortcuts', self._on_show_shortcuts),
            ('show-about', self._on_show_about),
            ('show-saving-location', self._on_show_saving_location),
            ('quit', self._on_quit),
        ]

        for action, callback in simple_actions:
            simple_action = Gio.SimpleAction.new(action, None)
            simple_action.connect("activate", callback)
            self.add_action(simple_action)

        self.set_accels_for_action('app.show-shortcuts', ('<Ctrl>question',))
        self.set_accels_for_action('app.quit', ('<Ctrl>q',))
        self.set_accels_for_action('win.record-speaker', ('<Ctrl>a',))
        self.set_accels_for_action('win.record-mic', ('<Ctrl>m',))
        self.set_accels_for_action('win.show-pointer', ('<Ctrl>p',))

    def _on_select_location(self, action, param):
        dialog = Gtk.FileChooserDialog(transient_for=self.window, modal=True,
                                       action=Gtk.FileChooserAction.SELECT_FOLDER,
                                       title=_("Select a Folder"))
        dialog.add_button(_("Cancel"), Gtk.ResponseType.CANCEL,)
        dialog.add_button(_("Select"), Gtk.ResponseType.ACCEPT,)
        dialog.present()
        dialog.connect('response', self._on_select_folder_response)

    def _on_select_folder_response(self, dialog, response):
        if response == Gtk.ResponseType.ACCEPT:
            directory = dialog.get_file().get_path()
            homefolder = GLib.get_home_dir()
            is_in_homefolder = directory.startswith(homefolder)
            if is_in_homefolder and not directory == homefolder:
                self.settings.set_saving_location(directory)
            else:
                error = ErrorDialog(
                    parent=self.window,
                    title=_("Save location not set"),
                    text=_("Please choose an accessible location and retry."),
                )
                error.present()
        dialog.close()

    def _on_show_shortcuts(self, action, param):
        builder = Gtk.Builder()
        builder.add_from_resource('/io/github/seadve/Kooha/ui/shortcuts.ui')
        window = builder.get_object('shortcuts')
        window.set_transient_for(self.window)
        window.present()

    def _on_show_about(self, action, param):
        about = Gtk.AboutDialog()
        about.set_transient_for(self.window)
        about.set_modal(True)
        about.set_version(self.version)
        about.set_program_name("Kooha")
        about.set_logo_icon_name('io.github.seadve.Kooha')
        about.set_authors(
            [
                "Dave Patrick",
                "",
                "mathiascode",
                "FlexW",
            ]
        )
        about.set_comments(_("Elegantly record your screen"))
        about.set_wrap_license(True)
        about.set_license_type(Gtk.License.GPL_3_0)
        about.set_copyright(_("Copyright 2021 Dave Patrick"))
        # Translators: Replace "translator-credits" with your names, one name per line
        about.set_translator_credits(_("translator-credits"))
        about.set_website_label(_("GitHub"))
        about.set_website("https://github.com/SeaDve/Kooha")
        about.present()

    def _on_show_saving_location(self, action, param):
        saving_location = self.settings.get_saving_location()
        Gio.AppInfo.launch_default_for_uri(f'file://{saving_location}')

    def _on_quit(self, action, param):
        if self.window.recorder.state == Gst.State.NULL:
            self.quit()

    def new_notification(self, title, body, action):
        notification = Gio.Notification.new(title)
        notification.set_body(body)
        notification.set_default_action(action)
        self.send_notification(None, notification)


def main(version):
    app = Application(version)
    return app.run(sys.argv)
