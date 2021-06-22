# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from collections import namedtuple

from gi.repository import Gtk, GObject

Point = namedtuple('Point', 'x y')
Rectangle = namedtuple('Rectangle', 'x y w h')
Screen = namedtuple('Screen', 'w h')


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/area_selector.ui')
class AreaSelector(Gtk.Window):
    __gtype_name__ = 'AreaSelector'
    __gsignals__ = {'captured': (GObject.SIGNAL_RUN_FIRST, None, (object, object)),
                    'cancelled': (GObject.SIGNAL_RUN_FIRST, None, ())}

    drawing_area = Gtk.Template.Child()

    def __init__(self):
        super().__init__()
        self.drawing_area.set_cursor_from_name('crosshair')
        self.drawing_area.set_draw_func(self._drawing_area_clean)

    @Gtk.Template.Callback()
    def _on_pressed_notify(self, gesture, n_press, x, y):
        self.dragging = True
        self.start_point = Point(x, y)

    @Gtk.Template.Callback()
    def _on_released_notify(self, gesture, n_press, x, y):
        self.dragging = False
        self.end_point = Point(x, y)

        selection_rectangle = self._get_geometry(self.start_point, self.end_point)
        actual_screen = Screen(self.get_width(), self.get_height())

        self.emit('captured', selection_rectangle, actual_screen)
        self.drawing_area.set_draw_func(self._drawing_area_clean)

    @Gtk.Template.Callback()
    def _on_motion_notify(self, controller, x, y):
        if not self.dragging:
            return

        w = x - self.start_point.x
        h = y - self.start_point.y

        self.drawing_area.set_draw_func(self._drawing_area_draw,
                                        self.start_point.x, self.start_point.y,
                                        w, h)

    @Gtk.Template.Callback()
    def _on_key_pressed_notify(self, controller, keyval, keycode, state):
        if keyval == 65307:
            self.emit('cancelled')

    @Gtk.Template.Callback()
    def _on_close_request(self, window):
        self.emit('cancelled')

    def _drawing_area_draw(self, dwa, ctx, dwa_w, dwa_h, x, y, w, h):
        ctx.rectangle(x, y, w, h)
        ctx.set_source_rgba(0.1, 0.45, 0.8, 0.3)
        ctx.fill_preserve()
        ctx.set_source_rgb(0.1, 0.45, 0.8)
        ctx.set_line_width(1)
        ctx.stroke()

    def _drawing_area_clean(self, dwa, ctx, dwa_w, dwa_h):
        ctx.rectangle(1.5, 1.5, dwa_w - 3, dwa_h - 3)
        ctx.set_source_rgb(1, 1, 1)
        ctx.set_line_width(3)
        ctx.stroke()

    def _get_geometry(self, p1, p2):
        min_x = min(p1.x, p2.x)
        min_y = min(p1.y, p2.y)
        w = abs(p1.x - p2.x)
        h = abs(p1.y - p2.y)

        if w == h == 0:
            w, min_x = min_x, w
            h, min_y = min_y, h

        return Rectangle(min_x, min_y, w, h)

    def select_area(self):
        self.dragging = False
        self.fullscreen()
        self.present()
