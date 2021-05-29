from enum import Enum
import time
import os

from gi.repository import Gio, GLib


class AudioSourceType(Enum):
    ONLY_AUDIO = 0
    ONLY_MIC = 1
    BOTH = 2


class Settings(Gio.Settings):

    def __init__(self):
        super().__init__('io.github.seadve.Kooha')

    def get_audio_option(self):
        is_record_audio = self.get_boolean('record-audio')
        is_record_mic = self.get_boolean('record-microphone')
        if is_record_audio and is_record_mic:
            audio_option = AudioSourceType.BOTH
        elif is_record_audio and not is_record_mic:
            audio_option = AudioSourceType.ONLY_AUDIO
        elif is_record_mic and not is_record_audio:
            audio_option = AudioSourceType.ONLY_MIC
        return audio_option

    def get_video_framerate(self):
        return self.get_int('video-framerate')

    def get_is_show_pointer(self):
        return self.get_boolean('show-pointer')

    def get_saving_location(self):
        saving_location = self.get_string('saving-location')
        if saving_location == 'default':
            saving_location = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_VIDEOS)
            if not os.path.exists(saving_location):
                saving_location = GLib.get_home_dir()
        return saving_location

    def get_video_format(self):
        return self.get_string('video-format')

    def get_file_path(self):
        saving_location = self.get_saving_location()
        filename = f"Kooha-{time.strftime('%Y-%m-%d-%H:%M:%S', time.localtime())}"
        video_format = self.get_video_format()
        return os.path.join(saving_location, f'{filename}.{video_format}')

    def get_record_delay(self):
        return int(self.get_string('record-delay'))
