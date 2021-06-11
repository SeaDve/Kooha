# SPDX-FileCopyrightText: Copyright 2021 SeaDve
# SPDX-License-Identifier: GPL-3.0-or-later

from collections import namedtuple

from gi.repository import Gtk, Gdk, GObject

Point = namedtuple('Point', 'x y')


@Gtk.Template(resource_path='/io/github/seadve/Kooha/ui/area_selector.ui')
class AreaSelector(Gtk.Window):
    __gtype_name__ = 'AreaSelector'
    __gsignals__ = {'captured': (GObject.SIGNAL_RUN_FIRST, None, ()),
                    'cancelled': (GObject.SIGNAL_RUN_FIRST, None, ())}

    drawing_area = Gtk.Template.Child()

    def __init__(self):
        super().__init__()

    @Gtk.Template.Callback()
    def _on_pressed_notify(self, gesture, n_press, x, y):
        self.dragging = True
        self.start_point = Point(x, y)
        self._update_drawing_area(x, y, 0, 0)

    @Gtk.Template.Callback()
    def _on_released_notify(self, gesture, n_press, x, y):
        self.dragging = False
        self.end_point = Point(x, y)
        self._captured()

    @Gtk.Template.Callback()
    def _on_motion_notify(self, controller, x, y):
        if not self.dragging:
            return

        w = x - self.start_point.x
        h = y - self.start_point.y
        self._update_drawing_area(self.start_point.x, self.start_point.y, w, h)

    @Gtk.Template.Callback()
    def _on_key_pressed_notify(self, controller, keyval, keycode, state):
        if keyval == 65307:
            self.close()
            self.emit('cancelled')

    def _update_drawing_area(self, x, y, w, h):
        self.drawing_area.set_draw_func(self._drawing_area_draw, x, y, w, h)

    def _drawing_area_draw(self, dwa, ctx, dwa_w, dwa_h, x, y, w, h):
        ctx.rectangle (x, y, w, h)
        ctx.set_source_rgba(0.1, 0.45, 0.8, 0.3)
        ctx.fill ()

        ctx.rectangle (x, y, w, h)
        ctx.set_source_rgb(0.1, 0.45, 0.8)
        ctx.set_line_width(1)
        ctx.stroke ()

    def _get_topleft_point(self, p1, p2):
        min_x = min(p1.x, p2.x)
        min_y = min(p1.y, p2.y)
        return Point(min_x, min_y)

    def _get_area(self, p1, p2):
        w = abs(p1.x - p2.x)
        h = abs(p1.y - p2.y)
        return w, h

    def _captured(self):
        topleft_point = self._get_topleft_point(self.start_point, self.end_point)

        width, height = self._get_area(self.start_point, self.end_point)
        final_x, final_y = (*topleft_point, )

        if width == height == 0:
            final_x, width = width, final_x
            final_y, height = height, final_y

        self.output_coordinates = int(final_x), int(final_y), int(width), int(height)
        self.close()
        self.emit('captured')
        print(self.output_coordinates)

    def select_area(self):
        self.dragging = False
        self.output_coordinates = None
        self.fullscreen()
        self.present()
        self.set_cursor(Gdk.Cursor.new_from_name('crosshair'))

    def select_area_finish(self):
        return self.output_coordinates     
