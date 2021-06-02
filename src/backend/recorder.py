from subprocess import PIPE, Popen

from gi.repository import GObject, Gst

from kooha.backend.portal import Portal
from kooha.backend.settings import Settings
from kooha.backend.pipeline_builder import PipelineBuilder

Gst.init(None)

# TODO avoid redundant pipeline state setting
# TODO implement area recording


class Recorder(GObject.GObject):
    __gtype_name__ = 'Recorder'

    state = GObject.Property(type=Gst.State, default=Gst.State.NULL)

    def __init__(self):

        self.portal = Portal()
        self.portal.connect('ready', self._on_portal_ready)
        self.settings = Settings()

    def _on_portal_ready(self, portal):
        framerate = self.settings.get_video_framerate()
        video_format = self.settings.get_video_format()
        file_path = self.settings.get_file_path().replace(" ", r"\ ")
        audio_source_type = self.settings.get_audio_option()
        fd, node_id = self.portal.get_screen_info()
        default_audio_sources = self._get_default_audio_sources()

        pipeline_builder = PipelineBuilder(fd, node_id, framerate, file_path, video_format, audio_source_type)
        pipeline_builder.set_audio_source(*default_audio_sources)
        self.pipeline = pipeline_builder.build()

        self.pipeline.set_state(Gst.State.PLAYING)
        self.record_bus = self.pipeline.get_bus()
        self.record_bus.add_signal_watch()
        self.handler_id = self.record_bus.connect('message', self._on_gst_message)

    def _on_gst_message(self, bus, message):
        t = message.type
        if t == Gst.MessageType.EOS:
            self.pipeline.set_state(Gst.State.NULL)
        elif t == Gst.MessageType.ERROR:
            err, debug = message.parse_error()
            print("Error: %s" % err, debug)

    def _get_default_audio_sources(self):
        pactl_output = Popen(
            'pactl info | tail -n +13 | cut -d" " -f3',
            shell=True,
            text=True,
            stdout=PIPE
        ).stdout.read().rstrip()
        device_list = pactl_output.split("\n")
        default_sink = f'{device_list[0]}.monitor'
        default_source = device_list[1]
        if default_sink == default_source:
            return default_sink, None
        return default_sink, default_source

    def start(self):
        self.portal.open()
        # TODO add support for optional cursor recording
        # TODO handle cancled open portal

    def pause(self):
        self.pipeline.set_state(Gst.State.PAUSED)
        self.state = Gst.State.PAUSED

    def resume(self):
        self.pipeline.set_state(Gst.State.PLAYING)
        self.state = Gst.State.PLAYING

    def stop(self):
        self.pipeline.set_state(Gst.State.NULL)
        self.record_bus.remove_watch()
        self.record_bus.disconnect(self.handler_id)

        self.portal.close()
