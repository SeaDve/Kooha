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

from gi.repository import Gst, Gtk, Adw

from kooha.backend.recorder import Recorder  # noqa: F401
from kooha.backend.timer import Timer, TimerState  # noqa: F401

# TODO implement kb shortcuts for capture mode
# TODO implement ui support for pause and resume


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/window.ui')
class KoohaWindow(Adw.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    start_record_button = Gtk.Template.Child()
    title_stack = Gtk.Template.Child()
    main_stack = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()

    recorder = Gtk.Template.Child()
    timer = Gtk.Template.Child()

    def __init__(self, settings, **kwargs):
        super().__init__(**kwargs)
        self.settings = settings

        self.start_record_button.grab_focus()
        self._setup_actions()

    def _setup_actions(self):
        settings_actions = [
            'record-speaker', 'record-mic', 'show-pointer',
            'record-delay', 'video-format',
        ]

        for action in settings_actions:
            settings_action = self.settings.create_action(action)
            self.add_action(settings_action)

    @Gtk.Template.Callback()
    def _on_recorder_state_notify(self, recorder, state):
        if recorder.state == Gst.State.NULL:
            self.main_stack.set_visible_child_name('main-screen')
            self.props.application.new_notification(
                title=_("Screencast Recorded!"),
                body=f'{_("The recording has been saved in")} {self.settings.get_saving_location()}',
                action='app.show-saving-location',
            )
            self.timer.stop()
        elif recorder.state == Gst.State.PLAYING:
            self.main_stack.set_visible_child_name('recording')

    @Gtk.Template.Callback()
    def _on_timer_state_notify(self, timer, state):
        if timer.state == TimerState.DELAYED:
            self.main_stack.set_visible_child_name('delay')
        elif timer.state == TimerState.STOPPED:
            self.main_stack.set_visible_child_name('main-screen')

    @Gtk.Template.Callback()
    def _on_timer_time_notify(self, timer, time):
        self.delay_label.set_label(str(timer.time))
        self.time_recording_label.set_label("%02d∶%02d" % divmod(timer.time, 60))

    @Gtk.Template.Callback()
    def _on_timer_delay_done(self, timer):
        self.recorder.start()

    @Gtk.Template.Callback()
    def _on_recorder_ready(self, recorder):
        record_delay = self.settings.get_record_delay()
        self.timer.start(record_delay)

    @Gtk.Template.Callback()
    def _on_start_record_button_clicked(self, button):
        self.recorder.ready()

    @Gtk.Template.Callback()
    def _on_stop_record_button_clicked(self, button):
        self.recorder.stop()

    @Gtk.Template.Callback()
    def _on_cancel_delay_button_clicked(self, button):
        self.timer.stop()
