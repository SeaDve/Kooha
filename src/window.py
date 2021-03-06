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

from time import strftime, localtime

from gi.repository import Gtk, Gio, GLib, Handy

from .lib import VideoRecorder, AudioRecorder, Timer, DelayTimer

# add --disable-everything && fix unknown input format: 'pulse'

# fix ffmpeg sound delay/advance && other audio bugs (echo)

# fix mic bug wherein it will record computer sounds when there is no mic (add way to find mic source)

# add support with other formats


@Gtk.Template(resource_path='/io/github/seadve/Kooha/window.ui')
class KoohaWindow(Handy.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    stop_record_button = Gtk.Template.Child()
    cancel_delay_button = Gtk.Template.Child()
    start_record_button_box = Gtk.Template.Child()
    start_stop_record_button_stack = Gtk.Template.Child()

    fullscreen_mode_toggle = Gtk.Template.Child()
    selection_mode_toggle = Gtk.Template.Child()

    header_revealer = Gtk.Template.Child()
    title_stack = Gtk.Template.Child()
    fullscreen_mode_label = Gtk.Template.Child()
    selection_mode_label = Gtk.Template.Child()

    record_audio_toggle = Gtk.Template.Child()
    record_microphone_toggle = Gtk.Template.Child()
    show_pointer_toggle = Gtk.Template.Child()

    main_stack = Gtk.Template.Child()
    main_screen_box = Gtk.Template.Child()
    recording_label_box = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label_box = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()

    menu_button = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.application = kwargs["application"]

        # popover init
        builder = Gtk.Builder()
        builder.add_from_resource('/io/github/seadve/Kooha/menu.ui')
        menu_model = builder.get_object('menu')
        popover = Gtk.Popover.new_from_model(self.menu_button, menu_model)
        self.menu_button.set_popover(popover)

        # settings init
        self.record_audio_toggle.set_active(self.application.settings.get_boolean("record-audio"))
        self.record_microphone_toggle.set_active(self.application.settings.get_boolean("record-microphone"))
        self.show_pointer_toggle.set_active(self.application.settings.get_boolean("show-pointer"))

        # timer init
        self.delay_timer = DelayTimer(self.delay_label, self.start_recording)

    @Gtk.Template.Callback()
    def on_start_record_button_clicked(self, widget):

        framerate = 30
        pipeline = "queue ! vp8enc min_quantizer=25 max_quantizer=25 cpu-used=3 cq_level=13 deadline=1 threads=3 ! queue ! matroskamux"

        show_pointer = self.application.settings.get_boolean("show-pointer")
        delay = int(self.application.settings.get_string("record-delay"))

        video_format = "." + self.application.settings.get_string("video-format")
        filename = fileNameTime = "/Kooha-" + strftime("%Y-%m-%d-%H:%M:%S", localtime())
        self.directory = self.application.settings.get_string("saving-location") + filename + video_format
        if self.application.settings.get_string("saving-location") == "default":
            video_directory = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_VIDEOS)
            if video_directory == None:
                video_directory = os.getenv("HOME")
            self.directory = video_directory + filename + video_format

        self.video_recorder = VideoRecorder(self.fullscreen_mode_toggle, framerate, show_pointer, pipeline, self.directory)

        if not self.fullscreen_mode_toggle.get_active():
            self.video_recorder.get_coordinates()

        self.delay_timer.start(delay)

        if delay > 0:
            self.main_stack.set_visible_child(self.delay_label_box)
            self.start_stop_record_button_stack.set_visible_child(self.cancel_delay_button)
            self.header_revealer.set_reveal_child(False)

    def start_recording(self):

        record_audio = self.application.settings.get_boolean("record-audio")
        record_microphone = self.application.settings.get_boolean("record-microphone")

        self.audio_recorder = AudioRecorder(record_audio, record_microphone, self.directory)

        if record_audio or record_microphone:
            self.directory = self.audio_recorder.get_tmp_dir() + "/.Kooha_tmpvideo.mkv"

        self.video_recorder.start()
        self.audio_recorder.start()

        self.header_revealer.set_reveal_child(False)
        self.start_stop_record_button_stack.set_visible_child(self.stop_record_button)
        self.main_stack.set_visible_child(self.recording_label_box)

        self.timer = Timer(self.time_recording_label)
        self.timer.start()

        self.application.playsound('io/github/seadve/Kooha/chime.ogg')

    @Gtk.Template.Callback()
    def on_stop_record_button_clicked(self, widget):

        self.header_revealer.set_reveal_child(True)
        self.start_stop_record_button_stack.set_visible_child(self.start_record_button_box)
        self.main_stack.set_visible_child(self.main_screen_box)

        self.video_recorder.stop()

        self.audio_recorder.stop()
        self.timer.stop()

    @Gtk.Template.Callback()
    def on_cancel_delay_button_clicked(self, widget):
        self.delay_timer.cancel()

        self.main_stack.set_visible_child(self.main_screen_box)
        self.start_stop_record_button_stack.set_visible_child(self.start_record_button_box)
        self.header_revealer.set_reveal_child(True)

    @Gtk.Template.Callback()
    def on_fullscreen_mode_clicked(self, widget):
        self.title_stack.set_visible_child(self.fullscreen_mode_label)

    @Gtk.Template.Callback()
    def on_selection_mode_clicked(self, widget):
        self.title_stack.set_visible_child(self.selection_mode_label)

    @Gtk.Template.Callback()
    def on_record_audio_toggled(self, widget):
        if self.record_audio_toggle.get_active():
            self.application.settings.set_boolean("record-audio", True)
        else:
            self.application.settings.set_boolean("record-audio", False)

    @Gtk.Template.Callback()
    def on_record_microphone_toggled(self, widget):
        if self.record_microphone_toggle.get_active():
            self.application.settings.set_boolean("record-microphone", True)
        else:
            self.application.settings.set_boolean("record-microphone", False)

    @Gtk.Template.Callback()
    def on_show_pointer_toggled(self, widget):
        if self.show_pointer_toggle.get_active():
            self.application.settings.set_boolean("show-pointer", True)
        else:
            self.application.settings.set_boolean("show-pointer", False)
