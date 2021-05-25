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

import sys

import gi
gi.require_version('Gst', '1.0')
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Gio, Gdk, GLib, Adw

from kooha.window import KoohaWindow


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

        self.settings = Gio.Settings.new('io.github.seadve.Kooha')
        self.setup_actions()

        Adw.init()

    def do_activate(self):
        self.window = self.props.active_window
        if not self.window:
            self.window = KoohaWindow(self.settings, application=self)
        self.window.present()

    def setup_actions(self):
        settings_actions = [
            ("record-audio", ("<Ctrl>a",)),
            ("record-microphone", ("<Ctrl>m",)),
            ("show-pointer", ("<Ctrl>p",)),
            ("record-delay", None),
            ("video-format", None),
        ]

        simple_actions = [
            ("select-location", self.select_location_dialog, None),
            ("show-shortcuts", self.show_shortcuts_window, ("<Ctrl>question",)),
            ("show-about", self.show_about_dialog, None),
            ("show-saving-location", self.show_saving_location, None),
            ("change-capture-mode", self.on_change_capture_mode, ("<Ctrl>f",)),
            ("quit", self.on_quit, ("<Ctrl>q",)),
        ]

        for action, accel in settings_actions:
            settings_action = self.settings.create_action(action)
            self.add_action(settings_action)
            if accel:
                self.set_accels_for_action(f"app.{action}", accel)

        for action, callback, accel in simple_actions:
            simple_action = Gio.SimpleAction.new(action, None)
            simple_action.connect("activate", callback)
            self.add_action(simple_action)
            if accel:
                self.set_accels_for_action(f"app.{action}", accel)

    def select_location_dialog(self, action, param):
        dialog = Gtk.FileChooserDialog(transient_for=self.window, modal=True,
                                       action=Gtk.FileChooserAction.SELECT_FOLDER,
                                       title=_("Select a Folder"))
        dialog.add_button(_("Cancel"), Gtk.ResponseType.CANCEL,)
        dialog.add_button(_("Select"), Gtk.ResponseType.ACCEPT,)
        dialog.present()
        dialog.connect('response', self._on_select_response)

    def _on_select_response(self, dialog, response):
        if response == Gtk.ResponseType.ACCEPT:
            directory = dialog.get_file().get_path()
            homefolder = GLib.get_home_dir()
            is_in_homefolder = directory.startswith(homefolder)
            if is_in_homefolder and not directory == homefolder:
                self.settings.set_string("saving-location", directory)
            else:
                error = Gtk.MessageDialog(transient_for=self.window, modal=True,
                                          buttons=Gtk.ButtonsType.OK, title=_("Save location not set"),
                                          text=_("Please choose an accessible location and retry."))
                error.connect("response", lambda *_: error.close())
                error.present()
        dialog.close()

    def show_shortcuts_window(self, action, param):
        builder = Gtk.Builder()
        builder.add_from_resource('/io/github/seadve/Kooha/ui/shortcuts.ui')
        window = builder.get_object('shortcuts')
        window.set_transient_for(self.window)
        window.present()

    def show_about_dialog(self, action, param):
        about = Gtk.AboutDialog()
        about.set_transient_for(self.window)
        about.set_modal(True)
        about.set_version(self.version)
        about.set_program_name("Kooha")
        about.set_logo_icon_name("io.github.seadve.Kooha")
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

    def show_saving_location(self, action, param):
        saving_location = self.window.get_saving_location()[1]
        Gio.AppInfo.launch_default_for_uri(f"file://{saving_location}")

    def on_change_capture_mode(self, action, param):
        if self.window.main_stack.get_visible_child_name() == "main-screen":
            if self.window.title_stack.get_visible_child_name() == "selection-mode":
                self.window.title_stack.set_visible_child_name("fullscreen-mode")
            else:
                self.window.title_stack.set_visible_child_name("selection-mode")

    def on_quit(self, action, param):
        if self.window.main_stack.get_visible_child_name() == "main-screen":
            self.quit()


def main(version):
    app = Application(version)
    return app.run(sys.argv)
