use adw::subclass::prelude::*;
use error_stack::{Context, IntoReport, Report, Result, ResultExt};
use futures_channel::oneshot::{self, Sender};
use gtk::{
    gdk, gio,
    glib::{self, clone, signal::Inhibit},
    graphene::{Point, Rect, Size},
    gsk,
    prelude::*,
};

use std::{cell::RefCell, fmt, time::Duration};

const LINE_WIDTH: f32 = 1.0;

#[derive(Debug, Clone, Copy)]
#[must_use]
pub enum Response {
    Ok { selection: Rect, screen: Size },
    Cancelled,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct AreaSelector {
        pub(super) sender: RefCell<Option<Sender<Response>>>,
        pub(super) start_position: RefCell<Option<Point>>,
        pub(super) current_position: RefCell<Option<Point>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "KoohaAreaSelector";
        type Type = super::AreaSelector;
        type ParentType = gtk::Window;
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
        fn snapshot(&self, obj: &Self::Type, snapshot: &gtk::Snapshot) {
            obj.on_snapshot(snapshot);
        }
    }

    impl WindowImpl for AreaSelector {
        fn close_request(&self, obj: &Self::Type) -> Inhibit {
            if let Some(sender) = self.sender.take() {
                let _ = sender.send(Response::Cancelled);
            }

            self.parent_close_request(obj)
        }
    }
}

glib::wrapper! {
    pub struct AreaSelector(ObjectSubclass<imp::AreaSelector>)
        @extends gtk::Widget, gtk::Window;
}

impl AreaSelector {
    pub async fn select_area() -> Response {
        let this: AreaSelector = glib::Object::new(&[]).expect("Failed to create AreaSelector.");
        let (sender, receiver) = oneshot::channel();
        this.imp().sender.replace(Some(sender));

        this.present();

        // Delay is needed to wait for the window to show. Otherwise, it
        // will be too early and it will raise the wrong window.
        glib::timeout_future(Duration::from_millis(100)).await;
        set_raise_active_window_request(true).await;

        let res = receiver.await.unwrap();

        set_raise_active_window_request(false).await;

        res
    }

    fn on_snapshot(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();

        if let Some(ref start_position) = *imp.start_position.borrow() {
            let current_position = imp.current_position.take().unwrap();

            let width = current_position.x() - start_position.x();
            let height = current_position.y() - start_position.y();

            let selection_rect = Rect::new(
                start_position.x() as f32,
                start_position.y() as f32,
                width as f32,
                height as f32,
            );

            let border_color = gdk::RGBA::builder()
                .red(0.1)
                .green(0.45)
                .blue(0.8)
                .alpha(1.0)
                .build();
            let fill_color = gdk::RGBA::builder()
                .red(0.1)
                .green(0.45)
                .blue(0.8)
                .alpha(0.3)
                .build();

            snapshot.append_color(&fill_color, &selection_rect);
            snapshot.append_border(
                &gsk::RoundedRect::from_rect(selection_rect, 0.0),
                &[LINE_WIDTH; 4],
                &[border_color; 4],
            );
        } else {
            let placeholder_color = gdk::RGBA::builder().build();
            let placeholder_rect = Rect::zero();
            snapshot.append_color(&placeholder_color, &placeholder_rect);
        }
    }

    fn setup_signals(&self) {
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        key_controller.connect_key_pressed(
            clone!(@weak self as obj => @default-return Inhibit(false), move |_, keyval, _, _| {
                if keyval == gdk::Key::Escape {
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
            let start_position = Point::new(x as f32, y as f32);
            obj.imp().start_position.replace(Some(start_position));
        }));
        gesture_drag.connect_drag_update(
            clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
                if let Some((start_x, start_y)) = gesture.start_point() {
                    let current_position = Point::new((start_x + offset_x) as f32, (start_y + offset_y) as f32);
                    obj.imp().current_position.replace(Some(current_position));
                    obj.queue_draw();
                }
            }),
        );
        gesture_drag.connect_drag_end(
            clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
                if let Some((start_x, start_y)) = gesture.start_point() {
                    let imp = obj.imp();

                    let start_position = imp.start_position.take().unwrap();
                    let end_position = Point::new((start_x + offset_x) as f32, (start_y + offset_y) as f32);

                    let selection = rect_from_points(start_position, end_position);
                    let screen = Size::new(obj.width() as f32, obj.height() as f32);

                    imp.sender
                        .take()
                        .unwrap()
                        .send(Response::Ok { selection, screen })
                        .unwrap();
                    obj.close();
                }
            }),
        );
        self.add_controller(&gesture_drag);
    }
}

