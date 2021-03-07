# lib.py
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
import os

from time import strftime, gmtime, sleep

from subprocess import PIPE, Popen

from gi.repository import GLib, Gio

class Timer:

    def __init__(self, label):
        self.label = label
        self.time = 1
        self.ongoing = True

        self.label.set_text("00∶00")

    def displaytimer(self):
        if not self.ongoing:
            return False
        self.label.set_text(strftime("%M∶%S", gmtime(self.time)))
        self.time += 1
        return True

    def start(self):
        GLib.timeout_add(1000, self.displaytimer)

    def stop(self):
        self.ongoing = False



class DelayTimer:

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
                    self.label.set_text(str((self.time_delay // 100)+1))
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



class AudioRecorder:

    def __init__(self, record_audio, record_microphone, saving_location):
        self.record_audio = record_audio
        self.record_microphone = record_microphone
        self.saving_location = saving_location

    def start(self):
        if self.record_audio or self.record_microphone:
            command_list = ["ffmpeg -f"]

            if self.record_audio:
                command_list.append("pulse -i {0}".format(self.get_default_audio_output())) # TODO test this with other devices

            if self.record_audio and self.record_microphone:
                command_list.append("-f")

            if self.record_microphone:
                command_list.append("pulse -i default")

            if self.record_audio and self.record_microphone:
                command_list.append("-filter_complex amerge -ac 2")
                #command_list.append("-preset veryfast")

            command_list.append("{0}/.Kooha_tmpaudio.mkv -y".format(self.get_tmp_dir()))

            command = " ".join(command_list)
            self.audio_subprocess = Popen(command, shell=True)

            command_list.clear()

    def stop(self):
        if self.record_audio or self.record_microphone:
            self.audio_subprocess.send_signal(signal.SIGINT)
            sleep(1) # TODO replace with more ideal solution
            Popen("ffmpeg -i {0}/.Kooha_tmpvideo.mkv -i {0}/.Kooha_tmpaudio.mkv -c:v copy -c:a aac {1} -y".format(self.get_tmp_dir(), self.saving_location), shell=True)

    def get_default_audio_output(self):
        test = Popen("pactl list sources | grep \"analog.*monitor\" | perl -pe 's/.* //g'", shell = True, stdout=PIPE).stdout.read()
        return str(test)[2:-3]

    def get_tmp_dir(self): # TODO test with other device
        video_dir = "{0}/tmp".format(os.getenv("XDG_CACHE_HOME"))
        return video_dir



class VideoRecorder:

    def __init__(self, fullscreen_mode_toggle, framerate, show_pointer, pipeline, directory):
        self.fullscreen_mode_toggle = fullscreen_mode_toggle
        self.framerate = framerate
        self.show_pointer = show_pointer
        self.pipeline = pipeline
        self.directory = directory

        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.GNOMEScreencast = Gio.DBusProxy.new_sync(
                    bus,
                    Gio.DBusProxyFlags.NONE,
                    None,
                    "org.gnome.Shell.Screencast",
                    "/org/gnome/Shell/Screencast",
                    "org.gnome.Shell.Screencast",
                    None)

        self.GNOMESelectArea = Gio.DBusProxy.new_sync(
                    bus,
                    Gio.DBusProxyFlags.NONE,
                    None,
                    "org.gnome.Shell.Screenshot",
                    "/org/gnome/Shell/Screenshot",
                    "org.gnome.Shell.Screenshot",
                    None)


    def start(self):
        if self.fullscreen_mode_toggle.get_active():
            self.GNOMEScreencast.call_sync(
                        "Screencast",
                        GLib.Variant.new_tuple(
                            GLib.Variant.new_string(self.directory),
                            GLib.Variant("a{sv}",
                                {"framerate": GLib.Variant("i", self.framerate),
                                 "draw-cursor": GLib.Variant("b", self.show_pointer),
                                 "pipeline": GLib.Variant("s", self.pipeline)}
                            ),
                        ),
                        Gio.DBusProxyFlags.NONE,
                        -1,
                        None)

        elif not self.fullscreen_mode_toggle.get_active():
            self.GNOMEScreencast.call_sync(
                    "ScreencastArea",
                    GLib.Variant.new_tuple(
                        GLib.Variant("i", self.coordinates[0]),
                        GLib.Variant("i", self.coordinates[1]),
                        GLib.Variant("i", self.coordinates[2]),
                        GLib.Variant("i", self.coordinates[3]),
                        GLib.Variant.new_string(self.directory),
                        GLib.Variant("a{sv}",
                            {"framerate": GLib.Variant("i", self.framerate),
                             "draw-cursor": GLib.Variant("b", self.show_pointer),
                             "pipeline": GLib.Variant("s", self.pipeline)}
                        ),
                    ),
                    Gio.DBusProxyFlags.NONE,
                    -1,
                    None)

    def stop(self):
        self.GNOMEScreencast.call_sync(
            "StopScreencast",
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None)

    def get_coordinates(self):
        self.coordinates = self.GNOMESelectArea.call_sync("SelectArea", None, Gio.DBusProxyFlags.NONE, -1, None)

    def set_directory(self, directory):
        self.directory = directory
