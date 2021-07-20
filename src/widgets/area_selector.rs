use adw::subclass::prelude::*;
use gtk::{
    gdk::{self, keys::Key},
    glib::{self, clone, signal::Inhibit, GBoxed},
    graphene, gsk,
    prelude::*,
    subclass::prelude::*,
};

use std::{mem, time::Duration};

use crate::backend::Screen;
use crate::backend::Utils;

const BORDER_COLOR: gdk::RGBA = gdk::RGBA {
    red: 0.1,
    green: 0.45,
    blue: 0.8,
    alpha: 1.0,
};
const FILL_COLOR: gdk::RGBA = gdk::RGBA {
    red: 0.1,
    green: 0.45,
    blue: 0.8,
    alpha: 0.3,
};

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "Rectangle")]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rectangle {
    pub fn from_points(point_1: Point, point_2: Point) -> Self {
        let mut x = point_1.x.min(point_2.x);
        let mut y = point_1.y.min(point_2.y);
        let mut width = (point_1.x - point_2.x).abs();
        let mut height = (point_1.y - point_2.y).abs();

        if width == 0.0 && height == 0.0 {
            mem::swap(&mut width, &mut x);
            mem::swap(&mut height, &mut y);
        }

        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn as_rescaled_tuple(&self, scale_factor: f64) -> (f64, f64, f64, f64) {
        (
            self.x * scale_factor,
            self.y * scale_factor,
            self.width * scale_factor,
            self.height * scale_factor,
        )
    }
}

mod imp {
    use super::*;

    use glib::subclass::Signal;
    use gtk::CompositeTemplate;
    use once_cell::sync::Lazy;

    use std::cell::RefCell;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/area_selector.ui")]
    pub struct KhaAreaSelector {
        pub start_point: RefCell<Option<Point>>,
        pub current_point: RefCell<Option<Point>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaAreaSelector {
        const NAME: &'static str = "KhaAreaSelector";
        type Type = super::KhaAreaSelector;
        type ParentType = gtk::Window;

        fn new() -> Self {
            Self {
                start_point: RefCell::new(None),
                current_point: RefCell::new(None),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for KhaAreaSelector {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.set_cursor_from_name(Some("crosshair"));
            obj.connect_close_request(
                clone!(@weak obj => @default-return Inhibit(false), move |_| {
                    obj.emit_cancelled();
                    Inhibit(false)
                }),
            );
            obj.connect_show(clone!(@weak obj => move |_| {
                obj.set_raise_request(true);
            }));

            let key_controller = gtk::EventControllerKey::new();
            key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
            key_controller.connect_key_pressed(
                clone!(@weak obj => @default-return Inhibit(false), move |_, keyval, _, _| {
                    if keyval == Key::from_name("Escape") {
                        obj.emit_cancelled();
                        Inhibit(true)
                    } else {
                        Inhibit(false)
                    }
                }),
            );
            obj.add_controller(&key_controller);

            let gesture_drag = gtk::GestureDrag::new();
            gesture_drag.set_exclusive(true);
            gesture_drag.connect_drag_begin(clone!(@weak obj => move |_, x, y| {
                let imp = obj.private();
                imp.start_point.replace(Some(Point::new(x, y)));
            }));
            gesture_drag.connect_drag_update(clone!(@weak obj => move |gesture, offset_x, offset_y| {
                let imp = obj.private();
                if let Some(start_point) = gesture.start_point() {
                    let (start_x, start_y) = start_point;
                    imp.current_point.replace(Some(Point::new(start_x + offset_x, start_y + offset_y)));
                    obj.queue_draw();
                }
            }));
            gesture_drag.connect_drag_end(clone!(@weak obj => move |gesture, offset_x, offset_y| {
                let imp = obj.private();
                if let Some(start_point) = gesture.start_point() {
                    let (start_x, start_y) = start_point;

                    let start_point = imp.start_point.borrow().unwrap();
                    let end_point = Point::new(start_x + offset_x, start_y + offset_y);

                    let selection_rectangle = Rectangle::from_points(start_point, end_point);
                    let actual_screen = Screen::new(obj.width(), obj.height());

                    obj.emit_captured(selection_rectangle, actual_screen);
                }
            }));
            obj.add_controller(&gesture_drag);
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder(
                        "captured",
                        &[
                            Rectangle::static_type().into(),
                            Screen::static_type().into(),
                        ],
                        <()>::static_type().into(),
                    )
                    .build(),
                    Signal::builder("cancelled", &[], <()>::static_type().into()).build(),
                ]
            });
            SIGNALS.as_ref()
        }
    }

    impl WidgetImpl for KhaAreaSelector {
        fn snapshot(&self, _widget: &Self::Type, snapshot: &gtk::Snapshot) {
            if self.start_point.borrow().is_none() {
                let placeholder_color = gdk::RGBABuilder::new().build();
                let placeholder_rect = graphene::Rect::zero();
                snapshot.append_color(&placeholder_color, &placeholder_rect);
            } else {
                let start_point = self.start_point.borrow().unwrap();
                let current_point = self.current_point.borrow().unwrap();

                let width = current_point.x - start_point.x;
                let height = current_point.y - start_point.y;

                let selection_rect = graphene::Rect::new(
                    start_point.x as f32,
                    start_point.y as f32,
                    width as f32,
                    height as f32,
                );

                snapshot.append_color(&FILL_COLOR, &selection_rect);
                snapshot.append_border(
                    &gsk::RoundedRect::from_rect(selection_rect, 0.0),
                    &[1.0, 1.0, 1.0, 1.0],
                    &[BORDER_COLOR; 4],
                );
            }
        }
    }
    impl WindowImpl for KhaAreaSelector {}
}

glib::wrapper! {
    pub struct KhaAreaSelector(ObjectSubclass<imp::KhaAreaSelector>)
        @extends gtk::Widget, gtk::Window;
}

impl KhaAreaSelector {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create KhaAreaSelector")
    }

    fn private(&self) -> &imp::KhaAreaSelector {
        &imp::KhaAreaSelector::from_instance(self)
    }

    fn set_raise_request(&self, is_raised: bool) {
        // Delay is needed to wait for the window to show. Otherwise, it
        // will be too early and it will raise the wrong window.
        let delay = if is_raised { 100 } else { 0 };

        glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            match Utils::set_raise_active_window_request(is_raised) {
                Ok(_) => log::info!("Sucessfully set raise active window to {}", is_raised),
                Err(error) => log::warn!("{}", error),
            }
        });
    }

    fn emit_cancelled(&self) {
        self.emit_by_name("cancelled", &[]).unwrap();
        self.clean();
        self.hide();
    }

    fn emit_captured(&self, selection_rectangle: Rectangle, actual_screen: Screen) {
        self.emit_by_name("captured", &[&selection_rectangle, &actual_screen])
            .unwrap();
        self.clean();
        self.hide();
    }

    fn clean(&self) {
        let imp = self.private();

        imp.start_point.replace(None);
        imp.current_point.replace(None);
        self.queue_draw();
        self.set_raise_request(false);
    }

    pub fn select_area(&self) {
        self.fullscreen();
        self.present();
    }
}
