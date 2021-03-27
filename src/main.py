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

import os
import sys
import gi

gi.require_version('Gst', '1.0')
gi.require_version('Gtk', '3.0')
gi.require_version('Handy', '1')

from gettext import gettext as _
from gi.repository import Gtk, Gio, Gdk, GLib, Gst, Handy
Gst.init(sys.argv)

from .window import KoohaWindow


class Application(Gtk.Application):
    def __init__(self, version):
        super().__init__(application_id='io.github.seadve.Kooha',
                         flags=Gio.ApplicationFlags.FLAGS_NONE)

        self.version = version

        GLib.set_application_name("Kooha")
        GLib.set_prgname('io.github.seadve.Kooha')

    def do_startup(self):
        Gtk.Application.do_startup(self)

        Handy.init()

        css_provider = Gtk.CssProvider()
        css_provider.load_from_resource('/io/github/seadve/Kooha/style.css')
        screen = Gdk.Screen.get_default()
        Gtk.StyleContext.add_provider_for_screen(screen, css_provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

        self.settings = Gio.Settings.new('io.github.seadve.Kooha')
        self.setup_actions()

    def do_activate(self):
        self.window = self.props.active_window
        if not self.window:
            self.window = KoohaWindow(self.settings, application=self)
        self.window.present()

    def setup_actions(self):
        settings_actions = [
            "record-audio",
            "record-microphone",
            "show-pointer",
            "record-delay",
            "video-format",
        ]

        simple_actions = [
            ("select-location", self.select_location_dialog, None),
            ("show-shortcuts", self.show_shortcuts_window, ("<Ctrl>question",)),
            ("show-about", self.show_about_dialog, None),
            ("change-capture-mode", self.on_change_capture_mode, ("<Ctrl>f",)),
            ("show-saving-location", self.show_saving_location, None),
            ("quit", self.on_quit, ("<Ctrl>q",)),
        ]

        for action in settings_actions:
            settings_action = self.settings.create_action(action)
            self.add_action(settings_action)

        for action, callback, accel in simple_actions:
            simple_action = Gio.SimpleAction.new(action, None)
            simple_action.connect("activate", callback)
            self.add_action(simple_action)
            if accel: self.set_accels_for_action(f"app.{action}", accel)

    def select_location_dialog(self, action, widget):
        dialog = Gtk.FileChooserDialog(title=_("Select a Folder"), action=Gtk.FileChooserAction.SELECT_FOLDER)
        dialog.add_buttons(_("Cancel"), Gtk.ResponseType.CANCEL, _("Select"), Gtk.ResponseType.ACCEPT)
        dialog.set_transient_for(self.window)
        response = dialog.run()
        if response == Gtk.ResponseType.ACCEPT:
            directory = dialog.get_filenames()
        dialog.destroy()
        try:
            if not os.access(directory[0], os.W_OK) or not directory[0].startswith(os.getenv("HOME")) or directory[0] == os.getenv("HOME"):
                error = Gtk.MessageDialog(transient_for=self.window, type=Gtk.MessageType.WARNING, buttons=Gtk.ButtonsType.OK, text=_("Save location not set"))
                error.format_secondary_text(_("Please choose an accessible location and retry."))
                error.run()
                error.destroy()
            else:
                self.settings.set_string("saving-location", directory[0])
        except:
            return

    def show_shortcuts_window(self, action, widget):
        window = Gtk.Builder.new_from_resource('/io/github/seadve/Kooha/shortcuts.ui').get_object('shortcuts')
        window.set_transient_for(self.window)
        window.present()

    def show_about_dialog(self, action, widget):
        about = Gtk.AboutDialog()
        about.set_transient_for(self.window)
        about.set_modal(True)
        about.set_version(self.version)
        about.set_program_name("Kooha")
        about.set_logo_icon_name("io.github.seadve.Kooha")
        about.set_authors(["Dave Patrick"])
        about.set_comments(_("Simple screen recorder"))
        about.set_wrap_license(True)
        about.set_license_type(Gtk.License.GPL_3_0)
        about.set_copyright(_("Copyright 2021 Dave Patrick"))
        # Translators: Replace "translator-credits" with your names, one name per line
        about.set_translator_credits(_("translator-credits"))
        about.set_website_label(_("GitHub"))
        about.set_website("https://github.com/SeaDve/Kooha")
        about.show()

    def show_saving_location(self, action, widget):
        saving_location = self.window.get_saving_location()[1]
        Gio.AppInfo.launch_default_for_uri(f"file://{saving_location}")

    def on_change_capture_mode(self, action, widget):
        if self.window.main_stack.get_visible_child() is self.window.main_screen_box:
            if self.window.title_stack.get_visible_child() is self.window.selection_mode_label:
                self.window.title_stack.set_visible_child(self.window.fullscreen_mode_label)
            else:
                self.window.title_stack.set_visible_child(self.window.selection_mode_label)

    def on_quit(self, action, widget):
        if self.window.main_stack.get_visible_child() is self.window.main_screen_box:
            self.quit()


def main(version):
    app = Application(version)
    return app.run(sys.argv)
