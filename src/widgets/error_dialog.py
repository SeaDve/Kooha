# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from gi.repository import Gtk


class ErrorDialog(Gtk.MessageDialog):

    def __init__(self, parent, title, text):
        super().__init__(modal=True,
                         transient_for=parent,
                         buttons=Gtk.ButtonsType.OK,
                         title=title,
                         text=text)
        self.connect('response', lambda *_: self.close())
