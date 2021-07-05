# SPDX-FileCopyrightText: Copyright 2020 The GNOME Music developers
# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

import inspect
import os

from gi.repository import GLib


class Logger:

    @staticmethod
    def _log(message, level):
        stack = inspect.stack()

        filename = os.path.basename(stack[2][1])
        line = stack[2][2]
        function = stack[2][3]

        if level in (GLib.LogLevelFlags.LEVEL_DEBUG, GLib.LogLevelFlags.LEVEL_WARNING):
            message = f"({filename}, {function}, {line}) {message}"

        variant_dict = GLib.Variant("a{sv}", {
            "MESSAGE": GLib.Variant("s", str(message)),
            "CODE_FILE": GLib.Variant("s", filename),
            "CODE_LINE": GLib.Variant("i", line),
            "CODE_FUNC": GLib.Variant("s", function)
        })

        GLib.log_variant("io.github.seadve.Kooha", level, variant_dict)

    @staticmethod
    def warning(message):
        Logger._log(message, GLib.LogLevelFlags.LEVEL_WARNING)

    @staticmethod
    def info(message):
        Logger._log(message, GLib.LogLevelFlags.LEVEL_INFO)

    @staticmethod
    def debug(message):
        Logger._log(message, GLib.LogLevelFlags.LEVEL_DEBUG)
