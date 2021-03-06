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

import signal
import os # not yet used

import time
from time import sleep

from gi.repository import Gtk, Gio, GLib, Handy

from subprocess import PIPE, Popen

# add --disable-everything && fix unknown input format: 'pulse'

# fix ffmpeg sound delay/advance && other audio bugs (echo)

# fix mic bug wherein it will record computer sounds when there is no mic (add way to find mic source)

# add support with other formats


@Gtk.Template(resource_path='/io/github/seadve/Kooha/window.ui')
class KoohaWindow(Handy.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    start_record_button = Gtk.Template.Child()  #temp
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

        # setup popover
        builder = Gtk.Builder()
        builder.add_from_resource('/io/github/seadve/Kooha/menu.ui')
        menu_model = builder.get_object('menu')
        popover = Gtk.Popover.new_from_model(self.menu_button, menu_model)
        self.menu_button.set_popover(popover)

        # settings init
        self.record_audio_toggle.set_active(self.application.settings.get_boolean("record-audio"))
        self.record_microphone_toggle.set_active(self.application.settings.get_boolean("record-microphone"))
        self.show_pointer_toggle.set_active(self.application.settings.get_boolean("show-pointer"))

        # dbus init
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.GNOMEScreencast = Gio.DBusProxy.new_sync(
                    self.bus,
                    Gio.DBusProxyFlags.NONE,
                    None,
                    "org.gnome.Shell.Screencast",
                    "/org/gnome/Shell/Screencast",
                    "org.gnome.Shell.Screencast",
                    None)

        self.GNOMESelectArea = Gio.DBusProxy.new_sync(
                    self.bus,
                    Gio.DBusProxyFlags.NONE,
                    None,
                    "org.gnome.Shell.Screenshot",
                    "/org/gnome/Shell/Screenshot",
                    "org.gnome.Shell.Screenshot",
                    None)

    @Gtk.Template.Callback()
    def on_start_record_button_clicked(self, widget):

        delay = int(self.application.settings.get_string("record-delay"))

        self.delay_timer = DelayTimer(self.delay_label, self.start_recording)
        self.delay_timer.start(delay)

        if delay > 0:
            self.main_stack.set_visible_child(self.delay_label_box)
            self.start_stop_record_button_stack.set_visible_child(self.cancel_delay_button)
            self.header_revealer.set_reveal_child(False)

    @Gtk.Template.Callback()
    def on_cancel_delay_button_clicked(self, widget):
        self.delay_timer.cancel()

        self.main_stack.set_visible_child(self.main_screen_box)
        self.start_stop_record_button_stack.set_visible_child(self.start_record_button_box)
        self.header_revealer.set_reveal_child(True)


    def start_recording(self):

        framerate = 30
        pipeline = "queue ! vp8enc min_quantizer=25 max_quantizer=25 cpu-used=3 cq_level=13 deadline=1 threads=3 ! queue ! matroskamux"

        video_format = "." + self.application.settings.get_string("video-format")
        record_audio = self.application.settings.get_boolean("record-audio")
        record_microphone = self.application.settings.get_boolean("record-microphone")
        show_pointer = self.application.settings.get_boolean("show-pointer")

        filename = fileNameTime = "/Kooha-" + time.strftime("%Y-%m-%d-%H:%M:%S", time.localtime())
        directory = self.application.settings.get_string("saving-location") + filename + video_format
        self.saving_location = directory



        if record_audio or record_microphone:
            directory = self.get_tmp_dir() + "/.Kooha_tmpvideo.mkv"

        if self.fullscreen_mode_toggle.get_active():
            self.GNOMEScreencast.call_sync(
                        "Screencast",
                        GLib.Variant.new_tuple(
                            GLib.Variant.new_string(directory),
                            GLib.Variant("a{sv}",
                                {"framerate": GLib.Variant("i", framerate),
                                 "draw-cursor": GLib.Variant("b", show_pointer),
                                 "pipeline": GLib.Variant("s", pipeline)}
                            ),
                        ),
                        Gio.DBusProxyFlags.NONE,
                        -1,
                        None)

        elif self.selection_mode_toggle.get_active():
            coordinates = self.GNOMESelectArea.call_sync("SelectArea", None, Gio.DBusProxyFlags.NONE, -1, None)
            self.GNOMEScreencast.call_sync(
                    "ScreencastArea",
                    GLib.Variant.new_tuple(
                        GLib.Variant("i", coordinates[0]),
                        GLib.Variant("i", coordinates[1]),
                        GLib.Variant("i", coordinates[2]),
                        GLib.Variant("i", coordinates[3]),
                        GLib.Variant.new_string(directory),
                        GLib.Variant("a{sv}",
                            {"framerate": GLib.Variant("i", framerate),
                             "draw-cursor": GLib.Variant("b", show_pointer),
                             "pipeline": GLib.Variant("s", pipeline)}
                        ),
                    ),
                    Gio.DBusProxyFlags.NONE,
                    -1,
                    None)

        self.start_audio_record(record_audio, record_microphone)

        self.header_revealer.set_reveal_child(False)
        self.start_stop_record_button_stack.set_visible_child(self.stop_record_button)
        self.main_stack.set_visible_child(self.recording_label_box)

        self.timer = Timer(self.time_recording_label)
        self.timer.start()

        self.application.playsound('io/github/seadve/Kooha/chime.ogg')


    @Gtk.Template.Callback()
    def on_stop_record_button_clicked(self, widget):
        self.stop_recording()

    def stop_recording(self):

        self.header_revealer.set_reveal_child(True)
        self.start_stop_record_button_stack.set_visible_child(self.start_record_button_box)
        self.main_stack.set_visible_child(self.main_screen_box)

        self.GNOMEScreencast.call_sync(
            "StopScreencast",
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None)

        self.stop_audio_record()

        self.timer.stop()


    def start_audio_record(self, record_audio, record_microphone):
        if record_audio or record_microphone:
            command = ""
            command += "ffmpeg -f "

            if record_audio:
                command += "pulse -i {} ".format(self.get_default_audio_output()) # TODO test this with other devices

            if record_audio and record_microphone:
                command += "-f "

            if record_microphone:
                command += "pulse -i default "

            if record_audio and record_microphone:
                command += "-filter_complex amerge -ac 2 "

            command += self.get_tmp_dir() + "/.Kooha_tmpaudio.mp3 -y"

            self.audio_subprocess = Popen(command, shell=True)

            command = ""


    def stop_audio_record(self):
        record_audio = self.application.settings.get_boolean("record-audio")
        record_microphone = self.application.settings.get_boolean("record-microphone")

        if record_audio or record_microphone:
            self.audio_subprocess.send_signal(signal.SIGINT)
            sleep(1) # TODO replace with more ideal solution
            output = Popen("ffmpeg -i {0}/.Kooha_tmpvideo.mkv -i {0}/.Kooha_tmpaudio.mp3 -c:v copy -c:a aac {1} -y".format(self.get_tmp_dir(), self.saving_location), shell=True) # TODO add proper tmp directories


    def get_default_audio_output(self):# TODO move this to main
        test = Popen("pactl list sources | grep \"analog.*monitor\" | perl -pe 's/.* //g'", shell = True, stdout=PIPE).stdout.read()
        return str(test)[2:-3]



    def get_tmp_dir(self): # TODO replace this with better solution
        home_dir = os.getenv("HOME")
        return home_dir


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


class Timer():

    def __init__(self, label):
        self.time = 1
        self.label = label
        self.ongoing = None

        self.label.set_text("00∶00")

    def displaytimer(self):
        if self.ongoing is False:
            return False
        self.label.set_text(time.strftime("%M∶%S", time.gmtime(self.time)))
        self.time += 1
        return True

    def start(self):
        GLib.timeout_add(1000, self.displaytimer)

    def stop(self):
        self.ongoing = False
        self.label.set_text("00∶00")



class DelayTimer():

    def __init__(self, label, function):
        self.label = label
        self.function = function
        self.delaycancel = None

    def start(self, time_delay):
        self.time_delay = time_delay * 100
        if self.time_delay > 0:
            def countdown():
                if self.time_delay > 0:
                    self.time_delay -=10
                    GLib.timeout_add(100, countdown)
                    self.label.set_label(str((self.time_delay // 100)+1))
                else:
                    if not self.delaycancel:
                        self.function()
                        self.time_delay = time_delay * 100
                    else:
                        self.delaycancel = False
            countdown()
        else:
            self.function()


    def cancel(self):
        self.time_delay = 0
        self.delaycancel = True
    
