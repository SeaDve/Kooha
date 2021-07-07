# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

import os
import sys

import gi
gi.require_version('Gst', '1.0')
gi.require_version('Gtk', '4.0')
gi.require_version('Adw', '1')
from gi.repository import Gtk, Gio, Gdk, GLib, Adw, Gst

from kooha.backend.settings import Settings
from kooha.widgets.error_dialog import ErrorDialog
from kooha.widgets.window import Window

Gst.init(None)


class Application(Gtk.Application):

    def __init__(self, version):
        super().__init__(application_id='io.github.seadve.Kooha',
                         flags=Gio.ApplicationFlags.FLAGS_NONE)

        self.version = version

        GLib.set_application_name("Kooha")

    def do_startup(self):
        Gtk.Application.do_startup(self)

        Adw.init()

        css_provider = Gtk.CssProvider()
        css_provider.load_from_resource('/io/github/seadve/Kooha/ui/style.css')
        display = Gdk.Display.get_default()
        Gtk.StyleContext.add_provider_for_display(
            display, css_provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )

        self.settings = Settings()
        self._setup_actions()

    def do_activate(self):
        window = self.props.active_window
        if not window:
            window = Window(self.settings, application=self)
        window.present()

    def _setup_actions(self):
        simple_actions = [
            ('select-saving-location', self._on_select_saving_location, None),
            ('show-about', self._on_show_about, None),
            ('quit', self._on_quit, None),
            ('show-saving-location', self._on_show_saving_location, GLib.VariantType('s')),
            ('show-saved-recording', self._on_show_saved_recording, GLib.VariantType('s')),
        ]

        for action, callback, param_type in simple_actions:
            simple_action = Gio.SimpleAction.new(action, param_type)
            simple_action.connect('activate', callback)
            self.add_action(simple_action)

        self.set_accels_for_action('app.quit', ('<Ctrl>q',))
        self.set_accels_for_action('win.record-speaker', ('<Ctrl>a',))
        self.set_accels_for_action('win.record-mic', ('<Ctrl>m',))
        self.set_accels_for_action('win.show-pointer', ('<Ctrl>p',))
        self.set_accels_for_action('win.show-help-overlay', ('<Ctrl>question',))

    def _on_select_saving_location(self, action, param):
        chooser = Gtk.FileChooserDialog(transient_for=self.props.active_window,
                                        modal=True,
                                        action=Gtk.FileChooserAction.SELECT_FOLDER,
                                        title=_("Select a Folder"))
        chooser.add_button(_("Cancel"), Gtk.ResponseType.CANCEL)
        chooser.add_button(_("Select"), Gtk.ResponseType.ACCEPT)
        chooser.set_default_response(Gtk.ResponseType.ACCEPT)
        chooser.set_current_folder(Gio.File.new_for_path(self.settings.get_saving_location()))
        chooser.connect('response', self._on_select_folder_response)
        chooser.present()

    def _on_select_folder_response(self, chooser, response):
        if response != Gtk.ResponseType.ACCEPT:
            chooser.close()
            return

        directory = chooser.get_file().get_path()
        homefolder = GLib.get_home_dir()
        is_in_homefolder = directory.startswith(homefolder)

        if not is_in_homefolder or directory == homefolder:
            error = ErrorDialog(parent=self.props.active_window,
                                title=_(f"Inaccessible location “{directory}”"),
                                text=_("Please choose an accessible location and retry."))
            error.present()
            return

        self.settings.set_saving_location(directory)
        chooser.close()

    def _on_show_about(self, action, param):
        about = Gtk.AboutDialog()
        about.set_transient_for(self.props.active_window)
        about.set_modal(True)
        about.set_version(self.version)
        about.set_program_name(_("Kooha"))
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
        saving_location = param.unpack()
        Gio.AppInfo.launch_default_for_uri(f'file://{saving_location}')

    def _on_show_saved_recording(self, action, param):
        saved_recording = param.unpack()
        Gio.AppInfo.launch_default_for_uri(f'file://{saved_recording}')

    def _on_quit(self, action, param):
        window = self.props.active_window
        if window.recorder.state == Gst.State.NULL:
            self.quit()

    def send_record_success_notification(self, recording_file_path):
        saving_location = os.path.dirname(recording_file_path)

        notification = Gio.Notification.new(_("Screencast Recorded!"))
        notification.set_body(_(f"The recording has been saved in {saving_location}"))
        notification.set_default_action_and_target(
            'app.show-saving-location',
            GLib.Variant('s', saving_location)
        )
        notification.add_button_with_target(
            _("Open File"),
            'app.show-saved-recording',
            GLib.Variant('s', recording_file_path)
        )
        self.send_notification('record-success', notification)


def main(version):
    app = Application(version)
    return app.run(sys.argv)
