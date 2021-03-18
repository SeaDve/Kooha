# recorders.py
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

from subprocess import PIPE, Popen

from gi.repository import GLib, Gio, Gst


class AudioRecorder:
    def __init__(self, record_audio, record_microphone, saving_location):
        self.record_audio = record_audio
        self.record_microphone = record_microphone
        self.saving_location = saving_location.replace(" ", "\ ")

    def start(self):
        self.default_audio_output = self.get_default_audio_source("alsa_output")
        self.default_audio_input = self.get_default_audio_source("alsa_input")

        if (self.record_audio and self.default_audio_output) or (self.record_microphone and self.default_audio_input):
            if self.record_audio and self.default_audio_output:
                audio_pipeline = f'pulsesrc device="{self.default_audio_output}" ! audioconvert ! vorbisenc ! oggmux ! filesink location={self.get_tmp_dir("audio")}'

            elif self.record_microphone and self.default_audio_input:
                audio_pipeline = f'pulsesrc device="{self.default_audio_input}" ! audioconvert ! vorbisenc ! oggmux ! filesink location={self.get_tmp_dir("audio")}'

            elif (self.record_audio and self.default_audio_output) and (self.record_microphone and self.default_audio_input):
                audio_pipeline = f'pulsesrc device="{self.default_audio_output}" ! audiomixer name=mix ! audioconvert ! vorbisenc ! oggmux ! filesink location={self.get_tmp_dir("audio")} pulsesrc device="{self.default_audio_input}" ! queue ! mix.'

            self.audio_gst = Gst.parse_launch(audio_pipeline)
            bus = self.audio_gst.get_bus()
            bus.add_signal_watch()
            bus.connect("message", self.on_audio_gst_message)
            self.audio_gst.set_state(Gst.State.PLAYING)

    def stop(self):
        if (self.record_audio and self.default_audio_output) or (self.record_microphone and self.default_audio_input):
            self.audio_gst.set_state(Gst.State.NULL)

            self.joiner_gst = Gst.parse_launch(f'matroskamux name=mux ! filesink location={self.saving_location} filesrc location={self.get_tmp_dir("video")} ! matroskademux ! vp8dec ! queue ! vp8enc min_quantizer=10 max_quantizer=10 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 threads=3 ! queue ! mux. filesrc location={self.get_tmp_dir("audio")} ! oggdemux ! mux.')
            bus = self.joiner_gst.get_bus()
            bus.add_signal_watch()
            bus.connect('message', self.on_joiner_gst_message)
            self.joiner_gst.set_state(Gst.State.PLAYING)

    def on_joiner_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.joiner_gst.set_state(Gst.State.NULL)
            print("Done Processing")
        elif t == Gst.MessageType.ERROR:
            self.joiner_gst.set_state(Gst.State.NULL)
            err, debug = message.parse_error()
            print("Error: %s" % err, debug)

    def on_audio_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.audio_gst.set_state(Gst.State.NULL)
        elif t == Gst.MessageType.ERROR:
            self.audio_gst.set_state(Gst.State.NULL)
            err, debug = message.parse_error()
            print("audio_gst Error: %s" % err, debug)

    def get_default_audio_source(self, source):
        pactl_command = f"pactl list sources | grep \"Name: {source}\" | cut -d\" \" -f2"
        pactl_output = Popen(pactl_command, shell=True, text=True, stdout=PIPE).stdout.read().split("\n")
        print(f"Detected sources: {pactl_output}")
        if len(pactl_output) == 1:
            return None
        if source == "alsa_output":
            return pactl_output[0]
        elif source == "alsa_input":
            return pactl_output[-2]

    def get_tmp_dir(self, media_type):
        if media_type == "audio":
            extension = ".ogg"
        elif media_type == "video":
            extension = ".mkv"
        directory = GLib.getenv('XDG_CACHE_HOME')
        if not directory:
            directory = ""
        return f"{directory}/tmp/tmp{media_type}{extension}"


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
            self.GNOMEScreencast.call(
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
            self.GNOMEScreencast.call(
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
