// Based on gnome-shell's screenshot ui (GPLv2).
// Source: https://gitlab.gnome.org/GNOME/gnome-shell/-/blob/a3c84ca7463ed92b5be6f013a12bce927223f7c5/js/ui/screenshot.js

use gtk::{
    gdk,
    glib::{self, clone},
    graphene::{Point, Rect},
    gsk::RoundedRect,
    prelude::*,
    subclass::prelude::*,
};

use std::{
    cell::{Cell, RefCell},
    fmt,
};

const DEFAULT_SIZE: f64 = 100.0;

const SELECTION_COLOR: gdk::RGBA = gdk::RGBA::WHITE;
const SELECTION_HANDLE_RADIUS: f32 = 12.0;
const SELECTION_LINE_WIDTH: f32 = 2.0;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum CursorType {
    #[default]
    Default,
    Crosshair,
    Move,
    NorthResize,
    SouthResize,
    EastResize,
    WestResize,
    NorthEastResize,
    NorthWestResize,
    SouthEastResize,
    SouthWestResize,
}

impl CursorType {
    fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Crosshair => "crosshair",
            Self::Move => "move",
            Self::NorthResize => "n-resize",
            Self::SouthResize => "s-resize",
            Self::EastResize => "e-resize",
            Self::WestResize => "w-resize",
            Self::NorthEastResize => "ne-resize",
            Self::NorthWestResize => "nw-resize",
            Self::SouthEastResize => "se-resize",
            Self::SouthWestResize => "sw-resize",
        }
    }
}

#[derive(Default, Clone, Copy, glib::Boxed)]
#[boxed_type(name = "KoohaSelection", nullable)]
pub struct Selection {
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
}

impl fmt::Debug for Selection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rect = self.rect();
        f.debug_struct("Selection")
            .field("x", &rect.x())
            .field("y", &rect.y())
            .field("width", &rect.width())
            .field("height", &rect.height())
            .finish()
    }
}

impl Selection {
    pub fn left_x(&self) -> f32 {
        self.start_x.min(self.end_x)
    }

    pub fn right_x(&self) -> f32 {
        self.start_x.max(self.end_x)
    }

    pub fn top_y(&self) -> f32 {
        self.start_y.min(self.end_y)
    }

    pub fn bottom_y(&self) -> f32 {
        self.start_y.max(self.end_y)
    }

    pub fn rect(&self) -> Rect {
        Rect::new(
            self.left_x(),
            self.top_y(),
            (self.start_x - self.end_x).abs(),
            (self.start_y - self.end_y).abs(),
        )
    }
}

