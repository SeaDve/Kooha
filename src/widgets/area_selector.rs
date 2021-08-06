use adw::subclass::prelude::*;
use gtk::{
    gdk::{self, keys::Key},
    glib::{self, clone, signal::Inhibit, subclass::Signal, GBoxed, SignalHandlerId},
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
        pub start_position: RefCell<Option<Point>>,
        pub current_position: RefCell<Option<Point>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "AreaSelector";
        type Type = super::AreaSelector;
        type ParentType = gtk::Window;

        fn new() -> Self {
            Self {
                start_position: RefCell::new(None),
                current_position: RefCell::new(None),
            }
        }
    }

    impl ObjectImpl for AreaSelector {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.setup_signals();
            obj.set_cursor_from_name(Some("crosshair"));
            obj.remove_css_class("background");
            obj.set_decorated(false);
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
            if let Some(start_position) = *self.start_position.borrow() {
                let current_position = self.current_position.take().unwrap();

                let width = current_position.x - start_position.x;
                let height = current_position.y - start_position.y;

                let selection_rect = graphene::Rect::new(
                    start_position.x as f32,
                    start_position.y as f32,
                    width as f32,
                    height as f32,
                );

                snapshot.append_color(&FILL_COLOR, &selection_rect);
                snapshot.append_border(
                    &gsk::RoundedRect::from_rect(selection_rect, 0.0),
                    &[LINE_WIDTH; 4],
                    &[BORDER_COLOR; 4],
                );
            } else {
                let placeholder_color = gdk::RGBABuilder::new().build();
                let placeholder_rect = graphene::Rect::zero();
                snapshot.append_color(&placeholder_color, &placeholder_rect);
            }
        }

        fn show(&self, obj: &Self::Type) {
            self.parent_show(obj);
            obj.set_raise_request(true);
        }
    }

    impl WindowImpl for AreaSelector {
        fn close_request(&self, obj: &Self::Type) -> Inhibit {
            obj.emit_response(&AreaSelectorResponse::Cancelled);
            Inhibit(false)
        }
    }
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
        imp::AreaSelector::from_instance(self)
    }

    fn setup_signals(&self) {
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        key_controller.connect_key_pressed(
            clone!(@weak self as obj => @default-return Inhibit(false), move |_, keyval, _, _| {
                if keyval == Key::from_name("Escape") {
                    obj.emit_response(&AreaSelectorResponse::Cancelled);
                    Inhibit(true)
                } else {
                    Inhibit(false)
                }
            }),
        );
        self.add_controller(&key_controller);

        let gesture_drag = gtk::GestureDrag::new();
        gesture_drag.set_exclusive(true);
        gesture_drag.connect_drag_begin(clone!(@weak self as obj => move |_, x, y| {
            let imp = obj.private();
            imp.start_position.replace(Some(Point::new(x, y)));
        }));
        gesture_drag.connect_drag_update(clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
            let imp = obj.private();
            if let Some(gesture_start_point) = gesture.start_point() {
                let (start_x, start_y) = gesture_start_point;

                imp.current_position.replace(Some(Point::new(start_x + offset_x, start_y + offset_y)));
                obj.queue_draw();
            }
        }));
        gesture_drag.connect_drag_end(clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
            let imp = obj.private();
            if let Some(gesture_start_point) = gesture.start_point() {
                let (start_x, start_y) = gesture_start_point;

                let start_position = imp.start_position.borrow().unwrap();
                let end_position = Point::new(start_x + offset_x, start_y + offset_y);

                let selection_rectangle = Rectangle::from_points(start_position, end_position);
                let actual_screen = Screen::new(obj.width(), obj.height());

                obj.emit_response(&AreaSelectorResponse::Captured(selection_rectangle, actual_screen));
            }
        }));
        self.add_controller(&gesture_drag);
    }

    fn emit_response(&self, response: &AreaSelectorResponse) {
        self.emit_by_name("response", &[response]).unwrap();
        self.clean();
        self.destroy();
    }

    fn clean(&self) {
        let imp = self.private();
        imp.start_position.replace(None);

        self.queue_draw();
        self.set_raise_request(false);
    }

    fn set_raise_request(&self, is_raised: bool) {
        // Delay is needed to wait for the window to show. Otherwise, it
        // will be too early and it will raise the wrong window.
        let delay = if is_raised { 100 } else { 0 };

        glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            match utils::set_raise_active_window_request(is_raised) {
                Ok(_) => log::info!("Successfully set raise active window to {}", is_raised),
                Err(error) => log::warn!(
                    "Failed to set raise active window to {}: {}",
                    is_raised,
                    error
                ),
            }
        });
    }

    pub fn connect_response<F: Fn(&[glib::Value]) -> Option<glib::Value> + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("response", false, f).unwrap()
    }

    pub fn select_area(&self) {
        self.fullscreen();
        self.present();

        let display = gdk::Display::default().unwrap();
        log::info!("is_display_composited: {}", display.is_composited());
        log::info!("is_display_rgba: {}", display.is_rgba());
    }
}
