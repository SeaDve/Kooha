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
        if self.dragging:
            return

        self.dragging = True
        self.start_point = Point(x, y)
        self._update_drawing_area(x, y, 0, 0)

    @Gtk.Template.Callback()
    def _on_released_notify(self, gesture, n_press, x, y):
        if not self.dragging:
            return

        self.dragging = False
        self.end_point = Point(x, y)
        self._captured()

    @Gtk.Template.Callback()
    def _on_motion_notify(self, controller, x, y):
        if not self.dragging:
            return

        w = (self.start_point.x - x) * -1
        h = (self.start_point.y - y) * -1
        self._update_drawing_area(self.start_point.x, self.start_point.y, w, h)

    @Gtk.Template.Callback()
    def _on_key_pressed_notify(self, controller, keyval, keycode, state):
        if keyval == 65307 and keycode == 9:
            self.close()
            self.emit('cancelled')

    def _update_drawing_area(self, x, y, w, h):
        self.drawing_area.set_draw_func(self._drawing_area_draw, x, y, w, h)

    def _drawing_area_draw(self, dwa, ctx, dwa_w, dwa_h, x, y, w, h):
        ctx.rectangle (x, y, w, h)
        ctx.set_source_rgba(0.078, 0.396, 0.78, 0.3)
        ctx.fill ()

        ctx.rectangle (x, y, w, h)
        ctx.set_source_rgb(0.078, 0.396, 0.78)
        ctx.set_line_width(0.8)
        ctx.stroke ()

    def _get_topleft_most_point(self, *points):
        x_coords = (p.x for p in points)
        y_coords = (p.y for p in points)
        return Point(min(x_coords), min(y_coords))

    def _get_other_two_points(self, p1, p2):
        p3 = Point(p1.x, p2.y)
        p4 = Point(p2.x, p1.y)
        return p3, p4

    def _get_area(self, p1, p2):
        width = abs(p1.x - p2.x)
        height = abs(p1.y - p2.y)
        return width, height

    def _captured(self):
        point_3, point_4 = self._get_other_two_points(self.start_point, self.end_point)
        topleft_most_point = self._get_topleft_most_point(point_3,
                                                          point_4,
                                                          self.start_point,
                                                          self.end_point)
        width, height = self._get_area(self.start_point, self.end_point)

        final_x = topleft_most_point.x
        final_y = topleft_most_point.y

        if width == height == 0:
            final_x = 0
            final_y = 0
            width = topleft_most_point.x
            height = topleft_most_point.y

        self.output_coordinates = int(final_x), int(final_y), int(width), int(height)
        self.close()
        self.emit('captured')

        print(self.output_coordinates)

    def select_area(self):
        self.dragging = False
        self.output_coordinates = None
        self.fullscreen()
        self.present()
        self.get_root().set_cursor(Gdk.Cursor.new_from_name('crosshair'))

    def select_area_finish(self):
        return self.output_coordinates



        