async fn set_raise_active_window_request(is_raised: bool) {
    async fn inner(is_raised: bool) -> Result<(), ShellWindowEvalError> {
        shell_window_eval("make_above", is_raised)
            .await
            .attach_printable("Failed to invoke `make_above` method")?;
        shell_window_eval("stick", is_raised)
            .await
            .attach_printable("Failed to invoke `stick` method")?;
        Ok(())
    }

    match inner(is_raised).await {
        Ok(_) => tracing::info!("Successfully set raise active window to {}", is_raised,),
        Err(error) => tracing::warn!(
            "Failed to set raise active window to {}: {:?}",
            is_raised,
            error
        ),
    }
}

#[derive(Debug)]
pub struct ShellWindowEvalError;

impl fmt::Display for ShellWindowEvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Evaluating shell window command error")
    }
}

impl Context for ShellWindowEvalError {}

async fn shell_window_eval(method: &str, is_enabled: bool) -> Result<(), ShellWindowEvalError> {
    let reverse_keyword = if is_enabled { "" } else { "un" };
    let script = format!(
        "global.display.focus_window.{}{}()",
        reverse_keyword, method
    );

    let connection = gio::bus_get_future(gio::BusType::Session)
        .await
        .report()
        .change_context(ShellWindowEvalError)
        .attach_printable("Failed to get session bus connection")?;
    let reply = connection
        .call_future(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell",
            "org.gnome.Shell",
            "Eval",
            Some(&(&script,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
        )
        .await
        .report()
        .change_context(ShellWindowEvalError)
        .attach_printable_lazy(|| format!("Failed to call shell eval with script `{}`", &script))?;
    let (is_success, message) = reply.get::<(bool, String)>().ok_or_else(|| {
        Report::new(ShellWindowEvalError).attach_printable("Expected (bool, String) type reply")
    })?;

    if !is_success {
        return Err(Report::new(ShellWindowEvalError).attach_printable(format!(
            "Shell replied with no success. Got a message of {}",
            message
        )));
    };

    Ok(())
}

/// Create a [`Rect`] from two [`Point`]s.
///
/// If two points are equal, the `x` and `y` are set
/// to 0 and the width and height respectively will
/// be the `x` and `y`.
fn rect_from_points(a: Point, b: Point) -> Rect {
    use std::mem;

    let a_x = a.x();
    let a_y = a.y();
    let b_x = b.x();
    let b_y = b.y();

    let mut x = a_x.min(b_x);
    let mut y = a_y.min(b_y);

    let mut w = (a_x - b_x).abs();
    let mut h = (a_y - b_y).abs();

    if w == 0.0 && h == 0.0 {
        mem::swap(&mut w, &mut x);
        mem::swap(&mut h, &mut y);
    }

    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_from_points_zero() {
        let rec = rect_from_points(Point::zero(), Point::zero());
        assert_eq!(rec.x(), 0.0);
        assert_eq!(rec.y(), 0.0);
        assert_eq!(rec.width(), 0.0);
        assert_eq!(rec.height(), 0.0);
    }

    #[test]
    fn rect_from_points_other() {
        let rect = rect_from_points(Point::new(10.0, 20.0), Point::new(30.0, 40.0));
        assert_eq!(rect.x(), 10.0);
        assert_eq!(rect.y(), 20.0);
        assert_eq!(rect.width(), 20.0);
        assert_eq!(rect.height(), 20.0);
    }

    #[test]
    fn rect_from_points_other_1() {
        let rect = rect_from_points(Point::new(20.0, 40.0), Point::new(10.0, 30.0));
        assert_eq!(rect.x(), 10.0);
        assert_eq!(rect.y(), 30.0);
        assert_eq!(rect.width(), 10.0);
        assert_eq!(rect.height(), 10.0);
    }

    #[test]
    fn rect_from_points_commutative() {
        let rect_a = rect_from_points(Point::new(10.0, 30.0), Point::new(20.0, 40.0));
        let rect_b = rect_from_points(Point::new(20.0, 40.0), Point::new(10.0, 30.0));
        assert_eq!(rect_a, rect_b);
    }

    #[test]
    fn rect_from_equal_points() {
        let rect = rect_from_points(Point::new(10.0, 10.0), Point::new(10.0, 10.0));
        assert_eq!(rect.x(), 0.0);
        assert_eq!(rect.y(), 0.0);
        assert_eq!(rect.width(), 10.0);
        assert_eq!(rect.height(), 10.0);
    }
}
