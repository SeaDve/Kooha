# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from collections import namedtuple
import os
import time

from gi.repository import Gio, GLib

AudioOption = namedtuple('AudioOption', 'record_speaker record_mic')

# TODO fix getting home dir since sandbox can't access it


class Settings(Gio.Settings):

    def __init__(self):
        super().__init__('io.github.seadve.Kooha')

    def get_audio_option(self):
        is_record_speaker = self.get_boolean('record-speaker')
        is_record_mic = self.get_boolean('record-mic')
        return AudioOption(is_record_speaker, is_record_mic)

    def get_video_framerate(self):
        return self.get_int('video-framerate')

    def get_is_show_pointer(self):
        return self.get_boolean('show-pointer')

    def set_saving_location(self, directory):
        self.set_string('saving-location', directory)

    def get_saving_location(self):
        saving_location = self.get_string('saving-location')
        if saving_location == 'default':
            saving_location = GLib.get_user_special_dir(GLib.UserDirectory.DIRECTORY_VIDEOS)
            if not os.path.exists(saving_location or ''):
                saving_location = GLib.get_home_dir()
        return saving_location

    def get_video_format(self):
        return self.get_string('video-format')

    def get_file_path(self):
        saving_location = self.get_saving_location()
        filename = f"Kooha {time.strftime('%m-%d-%Y %H:%M:%S', time.localtime())}"
        video_format = self.get_video_format()
        return os.path.join(saving_location, f'{filename}.{video_format}')

    def get_record_delay(self):
        return int(self.get_string('record-delay'))
