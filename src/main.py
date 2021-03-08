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
import os
import gi

gi.require_version('Gtk', '3.0')
gi.require_version('Gst', '1.0')
gi.require_version('Handy', '1')

from gi.repository import Gtk, Gio, Gdk, GLib, Gst, Handy

from .window import KoohaWindow


class Application(Gtk.Application):
    def __init__(self, version):
        super().__init__(application_id='io.github.seadve.Kooha',
                         flags=Gio.ApplicationFlags.FLAGS_NONE)

        self.version = version

        GLib.set_application_name("Kooha")
        GLib.set_prgname('io.github.seadve.Kooha')

        Handy.init()
        Gst.init()

        css_provider = Gtk.CssProvider()
        css_provider.load_from_resource('/io/github/seadve/Kooha/style.css')
        screen = Gdk.Screen.get_default()
        style_context = Gtk.StyleContext()
        style_context.add_provider_for_screen(screen, css_provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

        # settings init
        self.settings = Gio.Settings.new('io.github.seadve.Kooha')

        self.setup_actions()
        self.set_accels_for_action("app.quit", ["<Ctrl>q"])

    def do_activate(self):
        self.win = self.props.active_window
        if not self.win:
            self.win = KoohaWindow(application=self)
        self.win.present()

    def setup_actions(self):
        action = Gio.SimpleAction.new_stateful("record-delay", GLib.VariantType.new("s"), GLib.Variant('s', "00"))
        action.set_state(self.settings.get_value("record-delay"))
        action.connect("change-state", self.set_value_record_delay)
        self.add_action(action)

        action = Gio.SimpleAction.new_stateful("video-format", GLib.VariantType.new("s"), GLib.Variant('s', "webm"))
        action.set_state(self.settings.get_value("video-format"))
        action.connect("change-state", self.set_value_video_format)
        self.add_action(action)

        action = Gio.SimpleAction.new("select-location", None)
        action.connect("activate", self.select_location_dialog)
        self.add_action(action)

        action = Gio.SimpleAction.new("show-shortcuts", None)
        action.connect("activate", self.show_shortcuts_window)
        self.add_action(action)

        action = Gio.SimpleAction.new("show-about", None)
        action.connect("activate", self.show_about_dialog)
        self.add_action(action)

        action = Gio.SimpleAction.new("quit", None)
        action.connect("activate", self.on_quit)
        self.add_action(action)

    def set_value_record_delay(self, action, value):
        self.settings.set_value("record-delay", value)
        action.set_state(value)

    def set_value_video_format(self, action, value):
        self.settings.set_value("video-format", value)
        action.set_state(value)

    def select_location_dialog(self, action, widget):
        dialog = Gtk.FileChooserNative(title="Select a Folder", action=Gtk.FileChooserAction.SELECT_FOLDER)
        dialog.set_transient_for(self.win)
        response = dialog.run()
        if response == Gtk.ResponseType.ACCEPT:
            directory = dialog.get_filenames()
        else:
            directory = None
        dialog.destroy()
        try:
            if not os.access(directory[0], os.W_OK):
                error = Gtk.MessageDialog(transient_for=self.win, type=Gtk.MessageType.WARNING, buttons=Gtk.ButtonsType.OK, text=_("Inaccessible location"))
                error.format_secondary_text(_("Please choose another location and retry."))
                error.run()
                error.destroy()
            else:
                self.settings.set_string("saving-location", directory[0])
        except:
            return

    def show_shortcuts_window(self, action, widget):
        window = Gtk.Builder.new_from_resource('/io/github/seadve/Kooha/shortcuts.ui').get_object('shortcuts')
        window.set_transient_for(self.win)
        window.present()

    def show_about_dialog(self, action, widget):
        about = Gtk.AboutDialog()
        about.set_transient_for(self.win)
        about.set_version(self.version)
        about.set_program_name("Kooha")
        about.set_logo_icon_name("io.github.seadve.Kooha")
        about.set_authors(["Dave Patrick"])
        about.set_comments("Screen recorder for GNOME Wayland")
        about.set_wrap_license(True)
        about.set_license_type(Gtk.License.GPL_3_0)
        about.set_copyright("© 2021 Dave Patrick")
        about.set_website_label("Github Homepage")
        about.set_website("https://github.com/SeaDve/Kooha")

        about.run()
        about.destroy()

    def on_quit(self, action, *args):
        win = self.get_windows()[0]
        if win.header_revealer.get_reveal_child():
            win.destroy()

    def playsound(self, sound):
        playbin = Gst.ElementFactory.make('playbin', 'playbin')
        playbin.props.uri = 'resource://' + sound
        set_result = playbin.set_state(Gst.State.PLAYING)
        bus = playbin.get_bus()
        bus.poll(Gst.MessageType.EOS, Gst.CLOCK_TIME_NONE)
        playbin.set_state(Gst.State.NULL)


def main(version):
    app = Application(version)
    return app.run(sys.argv)
