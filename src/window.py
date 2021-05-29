# window.py
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
from threading import Thread
from time import localtime, strftime

from gi.repository import Gio, GLib, Gst, Gtk, Adw

# from kooha.recorders import AudioRecorder, VideoRecorder
# from kooha.timers import DelayTimer, Timer
from kooha.backend.recorder import Recorder

Gst.init(None)


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/window.ui')
class KoohaWindow(Adw.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    start_record_button = Gtk.Template.Child()  # will be unused when DE check is removed
    title_stack = Gtk.Template.Child()
    main_stack = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()

    def __init__(self, settings, **kwargs):
        super().__init__(**kwargs)

        self.recorder = Recorder()

        self.start_record_button.grab_focus()

    @Gtk.Template.Callback()
    def on_start_record_button_clicked(self, widget):
        self.recorder.start()
        self.main_stack.set_visible_child_name("recording")
        # self.directory, video_directory, frmt = self.get_saving_location()

        # if os.path.exists(video_directory):
        #     if self.title_stack.get_visible_child_name() == "selection-mode":
        #         self.video_recorder.set_selection_mode()
        #     else:
        #         self.video_recorder.set_fullscreen_mode()

        #     delay = int(self.settings.get_string("record-delay"))
        #     self.delay_timer.start(delay)

        #     if delay > 0:
        #         self.main_stack.set_visible_child_name("delay")
        # else:
        #     error = Gtk.MessageDialog(transient_for=self, modal=True,
        #                               buttons=Gtk.ButtonsType.OK, title=_("Recording cannot start"),
        #                               text=_("The saving location you have selected may have been deleted."))
        #     error.present()
        #     error.connect("response", lambda *_: error.close())

    def send_recordingfinished_notification(self):
        notification = Gio.Notification.new(_("Screencast Recorded!"))
        notification_body = _("The recording has been saved in")
        notification.set_body(f"{notification_body} {self.get_saving_location()[1]}")
        notification.set_default_action("app.show-saving-location")
        self.get_application().send_notification(None, notification)

    @Gtk.Template.Callback()
    def on_stop_record_button_clicked(self, button):
        self.recorder.stop()

        self.main_stack.set_visible_child_name("main-screen")
        self.send_recordingfinished_notification()

    @Gtk.Template.Callback()
    def on_pause_record_button_clicked(self, button):
        self.recorder.pause()

    # @Gtk.Template.Callback()
    def on_resume_record_button_clicked(self, button):
        self.recorder.resume()

    @Gtk.Template.Callback()
    def on_cancel_delay_button_clicked(self, button):
        self.main_stack.set_visible_child_name("main-screen")
        self.delay_timer.cancel()
