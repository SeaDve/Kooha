use std::{
    cell::Cell,
    f64::consts::{FRAC_PI_2, TAU},
};

use adw::prelude::*;
use gtk::{
    cairo,
    glib::{self, clone},
    graphene::Rect,
    subclass::prelude::*,
};

const LINE_WIDTH: f64 = 4.0;

const ANIMATION_DURATION_MS: u32 = 300;

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::ProgressIcon)]
    pub struct ProgressIcon {
        #[property(get, set = Self::set_progress, minimum = 0.0, maximum = 1.0, explicit_notify)]
        pub(super) progress: Cell<f64>,

        pub(super) animation: OnceCell<adw::TimedAnimation>,
        pub(super) display_progress: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProgressIcon {
        const NAME: &'static str = "KoohaProgressIcon";
        type Type = super::ProgressIcon;
        type ParentType = gtk::Widget;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ProgressIcon {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            let animation_target = adw::CallbackAnimationTarget::new(clone!(
                #[weak]
                obj,
                move |value| {
                    let imp = obj.imp();
                    imp.display_progress.set(value);
                    obj.queue_draw();
                }
            ));
            let animation = adw::TimedAnimation::builder()
                .widget(&*obj)
                .duration(ANIMATION_DURATION_MS)
                .target(&animation_target)
                .build();
            self.animation.set(animation).unwrap();
        }
    }

    impl WidgetImpl for ProgressIcon {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();

            let width = obj.width();
            let height = obj.height();
            let color = obj.color();

            let cx = width as f64 / 2.0;
            let cy = height as f64 / 2.0;
            let radius = width as f64 / 2.0 - LINE_WIDTH / 2.0;
            let arc_end = self.display_progress.get() * TAU - FRAC_PI_2;

            let ctx = snapshot.append_cairo(&Rect::new(0.0, 0.0, width as f32, height as f32));
            ctx.set_line_width(LINE_WIDTH);
            ctx.set_line_cap(cairo::LineCap::Round);

            ctx.set_source_color(&color);
            ctx.move_to(cx, cy - radius);
            ctx.arc(cx, cy, radius, -FRAC_PI_2, arc_end);
            ctx.stroke().unwrap();

            ctx.set_source_color(&color.with_alpha(color.alpha() * 0.15));
            ctx.move_to(cx + radius * arc_end.cos(), cy + radius * arc_end.sin());
            ctx.arc(cx, cy, radius, arc_end, 3.0 * FRAC_PI_2);
            ctx.stroke().unwrap();
        }
    }

    impl ProgressIcon {
        fn set_progress(&self, progress: f64) {
            if (progress - self.progress.get()).abs() < f64::EPSILON {
                return;
            }

            let obj = self.obj();

            self.progress.set(progress);

            let animation = self.animation.get().unwrap();
            animation.set_value_from(animation.value());
            animation.set_value_to(progress);
            animation.play();

            obj.notify_progress();
        }
    }
}

glib::wrapper! {
     pub struct ProgressIcon(ObjectSubclass<imp::ProgressIcon>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ProgressIcon {
    pub fn new() -> Self {
        glib::Object::new()
    }
}
