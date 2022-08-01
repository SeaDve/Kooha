use adw::subclass::prelude::*;
use ashpd::zbus;
use futures_channel::oneshot::{self, Sender};
use gtk::{
    gdk,
    glib::{self, clone, signal::Inhibit},
    graphene, gsk,
    prelude::*,
};

use std::{cell::RefCell, time::Duration};

use crate::{
    cancelled::Cancelled,
    data_types::{Point, Rectangle, Screen},
};

const LINE_WIDTH: f32 = 1.0;

pub type AreaSelectorResponse = Result<(Rectangle, Screen), Cancelled>;

pub async fn select_area() -> AreaSelectorResponse {
    let selector: AreaSelector = glib::Object::new(&[]).expect("Failed to create AreaSelector.");
    selector.present();

    // Delay is needed to wait for the window to show. Otherwise, it
    // will be too early and it will raise the wrong window.
    glib::timeout_future(Duration::from_millis(100)).await;
    set_raise_active_window_request(true).await;

    let res = selector.wait_response().await;

    set_raise_active_window_request(false).await;

    res
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub(super) struct AreaSelector {
        pub(super) sender: RefCell<Option<Sender<AreaSelectorResponse>>>,
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
                let response = Err(Cancelled::new("Cancelled area selection"));
                sender.send(response).unwrap();
            }

            self.parent_close_request(obj)
        }
    }
}

glib::wrapper! {
    struct AreaSelector(ObjectSubclass<imp::AreaSelector>)
        @extends gtk::Widget, gtk::Window;
}

impl AreaSelector {
    async fn wait_response(&self) -> AreaSelectorResponse {
        let (sender, receiver) = oneshot::channel();
        self.imp().sender.replace(Some(sender));

        receiver.await.unwrap()
    }

    fn on_snapshot(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();

        if let Some(ref start_position) = *imp.start_position.borrow() {
            let current_position = imp.current_position.take().unwrap();

            let width = current_position.x - start_position.x;
            let height = current_position.y - start_position.y;

            let selection_rect = graphene::Rect::new(
                start_position.x as f32,
                start_position.y as f32,
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
            let placeholder_rect = graphene::Rect::zero();
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
            let start_position = Point::new(x, y);
            obj.imp().start_position.replace(Some(start_position));
        }));
        gesture_drag.connect_drag_update(
            clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
                if let Some((start_x, start_y)) = gesture.start_point() {
                    let current_position = Point::new(start_x + offset_x, start_y + offset_y);
                    obj.imp().current_position.replace(Some(current_position));
                    obj.queue_draw();
                }
            }),
        );
        gesture_drag.connect_drag_end(clone!(@weak self as obj => move |gesture, offset_x, offset_y| {
            if let Some((start_x, start_y)) = gesture.start_point() {
                let imp = obj.imp();

                let start_position = imp.start_position.take().unwrap();
                let end_position = Point::new(start_x + offset_x, start_y + offset_y);

                let selection_rectangle = Rectangle::from_points(&start_position, &end_position);
                let actual_screen = Screen::new(obj.width(), obj.height());

                let response = Ok((selection_rectangle, actual_screen));
                imp.sender.take().unwrap().send(response).unwrap();
                obj.close();
            }
        }));
        self.add_controller(&gesture_drag);
    }
}

async fn set_raise_active_window_request(is_raised: bool) {
    async fn inner(is_raised: bool) -> anyhow::Result<()> {
        shell_window_eval("make_above", is_raised).await?;
        shell_window_eval("stick", is_raised).await?;
        Ok(())
    }

    match inner(is_raised).await {
        Ok(_) => tracing::info!("Successfully set raise active window to {}", is_raised),
        Err(error) => tracing::warn!(
            "Failed to set raise active window to {}: {}",
            is_raised,
            error
        ),
    }
}

async fn shell_window_eval(method: &str, is_enabled: bool) -> anyhow::Result<()> {
    let reverse_keyword = if is_enabled { "" } else { "un" };
    let command = format!(
        "global.display.focus_window.{}{}()",
        reverse_keyword, method
    );

    let connection = zbus::Connection::session().await?;
    let reply = connection
        .call_method(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell",
            Some("org.gnome.Shell"),
            "Eval",
            &command,
        )
        .await?;
    let (is_success, message) = reply.body::<(bool, String)>()?;

    if !is_success {
        anyhow::bail!(message);
    };

    Ok(())
}
