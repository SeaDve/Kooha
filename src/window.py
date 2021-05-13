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

from gi.repository import Gio, GLib, Gst, Gtk, Handy

from kooha.recorders import AudioRecorder, VideoRecorder
from kooha.timers import DelayTimer, Timer

Gst.init(None)


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/window.ui')
class KoohaWindow(Handy.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    start_record_button = Gtk.Template.Child()  # will be unused when DE check is removed
    title_stack = Gtk.Template.Child()
    fullscreen_mode_label = Gtk.Template.Child()
    selection_mode_label = Gtk.Template.Child()

    main_stack = Gtk.Template.Child()
    main_screen_box = Gtk.Template.Child()
    recording_label_box = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label_box = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()
    processing_label_box = Gtk.Template.Child()

    def __init__(self, settings, **kwargs):
        super().__init__(**kwargs)
        self.settings = settings

        self.timer = Timer(self.time_recording_label)
        self.delay_timer = DelayTimer(self.delay_label, self.start_recording)
        self.video_recorder = VideoRecorder()

        desktop_environment = GLib.getenv('XDG_CURRENT_DESKTOP')
        if not desktop_environment or "GNOME" not in desktop_environment:
            self.start_record_button.set_sensitive(False)
            self.start_record_button.set_label(f"{desktop_environment or 'WM'} is not yet supported")

    @Gtk.Template.Callback()
    def on_start_record_button_clicked(self, widget):
        self.directory, video_directory = self.get_saving_location()

        if os.path.exists(video_directory):
            if self.title_stack.get_visible_child() is self.selection_mode_label:
                self.video_recorder.set_selection_mode()
            else:
                self.video_recorder.set_fullscreen_mode()

            delay = int(self.settings.get_string("record-delay"))
            self.delay_timer.start(delay)

            if delay > 0:
                self.main_stack.set_visible_child(self.delay_label_box)
        else:
            error = Gtk.MessageDialog(transient_for=self,
                                      type=Gtk.MessageType.WARNING,
                                      buttons=Gtk.ButtonsType.OK,
                                      text=_("Recording cannot start"))
            error.format_secondary_text(_("The saving location you have selected may have been deleted."))
            error.run()
            error.destroy()

    def start_recording(self):
        Thread(target=self.playchime).start()

        record_audio = self.settings.get_boolean("record-audio")
        record_microphone = self.settings.get_boolean("record-microphone")
        self.audio_recorder = AudioRecorder(self.directory, record_audio, record_microphone)

        framerate = self.settings.get_int("video-frames")
        show_pointer = self.settings.get_boolean("show-pointer")

        if ((record_audio and self.audio_recorder.default_audio_output)
                or (record_microphone and self.audio_recorder.default_audio_input)):
            directory = self.audio_recorder.get_tmp_dir("video")
            self.video_recorder.start(directory, framerate, show_pointer)
            self.audio_recorder.start()
            self.audio_mode = True
        else:
            self.video_recorder.start(self.directory, framerate, show_pointer)
            self.audio_mode = False

        self.timer.start()

        self.main_stack.set_visible_child(self.recording_label_box)

    def get_saving_location(self):
        video_directory = self.settings.get_string('saving-location')
        filename = f"Kooha-{strftime('%Y-%m-%d-%H:%M:%S', localtime())}"
        video_format = self.settings.get_string('video-format')
        if self.settings.get_string("saving-location") == "default":
            video_directory = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_VIDEOS)
            if not os.path.exists(video_directory):
                video_directory = GLib.get_home_dir()
        return (f"{video_directory}/{filename}.{video_format}", video_directory)

    def playchime(self):
        playbin = Gst.ElementFactory.make('playbin', 'playbin')
        playbin.props.uri = 'resource://io/github/seadve/Kooha/sounds/chime.ogg'
        playbin.set_state(Gst.State.PLAYING)
        bus = playbin.get_bus()
        bus.poll(Gst.MessageType.EOS, Gst.CLOCK_TIME_NONE)
        playbin.set_state(Gst.State.NULL)

    def send_recordingfinished_notification(self):
        notification = Gio.Notification.new(_("Screencast Recorded!"))
        notification_body = _("The recording has been saved in")
        notification.set_body(f"{notification_body} {self.get_saving_location()[1]}")
        notification.set_default_action("app.show-saving-location")
        self.get_application().send_notification(None, notification)

    @Gtk.Template.Callback()
    def on_stop_record_button_clicked(self, widget):
        self.video_recorder.stop()
        self.timer.stop()

        if self.audio_mode:
            self.main_stack.set_visible_child(self.processing_label_box)
            self.audio_recorder.stop(self)
            self.audio_mode = False
        else:
            self.main_stack.set_visible_child(self.main_screen_box)
            self.send_recordingfinished_notification()

    @Gtk.Template.Callback()
    def on_cancel_delay_button_clicked(self, widget):
        self.main_stack.set_visible_child(self.main_screen_box)

        self.delay_timer.cancel()