mod imp {
    use super::*;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct ViewPort {
        pub(super) paintable: RefCell<Option<gdk::Paintable>>,

        pub(super) paintable_rect: Cell<Option<Rect>>,

        pub(super) selection: Cell<Option<Selection>>,
        pub(super) selection_handles: Cell<Option<[Rect; 4]>>, // [top-left, top-right, bottom-right, bottom-left]

        pub(super) drag_start: Cell<Option<Point>>,
        pub(super) drag_cursor: Cell<CursorType>,

        pub(super) pointer_position: Cell<Option<Point>>,

        pub(super) handler_ids: RefCell<Vec<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ViewPort {
        const NAME: &'static str = "KoohaViewPort";
        type Type = super::ViewPort;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ViewPort {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<gdk::Paintable>("paintable")
                        .explicit_notify()
                        .build(),
                    glib::ParamSpecBoxed::builder::<Selection>("selection")
                        .read_only()
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            let obj = self.obj();

            match pspec.name() {
                "paintable" => {
                    let paintable: Option<gdk::Paintable> = value.get().unwrap();
                    obj.set_paintable(paintable.as_ref());
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "paintable" => obj.paintable().to_value(),
                "selection" => obj.selection().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let motion_controller = gtk::EventControllerMotion::new();
            motion_controller.connect_enter(clone!(@weak obj => move |controller, x, y| {
                obj.on_enter(controller, x, y);
            }));
            motion_controller.connect_motion(clone!(@weak obj => move |controller, x, y| {
                obj.on_motion(controller, x, y);
            }));
            motion_controller.connect_leave(clone!(@weak obj => move |controller| {
                obj.on_leave(controller);
            }));
            obj.add_controller(&motion_controller);

            let gesture_drag = gtk::GestureDrag::builder().exclusive(true).build();
            gesture_drag.connect_drag_begin(clone!(@weak obj => move |controller, x, y| {
                obj.on_drag_begin(controller, x, y);
            }));
            gesture_drag.connect_drag_update(clone!(@weak obj => move |controller, dx, dy| {
                obj.on_drag_update(controller, dx, dy);
            }));
            gesture_drag.connect_drag_end(clone!(@weak obj => move |controller, dx, dy| {
                obj.on_drag_end(controller, dx, dy);
            }));
            obj.add_controller(&gesture_drag);
        }
    }

    impl WidgetImpl for ViewPort {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if for_size == 0 {
                return (0, 0, 0, 0);
            }

            let Some(paintable) = self.obj().paintable() else {
                return (0, 0, -1, -1);
            };

            if orientation == gtk::Orientation::Horizontal {
                let (natural_width, _natural_height) = paintable.compute_concrete_size(
                    0.0,
                    if for_size < 0 { 0.0 } else { for_size as f64 },
                    DEFAULT_SIZE,
                    DEFAULT_SIZE,
                );
                (0, natural_width.ceil() as i32, -1, -1)
            } else {
                let (_natural_width, natural_height) = paintable.compute_concrete_size(
                    if for_size < 0 { 0.0 } else { for_size as f64 },
                    0.0,
                    DEFAULT_SIZE,
                    DEFAULT_SIZE,
                );
                (0, natural_height.ceil() as i32, -1, -1)
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();

            if let Some(paintable) = obj.paintable() {
                let widget_width = obj.width() as f64;
                let widget_height = obj.height() as f64;
                let widget_ratio = widget_width / widget_height;

                let paintable_width = paintable.intrinsic_width() as f64;
                let paintable_height = paintable.intrinsic_height() as f64;
                let paintable_ratio = paintable.intrinsic_aspect_ratio();

                let (width, height) =
                    if widget_width >= paintable_width && widget_height >= paintable_height {
                        (paintable_width, paintable_height)
                    } else if paintable_ratio > widget_ratio {
                        (widget_width, widget_width / paintable_ratio)
                    } else {
                        (widget_height * paintable_ratio, widget_height)
                    };
                let x = (widget_width - width.ceil()) / 2.0;
                let y = (widget_height - height.ceil()).floor() / 2.0;

                obj.imp().paintable_rect.set(Some(Rect::new(
                    x as f32,
                    y as f32,
                    width as f32,
                    height as f32,
                )));

                snapshot.save();
                snapshot.translate(&Point::new(x as f32, y as f32));
                paintable.snapshot(snapshot, width, height);
                snapshot.restore();
            }

            if let Some(selection) = obj.selection() {
                let selection_rect = selection.rect();

                if let Some(paintable_rect) = obj.paintable_rect() {
                    let shade_color = gdk::RGBA::new(0.0, 0.0, 0.0, 0.5);
                    snapshot.append_color(
                        &shade_color,
                        &Rect::new(
                            paintable_rect.x(),
                            paintable_rect.y(),
                            selection.left_x() - paintable_rect.x(),
                            paintable_rect.height(),
                        ),
                    );
                    snapshot.append_color(
                        &shade_color,
                        &Rect::new(
                            selection.right_x(),
                            paintable_rect.y(),
                            paintable_rect.width() + paintable_rect.x() - selection.right_x(),
                            paintable_rect.height(),
                        ),
                    );
                    snapshot.append_color(
                        &shade_color,
                        &Rect::new(
                            selection.left_x(),
                            paintable_rect.y(),
                            selection_rect.width(),
                            selection.top_y() - paintable_rect.y(),
                        ),
                    );
                    snapshot.append_color(
                        &shade_color,
                        &Rect::new(
                            selection.left_x(),
                            selection.bottom_y(),
                            selection_rect.width(),
                            paintable_rect.height() + paintable_rect.y() - selection.bottom_y(),
                        )
                        .normalize_r(),
                    );
                }

                snapshot.append_border(
                    &RoundedRect::from_rect(
                        Rect::new(
                            selection_rect.x(),
                            selection_rect.y(),
                            selection_rect.width().max(1.0),
                            selection_rect.height().max(1.0),
                        ),
                        0.0,
                    ),
                    &[SELECTION_LINE_WIDTH; 4],
                    &[SELECTION_COLOR; 4],
                );

                for handle in self.selection_handles.get().unwrap() {
                    let bounds = RoundedRect::from_rect(handle, SELECTION_HANDLE_RADIUS);
                    snapshot.append_outset_shadow(
                        &bounds,
                        &gdk::RGBA::new(0.0, 0.0, 0.0, 0.2),
                        0.0,
                        1.0,
                        2.0,
                        3.0,
                    );
                    snapshot.push_rounded_clip(&bounds);
                    snapshot.append_color(&SELECTION_COLOR, &handle);
                    snapshot.pop();
                }
            }
        }
    }
}

glib::wrapper! {
     pub struct ViewPort(ObjectSubclass<imp::ViewPort>)
        @extends gtk::Widget;
}

impl ViewPort {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_paintable(&self, paintable: Option<&impl IsA<gdk::Paintable>>) {
        let paintable = paintable.map(|p| p.as_ref());

        if paintable == self.paintable().as_ref() {
            return;
        }

        let _freeze_guard = self.freeze_notify();

        let imp = self.imp();

        let mut handler_ids = imp.handler_ids.borrow_mut();

        if let Some(previous_paintable) = imp.paintable.replace(paintable.cloned()) {
            for handler_id in handler_ids.drain(..) {
                previous_paintable.disconnect(handler_id);
            }
        }

        if let Some(paintable) = paintable {
            handler_ids.push(paintable.connect_invalidate_contents(
                clone!(@weak self as obj => move |_| {
                    obj.queue_draw();
                }),
            ));
            handler_ids.push(paintable.connect_invalidate_size(
                clone!(@weak self as obj => move |_| {
                    obj.queue_resize();
                }),
            ));
        }

        self.queue_resize();
        self.notify("paintable");
    }

    pub fn paintable(&self) -> Option<gdk::Paintable> {
        self.imp().paintable.borrow().clone()
    }

    pub fn selection(&self) -> Option<Selection> {
        self.imp().selection.get()
    }

    pub fn connect_selection_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("selection"), move |obj, _| f(obj))
    }

    pub fn paintable_rect(&self) -> Option<Rect> {
        self.imp().paintable_rect.get()
    }

    pub fn reset_selection(&self) {
        self.set_selection(None);
        self.update_selection_handles();
        self.queue_draw();
    }

    fn set_selection(&self, selection: Option<Selection>) {
        self.imp().selection.set(selection);
        self.notify("selection");
    }

    fn on_enter(&self, _controller: &gtk::EventControllerMotion, x: f64, y: f64) {
        let imp = self.imp();

        imp.pointer_position
            .set(Some(Point::new(x as f32, y as f32)));
    }

    fn on_motion(&self, _controller: &gtk::EventControllerMotion, x: f64, y: f64) {
        let imp = self.imp();

        imp.pointer_position
            .set(Some(Point::new(x as f32, y as f32)));

        if imp.drag_start.get().is_none() {
            let cursor_type = self.compute_cursor_type(x as f32, y as f32);
            self.set_cursor(cursor_type);
        }
    }

    fn on_leave(&self, _controller: &gtk::EventControllerMotion) {
        let imp = self.imp();

        imp.pointer_position.set(None);

        self.set_cursor(CursorType::Default);
    }

    fn on_drag_begin(&self, _gesture: &gtk::GestureDrag, x: f64, y: f64) {
        tracing::debug!("Drag begin at ({}, {})", x, y);

        let imp = self.imp();
        let cursor_type = self.compute_cursor_type(x as f32, y as f32);

        if cursor_type == CursorType::Crosshair {
            imp.drag_cursor.set(CursorType::Crosshair);
            self.set_cursor(CursorType::Crosshair);

            let paintable_rect = self.paintable_rect().unwrap();
            let x = (x as f32).clamp(
                paintable_rect.x(),
                paintable_rect.x() + paintable_rect.width(),
            );
            let y = (y as f32).clamp(
                paintable_rect.y(),
                paintable_rect.y() + paintable_rect.height(),
            );
            self.set_selection(Some(Selection {
                start_x: x,
                start_y: y,
                end_x: x,
                end_y: y,
            }));
            self.update_selection_handles();
        } else {
            imp.drag_cursor.set(cursor_type);
            imp.drag_start.set(Some(Point::new(x as f32, y as f32)));

            let selection = self.selection().unwrap();
            let mut new_selection = self.selection().unwrap();

            if cursor_type == CursorType::Move {
                new_selection.start_x = selection.left_x();
                new_selection.start_y = selection.top_y();
                new_selection.end_x = selection.right_x();
                new_selection.end_y = selection.bottom_y();
            }
            if matches!(
                cursor_type,
                CursorType::NorthWestResize | CursorType::WestResize | CursorType::SouthWestResize
            ) {
                new_selection.start_x = selection.right_x();
                new_selection.end_x = selection.left_x();
            }
            if matches!(
                cursor_type,
                CursorType::NorthEastResize | CursorType::EastResize | CursorType::SouthEastResize
            ) {
                new_selection.start_x = selection.left_x();
                new_selection.end_x = selection.right_x();
            }
            if matches!(
                cursor_type,
                CursorType::NorthWestResize | CursorType::NorthResize | CursorType::NorthEastResize
            ) {
                new_selection.start_y = selection.bottom_y();
                new_selection.end_y = selection.top_y();
            }
            if matches!(
                cursor_type,
                CursorType::SouthWestResize | CursorType::SouthResize | CursorType::SouthEastResize
            ) {
                new_selection.start_y = selection.top_y();
                new_selection.end_y = selection.bottom_y();
            }

            self.set_selection(Some(new_selection));
        }

        self.queue_draw();
    }

    fn on_drag_update(&self, _gesture: &gtk::GestureDrag, _: f64, _: f64) {
        let imp = self.imp();

        let pointer_position = imp.pointer_position.get().unwrap();

        let drag_cursor = imp.drag_cursor.get();

        if drag_cursor == CursorType::Crosshair {
            let Selection {
                start_x, start_y, ..
            } = self.selection().unwrap();
            let paintable_rect = self.paintable_rect().unwrap();
            self.set_selection(Some(Selection {
                start_x,
                start_y,
                end_x: pointer_position.x().clamp(
                    paintable_rect.x(),
                    paintable_rect.width() + paintable_rect.x(),
                ),
                end_y: pointer_position.y().clamp(
                    paintable_rect.y(),
                    paintable_rect.height() + paintable_rect.y(),
                ),
            }));
        } else {
            let drag_start = imp.drag_start.get().unwrap();
            let mut dx = pointer_position.x() - drag_start.x();
            let mut dy = pointer_position.y() - drag_start.y();

            if drag_cursor == CursorType::Move {
                let Selection {
                    start_x,
                    start_y,
                    end_x,
                    end_y,
                } = self.selection().unwrap();
                let mut new_start_x = start_x + dx;
                let mut new_start_y = start_y + dy;
                let mut new_end_x = end_x + dx;
                let mut new_end_y = end_y + dy;

                let mut overshoot_x = 0.0;
                let mut overshoot_y = 0.0;

                let paintable_rect = self.paintable_rect().unwrap();
                let selection_rect = self.selection().unwrap().rect();

                // Keep the size intact if we bumped into the stage edge.
                if new_start_x < paintable_rect.x() {
                    overshoot_x = paintable_rect.x() - new_start_x;
                    new_start_x = paintable_rect.x();
                    new_end_x = new_start_x + selection_rect.width();
                } else if new_end_x > paintable_rect.width() + paintable_rect.x() {
                    overshoot_x = paintable_rect.width() + paintable_rect.x() - new_end_x;
                    new_end_x = paintable_rect.width() + paintable_rect.x();
                    new_start_x = new_end_x - selection_rect.width();
                }
                if new_start_y < paintable_rect.y() {
                    overshoot_y = paintable_rect.y() - new_start_y;
                    new_start_y = paintable_rect.y();
                    new_end_y = new_start_y + selection_rect.height();
                } else if new_end_y > paintable_rect.height() + paintable_rect.y() {
                    overshoot_y = paintable_rect.height() + paintable_rect.y() - new_end_y;
                    new_end_y = paintable_rect.height() + paintable_rect.y();
                    new_start_y = new_end_y - selection_rect.height();
                }

                dx += overshoot_x;
                dy += overshoot_y;

                self.set_selection(Some(Selection {
                    start_x: new_start_x,
                    start_y: new_start_y,
                    end_x: new_end_x,
                    end_y: new_end_y,
                }));
            } else {
                if matches!(drag_cursor, CursorType::WestResize | CursorType::EastResize) {
                    dy = 0.0;
                }
                if matches!(
                    drag_cursor,
                    CursorType::NorthResize | CursorType::SouthResize
                ) {
                    dx = 0.0;
                }

                let paintable_rect = self.paintable_rect().unwrap();
                let mut new_selection = self.selection().unwrap();

                new_selection.end_x += dx;
                if new_selection.end_x >= paintable_rect.width() + paintable_rect.x() {
                    dx -= new_selection.end_x - (paintable_rect.width() + paintable_rect.x());
                    new_selection.end_x = paintable_rect.width() + paintable_rect.x();
                } else if new_selection.end_x < paintable_rect.x() {
                    dx -= new_selection.end_x - paintable_rect.x();
                    new_selection.end_x = paintable_rect.x();
                }

                new_selection.end_y += dy;
                if new_selection.end_y >= paintable_rect.height() + paintable_rect.y() {
                    dy -= new_selection.end_y - (paintable_rect.height() + paintable_rect.y());
                    new_selection.end_y = paintable_rect.height() + paintable_rect.y();
                } else if new_selection.end_y < paintable_rect.y() {
                    dy -= new_selection.end_y - paintable_rect.y();
                    new_selection.end_y = paintable_rect.y();
                }

                self.set_selection(Some(new_selection));
                let selection = new_selection;

                // If we drag the handle past a selection side, update which
                // handles are which.
                if selection.end_x > selection.start_x {
                    if drag_cursor == CursorType::NorthWestResize {
                        imp.drag_cursor.set(CursorType::NorthEastResize);
                    } else if drag_cursor == CursorType::SouthWestResize {
                        imp.drag_cursor.set(CursorType::SouthEastResize);
                    } else if drag_cursor == CursorType::WestResize {
                        imp.drag_cursor.set(CursorType::EastResize);
                    }
                } else {
                    // Disable clippy error
                    if drag_cursor == CursorType::NorthEastResize {
                        imp.drag_cursor.set(CursorType::NorthWestResize);
                    } else if drag_cursor == CursorType::SouthEastResize {
                        imp.drag_cursor.set(CursorType::SouthWestResize);
                    } else if drag_cursor == CursorType::EastResize {
                        imp.drag_cursor.set(CursorType::WestResize);
                    }
                }

                if selection.end_y > selection.start_y {
                    if drag_cursor == CursorType::NorthWestResize {
                        imp.drag_cursor.set(CursorType::SouthWestResize);
                    } else if drag_cursor == CursorType::NorthEastResize {
                        imp.drag_cursor.set(CursorType::SouthEastResize);
                    } else if drag_cursor == CursorType::NorthResize {
                        imp.drag_cursor.set(CursorType::SouthResize);
                    }
                } else {
                    // Disable clippy error
                    if drag_cursor == CursorType::SouthWestResize {
                        imp.drag_cursor.set(CursorType::NorthWestResize);
                    } else if drag_cursor == CursorType::SouthEastResize {
                        imp.drag_cursor.set(CursorType::NorthEastResize);
                    } else if drag_cursor == CursorType::SouthResize {
                        imp.drag_cursor.set(CursorType::NorthResize);
                    }
                }

                self.set_cursor(imp.drag_cursor.get());
            }

            imp.drag_start
                .set(Some(Point::new(drag_start.x() + dx, drag_start.y() + dy)));
        }

        self.update_selection_handles();
        self.queue_draw();
    }

    fn on_drag_end(&self, _gesture: &gtk::GestureDrag, dx: f64, dy: f64) {
        tracing::debug!("Drag end offset ({}, {})", dx, dy);

        let imp = self.imp();
        imp.drag_start.set(None);

        // The user clicked without dragging. Make up a larger selection
        // to reduce confusion.
        if let Some(mut selection) = self.selection() {
            if imp.drag_cursor.get() == CursorType::Crosshair
                && selection.end_x == selection.start_x
                && selection.end_y == selection.start_y
            {
                let offset = 20.0 * self.scale_factor() as f32;
                selection.start_x -= offset;
                selection.start_y -= offset;
                selection.end_x += offset;
                selection.end_y += offset;

                let paintable_rect = self.paintable_rect().unwrap();
                let selection_rect = selection.rect();

                // Keep the coordinates inside the stage.
                if selection.start_x < paintable_rect.x() {
                    selection.start_x = paintable_rect.x();
                    selection.end_x = selection.start_x + selection_rect.width();
                } else if selection.end_x > paintable_rect.width() + paintable_rect.x() {
                    selection.end_x = paintable_rect.width() + paintable_rect.x();
                    selection.start_x = selection.end_x - selection_rect.width();
                }
                if selection.start_y < paintable_rect.y() {
                    selection.start_y = paintable_rect.y();
                    selection.end_y = selection.start_y + selection_rect.height();
                } else if selection.end_y > paintable_rect.height() + paintable_rect.y() {
                    selection.end_y = paintable_rect.height() + paintable_rect.y();
                    selection.start_y = selection.end_y - selection_rect.height();
                }

                self.set_selection(Some(selection));
                self.update_selection_handles();
            }
        }

        if let Some(pointer_position) = imp.pointer_position.get() {
            let cursor_type = self.compute_cursor_type(pointer_position.x(), pointer_position.y());
            self.set_cursor(cursor_type);
        }
    }

    fn set_cursor(&self, cursor_type: CursorType) {
        self.set_cursor_from_name(Some(cursor_type.name()));
    }

    fn compute_cursor_type(&self, x: f32, y: f32) -> CursorType {
        let imp = self.imp();

        let point = Point::new(x, y);

        let Some(selection) = self.selection() else {
            return CursorType::Crosshair;
        };

        let [top_left_handle, top_right_handle, bottom_right_handle, bottom_left_handle] =
            imp.selection_handles.get().unwrap();

        if top_left_handle.contains_point(&point) {
            CursorType::NorthWestResize
        } else if top_right_handle.contains_point(&point) {
            CursorType::NorthEastResize
        } else if bottom_right_handle.contains_point(&point) {
            CursorType::SouthEastResize
        } else if bottom_left_handle.contains_point(&point) {
            CursorType::SouthWestResize
        } else if selection.rect().contains_point(&point) {
            CursorType::Move
        } else if top_left_handle
            .union(&top_right_handle)
            .contains_point(&point)
        {
            CursorType::NorthResize
        } else if top_right_handle
            .union(&bottom_right_handle)
            .contains_point(&point)
        {
            CursorType::EastResize
        } else if bottom_right_handle
            .union(&bottom_left_handle)
            .contains_point(&point)
        {
            CursorType::SouthResize
        } else if bottom_left_handle
            .union(&top_left_handle)
            .contains_point(&point)
        {
            CursorType::WestResize
        } else {
            CursorType::Crosshair
        }
    }

    fn update_selection_handles(&self) {
        let imp = self.imp();

        let Some(selection) = self.selection() else {
            imp.selection_handles.set(None);
            return;
        };

        let selection_handle_diameter = SELECTION_HANDLE_RADIUS * 2.0;
        let top_left = Rect::new(
            selection.left_x() - SELECTION_HANDLE_RADIUS,
            selection.top_y() - SELECTION_HANDLE_RADIUS,
            selection_handle_diameter,
            selection_handle_diameter,
        );
        let top_right = Rect::new(
            selection.right_x() - SELECTION_HANDLE_RADIUS,
            selection.top_y() - SELECTION_HANDLE_RADIUS,
            selection_handle_diameter,
            selection_handle_diameter,
        );
        let bottom_right = Rect::new(
            selection.right_x() - SELECTION_HANDLE_RADIUS,
            selection.bottom_y() - SELECTION_HANDLE_RADIUS,
            selection_handle_diameter,
            selection_handle_diameter,
        );
        let bottom_left = Rect::new(
            selection.left_x() - SELECTION_HANDLE_RADIUS,
            selection.bottom_y() - SELECTION_HANDLE_RADIUS,
            selection_handle_diameter,
            selection_handle_diameter,
        );

        imp.selection_handles
            .set(Some([top_left, top_right, bottom_right, bottom_left]));
    }
}

impl Default for ViewPort {
    fn default() -> Self {
        Self::new()
    }
}
