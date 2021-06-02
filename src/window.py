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

from gi.repository import Gio, Gst, Gtk, Adw

from kooha.backend.recorder import Recorder  # noqa: F401

Gst.init(None)


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/window.ui')
class KoohaWindow(Adw.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    start_record_button = Gtk.Template.Child()  # will be unused when DE check is removed
    title_stack = Gtk.Template.Child()
    main_stack = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()

    recorder = Gtk.Template.Child()

    def __init__(self, settings, **kwargs):
        super().__init__(**kwargs)

        self.settings = settings

        self.start_record_button.grab_focus()

    @Gtk.Template.Callback()
    def get_pause_resume_icon(self, window, recorder_state):
        return 'media-playback-pause-symbolic' if recorder_state is Gst.State.PAUSED else 'media-playback-start-symbolic'

    @Gtk.Template.Callback()
    def on_start_record_button_clicked(self, widget):
        self.recorder.start()
        self.main_stack.set_visible_child_name('recording')
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
        notification.set_body(f"{notification_body} {self.settings.get_saving_location()}")
        notification.set_default_action('app.show-saving-location')
        self.get_application().send_notification(None, notification)

    @Gtk.Template.Callback()
    def on_stop_record_button_clicked(self, button):
        self.recorder.stop()
        self.main_stack.set_visible_child_name('main-screen')
        self.send_recordingfinished_notification()

    @Gtk.Template.Callback()
    def on_pause_record_button_clicked(self, button):
        self.recorder.pause()

    # @Gtk.Template.Callback()
    def on_resume_record_button_clicked(self, button):
        self.recorder.resume()

    @Gtk.Template.Callback()
    def on_cancel_delay_button_clicked(self, button):
        self.main_stack.set_visible_child_name('main-screen')
        self.delay_timer.cancel()
