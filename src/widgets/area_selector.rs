use adw::subclass::prelude::*;
use gtk::{
    gdk::{self, keys::Key},
    glib::{self, clone, signal::Inhibit, subclass::Signal, GBoxed},
    graphene, gsk,
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::{cell::RefCell, time::Duration};

use crate::{
    data_types::{Point, Rectangle, Screen},
    utils,
};

const LINE_WIDTH: f32 = 1.0;
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

#[derive(Debug, Clone, GBoxed)]
#[gboxed(type_name = "AreaSelectorResponse")]
pub enum AreaSelectorResponse {
    Captured(Rectangle, Screen),
    Cancelled,
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct AreaSelector {
        pub start_point: RefCell<Option<Point>>,
        pub current_point: RefCell<Option<Point>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "AreaSelector";
        type Type = super::AreaSelector;
        type ParentType = gtk::Window;

        fn new() -> Self {
            Self {
                start_point: RefCell::new(None),
                current_point: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for AreaSelector {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            obj.set_cursor_from_name(Some("crosshair"));
            obj.set_decorated(false);
            obj.set_css_classes(&[]);

            obj.connect_close_request(
                clone!(@weak obj => @default-return Inhibit(false), move |_| {
                    obj.emit_response(AreaSelectorResponse::Cancelled);
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
                        obj.emit_response(AreaSelectorResponse::Cancelled);
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

                    obj.emit_response(AreaSelectorResponse::Captured(selection_rectangle, actual_screen));
                }
            }));
            obj.add_controller(&gesture_drag);
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder(
                    "response",
                    &[AreaSelectorResponse::static_type().into()],
                    <()>::static_type().into(),
                )
                .build()]
            });
            SIGNALS.as_ref()
        }
    }

    impl WidgetImpl for AreaSelector {
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
                    &[LINE_WIDTH; 4],
                    &[BORDER_COLOR; 4],
                );
            }
        }
    }
    impl WindowImpl for AreaSelector {}
}

glib::wrapper! {
    pub struct AreaSelector(ObjectSubclass<imp::AreaSelector>)
        @extends gtk::Widget, gtk::Window;
}

impl AreaSelector {
    pub fn new() -> Self {
        glib::Object::new(&[]).expect("Failed to create AreaSelector")
    }

    fn private(&self) -> &imp::AreaSelector {
        &imp::AreaSelector::from_instance(self)
    }

    fn set_raise_request(&self, is_raised: bool) {
        // Delay is needed to wait for the window to show. Otherwise, it
        // will be too early and it will raise the wrong window.
        let delay = if is_raised { 100 } else { 0 };

        glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            match utils::set_raise_active_window_request(is_raised) {
                Ok(_) => log::info!("Successfully set raise active window to {}", is_raised),
                Err(error) => log::warn!("{}", error),
            }
        });
    }

    fn emit_response(&self, response: AreaSelectorResponse) {
        self.emit_by_name("response", &[&response]).unwrap();
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
