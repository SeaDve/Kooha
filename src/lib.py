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

import os
from subprocess import PIPE, Popen

from gi.repository import GLib, Gio, Gst


class Timer:
    def __init__(self, label):
        self.label = label
        self.ongoing = False
        GLib.timeout_add_seconds(1, self.refresh_time, priority=GLib.PRIORITY_LOW)

    def refresh_time(self):
        if self.ongoing:
            self.time += 1
            self.label.set_text("%02d∶%02d" % divmod(self.time, 60))
        return True

    def start(self):
        self.time = 0
        self.label.set_text("00∶00")
        self.ongoing = True

    def stop(self):
        self.ongoing = False


class DelayTimer:
    def __init__(self, label, function):
        self.label = label
        self.function = function

    def displaydelay(self):
        if self.time_delay == 10 or self.delaycancel:
            if not self.delaycancel:
                self.function()
            return False
        self.time_delay -= 10
        self.label.set_text(str(self.time_delay // 100 + 1))
        return True

    def start(self, time_delay):
        if time_delay > 0:
            self.time_delay = time_delay * 100
            self.delaycancel = False
            self.label.set_text(str(time_delay))
            GLib.timeout_add(100, self.displaydelay)
        else:
            self.function()

    def cancel(self):
        self.delaycancel = True


class AudioRecorder:
    def __init__(self, record_audio, record_microphone, saving_location):
        self.record_audio = record_audio
        self.record_microphone = record_microphone
        self.saving_location = saving_location

    def start(self):
        self.default_audio_output = self.get_default_audio_output()
        self.default_audio_input = self.get_default_audio_input()

        if (self.record_audio and self.default_audio_output) or (self.record_microphone and self.default_audio_input):
            if self.record_audio and self.default_audio_output:
                audio_pipeline = f'pulsesrc device="{self.default_audio_output}" ! audioconvert ! vorbisenc ! oggmux ! filesink location={self.get_tmp_dir()}/.Kooha_tmpaudio.ogg'

            elif self.record_microphone and self.default_audio_input:
                audio_pipeline = f'pulsesrc device="{self.default_audio_input}" ! audioconvert ! vorbisenc ! oggmux ! filesink location={self.get_tmp_dir()}/.Kooha_tmpaudio.ogg'

            if (self.record_audio and self.default_audio_output) and (self.record_microphone and self.default_audio_input):
                audio_pipeline = f'pulsesrc device="{self.default_audio_output}" ! audiomixer name=mix ! audioconvert ! vorbisenc ! oggmux ! filesink location={self.get_tmp_dir()}/.Kooha_tmpaudio.ogg pulsesrc device="{self.default_audio_input}" ! queue ! mix.'

            self.audio_gst = Gst.parse_launch(audio_pipeline)
            bus = self.audio_gst.get_bus()
            bus.add_signal_watch()
            bus.connect("message", self.on_message)

            self.audio_gst.set_state(Gst.State.PLAYING)

    def stop(self):
        if (self.record_audio and self.default_audio_output) or (self.record_microphone and self.default_audio_input):
            self.audio_gst.set_state(Gst.State.NULL)

            self.joiner_gst = Gst.parse_launch(f"matroskamux name=mux ! filesink location={self.saving_location} filesrc location={self.get_tmp_dir()}/.Kooha_tmpvideo.mkv ! matroskademux ! vp8dec ! queue ! vp8enc min_quantizer=10 max_quantizer=10 cpu-used=3 cq_level=13 deadline=1 static-threshold=100 threads=3 ! queue ! mux. filesrc location={self.get_tmp_dir()}/.Kooha_tmpaudio.ogg ! oggdemux ! mux.")
            bus = self.joiner_gst.get_bus()
            bus.add_signal_watch()
            bus.connect('message', self.stop_message)
            self.joiner_gst.set_state(Gst.State.PLAYING)

    def stop_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.joiner_gst.set_state(Gst.State.NULL)
            print("Done Processing")
        elif t == Gst.MessageType.ERROR:
            self.joiner_gst.set_state(Gst.State.NULL)
            err, debug = message.parse_error()
            print("Error: %s" % err, debug)

    def on_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.audio_gst.set_state(Gst.State.NULL)
        elif t == Gst.MessageType.ERROR:
            self.audio_gst.set_state(Gst.State.NULL)
            err, debug = message.parse_error()
            print("audio_gst Error: %s" % err, debug)

    def get_default_audio_output(self):
        pactl_output = str(Popen("pactl list sources | grep \"Name: alsa_output\" | cut -d\" \" -f2", shell = True, stdout=PIPE).stdout.read(), "utf-8")
        if not pactl_output:
            return None
        return pactl_output.split("\n")[-2]

    def get_default_audio_input(self):
        pactl_output = str(Popen("pactl list sources | grep \"Name: alsa_input\" | cut -d\" \" -f2", shell = True, stdout=PIPE).stdout.read(), "utf-8")
        if not pactl_output:
            return None
        return pactl_output.split("\n")[-2]

    def get_tmp_dir(self):
        video_dir = f"{os.getenv('XDG_CACHE_HOME')}/tmp"
        return video_dir


class VideoRecorder:
    def __init__(self, fullscreen_mode_toggle):
        self.fullscreen_mode_toggle = fullscreen_mode_toggle

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


    def start(self, directory, framerate, show_pointer, pipeline):
        self.directory = directory
        self.framerate = framerate
        self.show_pointer = show_pointer
        self.pipeline = pipeline

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
