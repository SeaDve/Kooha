// Based on gnome-shell's screenshot ui (GPLv2).
// Source: https://gitlab.gnome.org/GNOME/gnome-shell/-/blob/a3c84ca7463ed92b5be6f013a12bce927223f7c5/js/ui/screenshot.js

use gtk::{
    gdk,
    glib::{self, clone},
    graphene::{Point, Rect},
    gsk::{self, RoundedRect},
    prelude::*,
    subclass::prelude::*,
};

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    fmt,
};

// TODO
// * Add animation when entering/leaving selection mode.
// * Add undo and redo.
// * Add minimum selection size.

const SIZE: f64 = 100.0;

const DEFAULT_SELECTION_SIZE: f32 = 40.0;

const SHADE_COLOR: gdk::RGBA = gdk::RGBA::BLACK.with_alpha(0.5);

const SELECTION_LINE_WIDTH: f32 = 2.0;
const SELECTION_LINE_COLOR: gdk::RGBA = gdk::RGBA::WHITE.with_alpha(0.6);

const SELECTION_HANDLE_COLOR: gdk::RGBA = gdk::RGBA::WHITE;
const SELECTION_HANDLE_SHADOW_COLOR: gdk::RGBA = gdk::RGBA::BLACK.with_alpha(0.2);
const SELECTION_HANDLE_RADIUS: f32 = 12.0;

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
    fn as_str(self) -> &'static str {
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

#[derive(Clone, Copy, PartialEq, glib::Boxed)]
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
    fn from_rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            start_x: x,
            start_y: y,
            end_x: x + width,
            end_y: y + height,
        }
    }

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

impl StaticVariantType for Selection {
    fn static_variant_type() -> Cow<'static, glib::VariantTy> {
        <(f64, f64, f64, f64)>::static_variant_type()
    }
}

impl FromVariant for Selection {
    fn from_variant(variant: &glib::Variant) -> Option<Self> {
        let (start_x, start_y, end_x, end_y) = variant.get::<(f64, f64, f64, f64)>()?;

        Some(Self {
            start_x: start_x as f32,
            start_y: start_y as f32,
            end_x: end_x as f32,
            end_y: end_y as f32,
        })
    }
}

impl ToVariant for Selection {
    fn to_variant(&self) -> glib::Variant {
        (
            self.start_x as f64,
            self.start_y as f64,
            self.end_x as f64,
            self.end_y as f64,
        )
            .to_variant()
    }
}

