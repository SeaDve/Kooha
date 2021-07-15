use adw::subclass::prelude::*;
use gtk::{
    gdk::keys::Key,
    glib::{self, clone, signal::Inhibit, GBoxed},
    prelude::*,
    subclass::prelude::*,
};

use std::{mem, time::Duration};

use crate::backend::Screen;
use crate::backend::Utils;

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

#[derive(Debug, Default, Clone, GBoxed)]
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

    use std::cell::Cell;

    #[derive(Debug, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/area_selector.ui")]
    pub struct KhaAreaSelector {
        pub is_dragging: Cell<bool>,
        pub start_point: Cell<Point>,
        #[template_child]
        pub drawing_area: TemplateChild<gtk::DrawingArea>,
        #[template_child]
        pub key_event_notifier: TemplateChild<gtk::EventControllerKey>,
        #[template_child]
        pub click_event_notifier: TemplateChild<gtk::GestureClick>,
        #[template_child]
        pub motion_event_notifier: TemplateChild<gtk::EventControllerMotion>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaAreaSelector {
        const NAME: &'static str = "KhaAreaSelector";
        type Type = super::KhaAreaSelector;
        type ParentType = gtk::Window;

        fn new() -> Self {
            Self {
                is_dragging: Cell::new(false),
                start_point: Cell::new(Point::new(0_f64, 0_f64)),
                drawing_area: TemplateChild::default(),
                key_event_notifier: TemplateChild::default(),
                click_event_notifier: TemplateChild::default(),
                motion_event_notifier: TemplateChild::default(),
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
            self.drawing_area.set_cursor_from_name(Some("crosshair"));
            obj.clean();
            obj.setup_signals();
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

    impl WidgetImpl for KhaAreaSelector {}
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

    fn setup_signals(&self) {
        let imp = self.private();

        self.connect_close_request(
            clone!(@weak self as win => @default-return Inhibit(false), move |_| {
                win.emit_cancelled();
                Inhibit(false)
            }),
        );

        self.connect_show(clone!(@weak self as win => move |_| {
            win.set_raise_request(true);
        }));

        imp.key_event_notifier.connect_key_pressed(
            clone!(@weak self as win => @default-return Inhibit(false), move |_, keyval, _, _| {
                if keyval == Key::from_name("Escape") {
                    win.emit_cancelled();
                    Inhibit(true)
                } else {
                    Inhibit(false)
                }
            }),
        );

        imp.click_event_notifier
            .connect_pressed(clone!(@weak self as win => move |_, _, x, y| {
                let win_ =  win.private();
                win_.is_dragging.set(true);
                win_.start_point.set(Point::new(x, y));
            }));

        imp.click_event_notifier
            .connect_released(clone!(@weak self as win => move |_, _, x, y| {
                let win_ =  win.private();
                win_.is_dragging.set(false);

                let start_point = win_.start_point.get();
                let end_point = Point::new(x, y);

                let selection_rectangle = Rectangle::from_points(start_point, end_point);
                let actual_screen = Screen::new(win.width(), win.height());
                win.emit_captured(selection_rectangle, actual_screen);
            }));

        imp.motion_event_notifier
            .connect_motion(clone!(@weak self as win => move |_, x, y| {
                let win_ =  win.private();
                let is_dragging = win_.is_dragging.get();

                if !is_dragging {
                    return;
                };

                let start_point = win_.start_point.get();
                let width = x - start_point.x;
                let height = y - start_point.y;

                win.draw(start_point.x, start_point.y, width, height);
            }));
    }

    fn clean(&self) {
        let imp = self.private();
        imp.drawing_area.set_draw_func(move |_, cr, _, _| {
            cr.new_path();
        });
    }

    fn draw(&self, x: f64, y: f64, width: f64, height: f64) {
        let imp = self.private();
        imp.drawing_area.set_draw_func(move |_, cr, _, _| {
            cr.rectangle(x, y, width, height);
            cr.set_source_rgba(0.1, 0.45, 0.8, 0.3);
            cr.fill_preserve().unwrap();
            cr.set_source_rgb(0.1, 0.45, 0.8);
            cr.set_line_width(1_f64);
            cr.stroke().unwrap();
        });
    }

    fn set_raise_request(&self, is_raised: bool) {
        if is_raised {
            glib::timeout_add_local_once(Duration::from_millis(100), move || {
                Utils::set_raise_active_window_request(true)
                    .expect("Failed to raise active window");
            });
        } else {
            Utils::set_raise_active_window_request(false).expect("Failed to unraise active window");
        };
    }

    fn emit_cancelled(&self) {
        self.set_raise_request(false);
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

    pub fn select_area(&self) {
        let imp = self.private();
        imp.is_dragging.set(false);
        self.fullscreen();
        self.present();
    }
}
