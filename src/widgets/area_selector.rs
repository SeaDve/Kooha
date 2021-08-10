use adw::subclass::prelude::*;
use futures::channel::oneshot::{Canceled, Receiver, Sender};
use gtk::{
    gdk::{self, keys::Key},
    glib::{self, clone, signal::Inhibit},
    graphene, gsk,
    prelude::*,
    subclass::prelude::*,
};

use std::{cell::RefCell, mem, time::Duration};

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

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct AreaSelector {
        pub sender: RefCell<Option<Sender<(Rectangle, Screen)>>>,
        pub receiver: RefCell<Option<Receiver<(Rectangle, Screen)>>>,
        pub start_position: RefCell<Option<Point>>,
        pub current_position: RefCell<Option<Point>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "AreaSelector";
        type Type = super::AreaSelector;
        type ParentType = gtk::Window;

        fn new() -> Self {
            let (sender, receiver) = futures::channel::oneshot::channel();

            Self {
                sender: RefCell::new(Some(sender)),
                receiver: RefCell::new(Some(receiver)),
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
            obj.fullscreen();
        }
    }

    impl WidgetImpl for AreaSelector {
        fn snapshot(&self, _widget: &Self::Type, snapshot: &gtk::Snapshot) {
            if let Some(ref start_position) = *self.start_position.borrow() {
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
            if let Some(sender) = self.sender.take() {
                mem::drop(sender);
            }

            obj.set_raise_request(false);
            obj.destroy();
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
        glib::Object::new(&[]).expect("Failed to create AreaSelector.")
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
                    obj.close();
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

            let start_position = Point::new(x, y);
            imp.start_position.replace(Some(start_position));
        }));
        gesture_drag.connect_drag_update(
            clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
                if let Some((start_x, start_y)) = gesture.start_point() {
                    let imp = obj.private();

                    let current_position = Point::new(start_x + offset_x, start_y + offset_y);
                    imp.current_position.replace(Some(current_position));
                    obj.queue_draw();
                }
            }),
        );
        gesture_drag.connect_drag_end(clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
            if let Some((start_x, start_y)) = gesture.start_point() {
                let imp = obj.private();

                let start_position = imp.start_position.take().unwrap();
                let end_position = Point::new(start_x + offset_x, start_y + offset_y);

                let selection_rectangle = Rectangle::from_points(&start_position, &end_position);
                let actual_screen = Screen::new(obj.width(), obj.height());

                imp.sender.take().unwrap().send((selection_rectangle, actual_screen)).unwrap();
                obj.close();
            }
        }));
        self.add_controller(&gesture_drag);
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

    pub async fn select_area(&self) -> Result<(Rectangle, Screen), Canceled> {
        let imp = self.private();

        self.present();

        imp.receiver.take().unwrap().await
    }
}