impl From<Selection> for glib::Variant {
    fn from(selection: Selection) -> glib::Variant {
        selection.to_variant()
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/view_port.ui")]
    #[properties(wrapper_type = super::ViewPort)]
    pub struct ViewPort {
        #[property(get, set = Self::set_paintable, explicit_notify, nullable)]
        pub(super) paintable: RefCell<Option<gdk::Paintable>>,
        #[property(get, set = Self::set_selection, explicit_notify, nullable)]
        pub(super) selection: Cell<Option<Selection>>,
        #[property(get, nullable)]
        pub(super) paintable_rect: Cell<Option<Rect>>,

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

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ViewPort {}

    impl WidgetImpl for ViewPort {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if for_size == 0 {
                return (0, 0, -1, -1);
            }

            let Some(paintable) = self.obj().paintable() else {
                return (0, 0, -1, -1);
            };

            if orientation == gtk::Orientation::Horizontal {
                let (natural_width, _) = paintable.compute_concrete_size(
                    0.0,
                    if for_size < 0 { 0.0 } else { for_size as f64 },
                    SIZE,
                    SIZE,
                );
                (0, natural_width.ceil() as i32, -1, -1)
            } else {
                let (_, natural_height) = paintable.compute_concrete_size(
                    if for_size < 0 { 0.0 } else { for_size as f64 },
                    0.0,
                    SIZE,
                    SIZE,
                );
                (0, natural_height.ceil() as i32, -1, -1)
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();

            let Some(paintable) = obj.paintable() else {
                self.paintable_rect.set(None);
                obj.notify_paintable_rect();
                return;
            };

            let width = width as f64;
            let height = height as f64;
            let ratio = width / height;

            let paintable_width = paintable.intrinsic_width() as f64;
            let paintable_height = paintable.intrinsic_height() as f64;
            let paintable_ratio = paintable.intrinsic_aspect_ratio();

            let (rel_paintable_width, rel_paintable_height) =
                if width >= paintable_width && height >= paintable_height {
                    (paintable_width, paintable_height)
                } else if paintable_ratio > ratio {
                    (width, width / paintable_ratio)
                } else {
                    (height * paintable_ratio, height)
                };

            let new_paintable_rect = Rect::new(
                ((width - rel_paintable_width) / 2.0).floor() as f32,
                ((height - rel_paintable_height) / 2.0).floor() as f32,
                rel_paintable_width.ceil() as f32,
                rel_paintable_height.ceil() as f32,
            );
            let prev_paintable_rect = self.paintable_rect.replace(Some(new_paintable_rect));
            obj.notify_paintable_rect();

            // Update selection if paintable rect changed.
            if let Some(prev_paintable_rect) = prev_paintable_rect {
                if let Some(selection) = obj.selection() {
                    let selection_rect = selection.rect();

                    let scale_x = new_paintable_rect.width() / prev_paintable_rect.width();
                    let scale_y = new_paintable_rect.height() / prev_paintable_rect.height();

                    let rel_x = selection_rect.x() - prev_paintable_rect.x();
                    let rel_y = selection_rect.y() - prev_paintable_rect.y();

                    obj.set_selection(Some(Selection::from_rect(
                        new_paintable_rect.x() + rel_x * scale_x,
                        new_paintable_rect.y() + rel_y * scale_y,
                        selection_rect.width() * scale_x,
                        selection_rect.height() * scale_y,
                    )));
                }
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();

            if let Some(paintable_rect) = obj.paintable_rect() {
                snapshot.save();

                snapshot.translate(&Point::new(paintable_rect.x(), paintable_rect.y()));

                let paintable = obj.paintable().unwrap();
                paintable.snapshot(
                    snapshot,
                    paintable_rect.width() as f64,
                    paintable_rect.height() as f64,
                );

                snapshot.restore();
            }

            if let Some(selection) = obj.selection() {
                let selection_rect = selection.rect().round_extents();

                // Shades the area outside the selection.
                if let Some(paintable_rect) = obj.paintable_rect() {
                    snapshot.push_mask(gsk::MaskMode::InvertedAlpha);

                    snapshot.append_color(&gdk::RGBA::BLACK, &selection_rect.inset_r(3.0, 3.0));
                    snapshot.pop();

                    snapshot.append_color(&SHADE_COLOR, &paintable_rect);
                    snapshot.pop();
                }

                let path_builder = gsk::PathBuilder::new();
                path_builder.add_rect(&selection_rect);
                snapshot.append_stroke(
                    &path_builder.to_path(),
                    &gsk::Stroke::builder(SELECTION_LINE_WIDTH)
                        .dash(&[10.0, 6.0])
                        .build(),
                    &SELECTION_LINE_COLOR,
                );

                for handle in self.selection_handles.get().unwrap() {
                    let bounds = RoundedRect::from_rect(handle, SELECTION_HANDLE_RADIUS);
                    snapshot.append_outset_shadow(
                        &bounds,
                        &SELECTION_HANDLE_SHADOW_COLOR,
                        0.0,
                        1.0,
                        2.0,
                        3.0,
                    );

                    snapshot.push_rounded_clip(&bounds);
                    snapshot.append_color(&SELECTION_HANDLE_COLOR, &handle);
                    snapshot.pop();
                }
            }
        }
    }

    impl ViewPort {
        fn set_paintable(&self, paintable: Option<gdk::Paintable>) {
            let obj = self.obj();

            if paintable == obj.paintable() {
                return;
            }

            let mut handler_ids = self.handler_ids.borrow_mut();

            if let Some(previous_paintable) = self.paintable.replace(paintable.clone()) {
                for handler_id in handler_ids.drain(..) {
                    previous_paintable.disconnect(handler_id);
                }
            }

            if let Some(paintable) = paintable {
                handler_ids.push(paintable.connect_invalidate_contents(
                    clone!(@weak obj => move |_| {
                        obj.queue_draw();
                    }),
                ));
                handler_ids.push(
                    paintable.connect_invalidate_size(clone!(@weak obj => move |_| {
                        obj.queue_resize();
                    })),
                );
            }

            self.paintable_rect.set(None);

            obj.queue_resize();
            obj.notify_paintable_rect();
            obj.notify_paintable();
        }

        fn set_selection(&self, selection: Option<Selection>) {
            let obj = self.obj();

            if selection == obj.selection() {
                return;
            }

            self.selection.set(selection);
            obj.update_selection_handles();
            obj.queue_draw();
            obj.notify_selection();
        }
    }
}

glib::wrapper! {
     pub struct ViewPort(ObjectSubclass<imp::ViewPort>)
        @extends gtk::Widget;
}

impl ViewPort {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn set_cursor(&self, cursor_type: CursorType) {
        if self
            .cursor()
            .and_then(|cursor| cursor.name())
            .is_some_and(|name| name == cursor_type.as_str())
        {
            return;
        }

        self.set_cursor_from_name(Some(cursor_type.as_str()));
    }

    fn compute_cursor_type(&self, x: f32, y: f32) -> CursorType {
        let imp = self.imp();

        let point = Point::new(x, y);

        if self.paintable().is_none() {
            return CursorType::Default;
        };

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

        imp.selection_handles.set(Some([
            top_left.round_extents(),
            top_right.round_extents(),
            bottom_right.round_extents(),
            bottom_left.round_extents(),
        ]));
    }
}

#[gtk::template_callbacks]
impl ViewPort {
    #[template_callback]
    fn enter(&self, x: f64, y: f64) {
        let imp = self.imp();

        imp.pointer_position
            .set(Some(Point::new(x as f32, y as f32)));
    }

    #[template_callback]
    fn motion(&self, x: f64, y: f64) {
        let imp = self.imp();

        imp.pointer_position
            .set(Some(Point::new(x as f32, y as f32)));

        if imp.drag_start.get().is_none() {
            let cursor_type = self.compute_cursor_type(x as f32, y as f32);
            self.set_cursor(cursor_type);
        }
    }

    #[template_callback]
    fn leave(&self) {
        let imp = self.imp();

        imp.pointer_position.set(None);

        self.set_cursor(CursorType::Default);
    }

    #[template_callback]
    fn drag_begin(&self, x: f64, y: f64) {
        tracing::trace!("Drag begin at ({}, {})", x, y);

        let Some(paintable_rect) = self.paintable_rect() else {
            return;
        };

        let imp = self.imp();
        let cursor_type = self.compute_cursor_type(x as f32, y as f32);

        if cursor_type == CursorType::Crosshair {
            imp.drag_cursor.set(CursorType::Crosshair);
            self.set_cursor(CursorType::Crosshair);

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
        } else {
            imp.drag_cursor.set(cursor_type);
            imp.drag_start.set(Some(Point::new(x as f32, y as f32)));

            let selection = self.selection().unwrap();
            let mut new_selection = selection;

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
    }

    #[template_callback]
    fn drag_update(&self, _dx: f64, _dy: f64) {
        let Some(paintable_rect) = self.paintable_rect() else {
            return;
        };

        let imp = self.imp();

        let pointer_position = imp.pointer_position.get().unwrap();

        let drag_cursor = imp.drag_cursor.get();

        if drag_cursor == CursorType::Crosshair {
            let Selection {
                start_x, start_y, ..
            } = self.selection().unwrap();
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

                let selection_rect = self.selection().unwrap().rect();

                // Keep the size intact if we bumped to the paintable rect.
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
    }

    #[template_callback]
    fn drag_end(&self, dx: f64, dy: f64) {
        tracing::trace!("Drag end offset ({}, {})", dx, dy);

        let Some(paintable_rect) = self.paintable_rect() else {
            return;
        };

        let imp = self.imp();
        imp.drag_start.set(None);

        // The user clicked without dragging. Make up a larger selection
        // to reduce confusion.
        if let Some(mut selection) = self.selection() {
            if imp.drag_cursor.get() == CursorType::Crosshair
                && selection.end_x == selection.start_x
                && selection.end_y == selection.start_y
            {
                let offset = DEFAULT_SELECTION_SIZE / 2.0;
                selection.start_x -= offset;
                selection.start_y -= offset;
                selection.end_x += offset;
                selection.end_y += offset;

                let selection_rect = selection.rect();

                // Keep the coordinates inside the paintable rect.
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
            }
        }

        if let Some(pointer_position) = imp.pointer_position.get() {
            let cursor_type = self.compute_cursor_type(pointer_position.x(), pointer_position.y());
            self.set_cursor(cursor_type);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_variant() {
        let original = Selection::from_rect(1.0, 2.0, 3.0, 4.0);
        let converted = original.to_variant().get::<Selection>().unwrap();
        assert_eq!(original, converted);
    }
}
