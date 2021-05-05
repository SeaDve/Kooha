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

from gi.repository import Gio, GLib, Gst


class AudioRecorder:
    def __init__(self, saving_location, record_audio, record_microphone):
        self.saving_location = saving_location.replace(" ", r"\ ")
        self.record_audio = record_audio
        self.record_microphone = record_microphone

        if self.record_audio or self.record_microphone:
            self.default_audio_output, self.default_audio_input = self.get_default_audio_devices()
            print(f"Default sink: {self.default_audio_output} \nDefault source: {self.default_audio_input}")

    def start(self):
        final_sink = f'! audioconvert ! opusenc ! webmmux ! filesink location={self.get_tmp_dir("audio")}'
        if self.record_audio and self.default_audio_output:
            audio_pipeline = f'pulsesrc device="{self.default_audio_output}" {final_sink}'

        elif self.record_microphone and self.default_audio_input:
            audio_pipeline = f'pulsesrc device="{self.default_audio_input}" {final_sink}'

        if ((self.record_audio and self.default_audio_output)
                and (self.record_microphone and self.default_audio_input)):
            audio_pipeline = (f'pulsesrc device="{self.default_audio_output}" ! audiomixer name=mix '
                              f'{final_sink} pulsesrc device="{self.default_audio_input}" ! queue ! mix.')

        self.audio_gst = Gst.parse_launch(audio_pipeline)
        bus = self.audio_gst.get_bus()
        bus.add_signal_watch()
        bus.connect("message", self._on_audio_gst_message)
        self.audio_gst.set_state(Gst.State.PLAYING)

    def stop(self, window):
        self.window = window

        self.audio_gst.send_event(Gst.Event.new_eos())
        joiner_pipeline = (f'matroskamux name=mux ! filesink location={self.saving_location} '
                           f'filesrc location={self.get_tmp_dir("video")} ! matroskademux ! vp8dec '
                           '! queue ! vp8enc min_quantizer=10 max_quantizer=10 cpu-used=16 cq_level=13 '
                           'deadline=1 static-threshold=100 threads=3 ! queue ! mux. '
                           f'filesrc location={self.get_tmp_dir("audio")} ! matroskademux ! mux.')
        self.joiner_gst = Gst.parse_launch(joiner_pipeline)
        bus = self.joiner_gst.get_bus()
        bus.add_signal_watch()
        bus.connect('message', self._on_joiner_gst_message)
        self.joiner_gst.set_state(Gst.State.PLAYING)

    def _on_joiner_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.joiner_gst.set_state(Gst.State.NULL)
            self.window.main_stack.set_visible_child(self.window.main_screen_box)
            self.window.send_recordingfinished_notification()
        elif t == Gst.MessageType.ERROR:
            self.joiner_gst.set_state(Gst.State.NULL)
            err, debug = message.parse_error()
            print("Error: %s" % err, debug)

    def _on_audio_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.audio_gst.set_state(Gst.State.NULL)
        elif t == Gst.MessageType.ERROR:
            self.audio_gst.set_state(Gst.State.NULL)
            err, debug = message.parse_error()
            print("audio_gst Error: %s" % err, debug)

    @staticmethod
    def get_default_audio_devices():
        pactl_output = Popen(
            'pactl info | tail -n +13 | cut -d" " -f3',
            shell=True,
            text=True,
            stdout=PIPE
        ).stdout.read().rstrip()
        device_list = pactl_output.split("\n")
        default_sink = f"{device_list[0]}.monitor"
        default_source = device_list[1]
        if default_sink == default_source:
            return (default_sink, None)
        return (default_sink, default_source)

    @staticmethod
    def get_tmp_dir(media_type):
        extension_list = {"audio": "ogg", "video": "mkv"}
        extension = extension_list[media_type]
        directory = GLib.getenv('XDG_CACHE_HOME')
        if not directory:
            directory = ""
        return f"{directory}/tmp/tmp{media_type}.{extension}"


class VideoRecorder:
    def __init__(self):
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.gnome_screencast = Gio.DBusProxy.new_sync(
            bus,
            Gio.DBusProxyFlags.NONE,
            None,
            "org.gnome.Shell.Screencast",
            "/org/gnome/Shell/Screencast",
            "org.gnome.Shell.Screencast",
            None
        )

        self.gnome_selectarea = Gio.DBusProxy.new_sync(
            bus,
            Gio.DBusProxyFlags.NONE,
            None,
            "org.gnome.Shell.Screenshot",
            "/org/gnome/Shell/Screenshot",
            "org.gnome.Shell.Screenshot",
            None
        )

    def start(self, directory, framerate, show_pointer):
        self.directory = directory
        self.framerate = framerate
        self.show_pointer = show_pointer
        self.pipeline = ("videoconvert chroma-mode=GST_VIDEO_CHROMA_MODE_NONE dither=GST_VIDEO_DITHER_NONE "
                         "matrix-mode=GST_VIDEO_MATRIX_MODE_OUTPUT_ONLY n-threads=3 ! queue ! vp8enc "
                         "cpu-used=16 max-quantizer=10 deadline=1 keyframe-mode=disabled threads=3 "
                         "static-threshold=1000 buffer-size=20000 ! queue ! matroskamux")

        if not self.selection_mode:
            self.gnome_screencast.call_sync(
                "Screencast",
                GLib.Variant.new_tuple(
                    GLib.Variant.new_string(self.directory),
                    GLib.Variant("a{sv}", {
                        "framerate": GLib.Variant("i", self.framerate),
                        "draw-cursor": GLib.Variant("b", self.show_pointer),
                        "pipeline": GLib.Variant("s", self.pipeline)
                    }),
                ),
                Gio.DBusProxyFlags.NONE,
                -1,
                None
            )

        elif self.selection_mode:
            self.gnome_screencast.call_sync(
                "ScreencastArea",
                GLib.Variant.new_tuple(
                    GLib.Variant("i", self.coordinates[0]),
                    GLib.Variant("i", self.coordinates[1]),
                    GLib.Variant("i", self.coordinates[2] // 2 * 2),
                    GLib.Variant("i", self.coordinates[3] // 2 * 2),
                    GLib.Variant.new_string(self.directory),
                    GLib.Variant("a{sv}", {
                        "framerate": GLib.Variant("i", self.framerate),
                        "draw-cursor": GLib.Variant("b", self.show_pointer),
                        "pipeline": GLib.Variant("s", self.pipeline)
                    }),
                ),
                Gio.DBusProxyFlags.NONE,
                -1,
                None
            )

    def stop(self):
        self.gnome_screencast.call_sync(
            "StopScreencast",
            None,
            Gio.DBusCallFlags.NONE,
            -1,
            None
        )

    def set_fullscreen_mode(self):
        self.selection_mode = False

    def set_selection_mode(self):
        self.coordinates = self.gnome_selectarea.call_sync(
            "SelectArea",
            None,
            Gio.DBusProxyFlags.NONE,
            -1,
            None
        )
        self.selection_mode = True
