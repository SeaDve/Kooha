# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import Gtk, Adw, GObject

from kooha.backend.recorder_controller import RecorderController  # noqa: F401
from kooha.backend.settings import Settings
from kooha.widgets.audio_toggle_button import AudioToggleButton  # noqa: F401
from kooha.widgets.error_dialog import ErrorDialog


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/window.ui')
class Window(Adw.ApplicationWindow):
    __gtype_name__ = 'Window'

    pause_record_button = Gtk.Template.Child()
    title_stack = Gtk.Template.Child()
    main_stack = Gtk.Template.Child()
    recording_label = Gtk.Template.Child()
    time_recording_label = Gtk.Template.Child()
    delay_label = Gtk.Template.Child()

    controller = Gtk.Template.Child()
    settings = GObject.Property(type=Settings)

    def __init__(self, settings, **kwargs):
        super().__init__(**kwargs)

        self.settings = settings
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
    def _get_audio_toggles_enablement(self, window, settings_video_format):
        return not settings_video_format == 'gif'

    @Gtk.Template.Callback()
    def _on_controller_state_notify(self, controller, pspec):
        if controller.state == RecorderController.State.NULL:
            self.main_stack.set_visible_child_name('main-screen')
        elif controller.state == RecorderController.State.PLAYING:
            self.main_stack.set_visible_child_name('recording')
            self.pause_record_button.set_icon_name('media-playback-pause-symbolic')
            self.recording_label.set_label(_("Recording"))
            self.time_recording_label.remove_css_class('paused')
        elif controller.state == RecorderController.State.PAUSED:
            self.pause_record_button.set_icon_name('media-playback-start-symbolic')
            self.recording_label.set_label(_("Paused"))
            self.time_recording_label.add_css_class('paused')
        elif controller.state == RecorderController.State.DELAYED:
            self.main_stack.set_visible_child_name('delay')

    @Gtk.Template.Callback()
    def _on_controller_time_notify(self, controller, pspec):
        minutes, seconds = divmod(controller.time, 60)
        self.time_recording_label.set_label(f"{minutes:02d}∶{seconds:02d}")
        self.delay_label.set_label(str(controller.time))

    @Gtk.Template.Callback()
    def _on_controller_record_success(self, controller, recording_file_path):
        self.props.application.send_record_success_notification(recording_file_path)

    @Gtk.Template.Callback()
    def _on_controller_record_failed(self, controller, error_message):
        error = ErrorDialog(
            parent=self,
            title=_("Sorry! An error has occured."),
            text=error_message,
        )
        error.present()

    @Gtk.Template.Callback()
    def _on_start_record_button_clicked(self, button):
        record_delay = self.settings.get_record_delay()
        self.controller.start(record_delay)

    @Gtk.Template.Callback()
    def _on_stop_record_button_clicked(self, button):
        self.controller.stop()

    @Gtk.Template.Callback()
    def _on_pause_record_button_clicked(self, button):
        if self.controller.state == RecorderController.State.PLAYING:
            self.controller.pause()
        else:
            self.controller.resume()

    @Gtk.Template.Callback()
    def _on_cancel_delay_button_clicked(self, button):
        self.controller.cancel_delay()
