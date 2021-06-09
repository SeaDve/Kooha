# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import Gst, Gtk, Adw, Gio, GObject

from kooha.backend.recorder import Recorder  # noqa: F401
from kooha.backend.timer import Timer, TimerState  # noqa: F401
from kooha.widgets.error_dialog import ErrorDialog


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/window.ui')
class KoohaWindow(Adw.ApplicationWindow):
    __gtype_name__ = 'KoohaWindow'

    start_record_button = Gtk.Template.Child()
    pause_record_button = Gtk.Template.Child()
    title_stack = Gtk.Template.Child()
    main_stack = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()

    recorder = Gtk.Template.Child()
    timer = Gtk.Template.Child()

    def __init__(self, settings, **kwargs):
        super().__init__(**kwargs)
        self.settings = settings

        self.settings.bind('capture-mode', self.title_stack,
                           'visible-child-name', Gio.SettingsBindFlags.DEFAULT)
        self.recorder.portal.bind_property('is_window_open', self,
                                           'visible', GObject.BindingFlags.INVERT_BOOLEAN)
        self.start_record_button.grab_focus()
        self._setup_actions()

    def _setup_actions(self):
        builder = Gtk.Builder.new_from_resource('/io/github/seadve/Kooha/ui/help_overlay.ui')
        help_overlay = builder.get_object('help_overlay')
        self.set_help_overlay(help_overlay)

        settings_actions = [
            'record-speaker', 'record-mic', 'show-pointer',
            'capture-mode', 'record-delay', 'video-format'
        ]

        for action in settings_actions:
            settings_action = self.settings.create_action(action)
            self.add_action(settings_action)

    @Gtk.Template.Callback()
    def _get_status_label_label(self, window, recorder_state):
        return _("Paused") if recorder_state == Gst.State.PAUSED else _("Recording")

    @Gtk.Template.Callback()
    def _on_recorder_state_notify(self, recorder, state):
        if recorder.state == Gst.State.NULL:
            self.timer.stop()
            self.main_stack.set_visible_child_name('main-screen')
        elif recorder.state == Gst.State.PLAYING:
            self.timer.resume()
            self.main_stack.set_visible_child_name('recording')
            self.pause_record_button.set_icon_name('media-playback-pause-symbolic')
            self.time_recording_label.remove_css_class('paused')
        elif recorder.state == Gst.State.PAUSED:
            self.timer.pause()
            self.pause_record_button.set_icon_name('media-playback-start-symbolic')
            self.time_recording_label.add_css_class('paused')

    @Gtk.Template.Callback()
    def _on_recorder_record_success(self, recorder, saving_location):
        self.props.application.new_notification(
            title=_("Screencast Recorded!"),
            body=_(f"The recording has been saved in {saving_location}"),
            action='app.show-saving-location',
        )

    @Gtk.Template.Callback()
    def _on_recorder_record_failed(self, recorder, error_message):
        error = ErrorDialog(
            parent=self,
            title=_("Sorry! An error has occured."),
            text=_(error_message),
        )
        error.present()

    @Gtk.Template.Callback()
    def _on_timer_state_notify(self, timer, state):
        if timer.state == TimerState.DELAYED:
            self.main_stack.set_visible_child_name('delay')
        elif timer.state == TimerState.STOPPED:
            self.main_stack.set_visible_child_name('main-screen')

    @Gtk.Template.Callback()
    def _on_timer_time_notify(self, timer, time):
        self.delay_label.set_label(str(timer.time))
        self.time_recording_label.set_label("%02d∶%02d" % divmod(timer.time - 1, 60))

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
    def _on_pause_record_button_clicked(self, button):
        if self.recorder.state == Gst.State.PLAYING:
            self.recorder.pause()
        else:
            self.recorder.resume()

    @Gtk.Template.Callback()
    def _on_cancel_delay_button_clicked(self, button):
        self.recorder.portal.close()
        self.timer.stop()
