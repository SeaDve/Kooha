mod view_port;

use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use futures_channel::oneshot::{self, Sender};
use gettextrs::gettext;
use gst::prelude::*;
use gtk::{
    gdk,
    glib::{self, clone},
    graphene::Rect,
};

use std::{cell::RefCell, os::unix::prelude::RawFd};

pub use self::view_port::Selection;
use self::view_port::ViewPort;
use crate::{application::Application, cancelled::Cancelled, pipeline, screencast_session::Stream};

const PREVIEW_FRAMERATE: u32 = 60;
const ASSUMED_HEADER_BAR_HEIGHT: f64 = 47.0;
const WINDOW_TO_MONITOR_SCALE_FACTOR: f64 = 0.4;

#[derive(Debug)]
pub struct SelectAreaData {
    /// Selection relative to paintable_rect
    pub selection: Selection,
    /// The geometry of paintable where the stream is displayed
    pub paintable_rect: Rect,
    /// Actual stream size
    pub stream_size: (i32, i32),
}

mod imp {
    use std::cell::OnceCell;

    use super::*;
    use gst::bus::BusWatchGuard;
    use gtk::CompositeTemplate;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/seadve/Kooha/ui/area-selector.ui")]
    pub struct AreaSelector {
        #[template_child]
        pub(super) window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub(super) done_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) loading: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) view_port: TemplateChild<ViewPort>,

        pub(super) pipeline: OnceCell<gst::Pipeline>,
        pub(super) stream_size: OnceCell<(i32, i32)>,
        pub(super) result_tx: RefCell<Option<Sender<Result<(), Cancelled>>>>,
        pub(super) async_done_tx: RefCell<Option<Sender<Result<(), Cancelled>>>>,
        pub(super) bus_watch_guard: OnceCell<BusWatchGuard>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "KoohaAreaSelector";
        type Type = super::AreaSelector;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("area-selector.cancel", None, move |obj, _, _| {
                if let Some(tx) = obj.imp().async_done_tx.take() {
                    let _ = tx.send(Err(Cancelled::new("area select loading")));
                }

                if let Some(tx) = obj.imp().result_tx.take() {
                    let _ = tx.send(Err(Cancelled::new("area select")));
                    obj.close();
                } else {
                    tracing::error!("Sent result twice");
                }
            });

            klass.install_action("area-selector.done", None, move |obj, _, _| {
                if let Some(tx) = obj.imp().result_tx.take() {
                    let _ = tx.send(Ok(()));
                    obj.close();
                } else {
                    tracing::error!("Sent response twice");
                }
            });

            klass.install_action("area-selector.reset", None, move |obj, _, _| {
                obj.imp().view_port.set_selection(None::<Selection>);
            });

            klass.add_binding_action(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "area-selector.cancel",
            );
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AreaSelector {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            self.view_port
                .connect_selection_notify(clone!(@weak obj => move |_| {
                    obj.update_selection_ui();
                }));

            self.view_port
                .connect_paintable_rect_notify(clone!(@weak obj => move |_| {
                    obj.update_selection_ui();
                }));

            obj.update_selection_ui();
        }

        fn dispose(&self) {
            if let Some(pipeline) = self.pipeline.get() {
                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::warn!("Failed to set pipeline to Null: {}", err);
                }
            }
        }
    }

    impl WidgetImpl for AreaSelector {}

    impl WindowImpl for AreaSelector {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();

            obj.save_selection();

            self.parent_close_request()
        }
    }

    impl AdwWindowImpl for AreaSelector {}
}

glib::wrapper! {
    pub struct AreaSelector(ObjectSubclass<imp::AreaSelector>)
        @extends gtk::Widget, gtk::Window, adw::Window,
        @implements gtk::Native;
}

impl AreaSelector {
    pub async fn present(
        fd: RawFd,
        streams: &[Stream],
        restore_selection: bool,
        parent: &impl IsA<gtk::Window>,
    ) -> Result<SelectAreaData> {
        let this: Self = glib::Object::new();
        let imp = this.imp();

        this.set_transient_for(Some(parent));
        this.set_modal(true);

        // Setup window size
        let parent = parent.as_ref();
        let scale_factor = WINDOW_TO_MONITOR_SCALE_FACTOR / parent.scale_factor() as f64;
        let surface = parent.surface().context("Parent has no surface")?;
        let monitor_geometry = RootExt::display(parent)
            .monitor_at_surface(&surface)
            .context("No monitor found")?
            .geometry();
        this.set_default_width(
            (monitor_geometry.width() as f64 * scale_factor - ASSUMED_HEADER_BAR_HEIGHT * 2.0)
                as i32,
        );
        this.set_default_height((monitor_geometry.height() as f64 * scale_factor) as i32);

        imp.stack.set_visible_child(&imp.loading.get());

        let (result_tx, result_rx) = oneshot::channel();
        imp.result_tx.replace(Some(result_tx));

        // Setup pipeline
        let pipeline = gst::Pipeline::new();
        let videosrc_bin = pipeline::make_pipewiresrc_bin(fd, streams, PREVIEW_FRAMERATE, None)?;
        let videoconvert = gst::ElementFactory::make("videoconvert").build()?;
        let sink = gst::ElementFactory::make("gtk4paintablesink").build()?;
        pipeline.add_many([videosrc_bin.upcast_ref(), &videoconvert, &sink])?;
        gst::Element::link_many([videosrc_bin.upcast_ref(), &videoconvert, &sink])?;
        imp.pipeline.set(pipeline.clone()).unwrap();

        // Setup paintable
        let paintable = sink.property::<gdk::Paintable>("paintable");
        imp.view_port.set_paintable(Some(paintable));

        if restore_selection {
            this.restore_selection();
        }

        pipeline.set_state(gst::State::Playing)?;

        let (async_done_tx, async_done_rx) = oneshot::channel();
        imp.async_done_tx.replace(Some(async_done_tx));

        // Setup bus to receive async done message
        let bus_watch_guard = pipeline
            .bus()
            .unwrap()
            .add_watch_local(
                clone!(@weak this as obj => @default-return glib::ControlFlow::Break, move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();
        imp.bus_watch_guard.set(bus_watch_guard).unwrap();

        this.present();

        // Wait for pipeline to be on playing state
        async_done_rx.await.unwrap()?;

        imp.stack.set_visible_child(&imp.view_port.get());

        // Get stream size
        let caps = videosrc_bin
            .static_pad("src")
            .context("Videosrc bin has no src pad")?
            .current_caps()
            .context("Videosrc bin src pad has no currentcaps")?;
        let caps_struct = caps
            .structure(0)
            .context("Videosrc bin src pad caps has no structure")?;
        let stream_width = caps_struct.get::<i32>("width")?;
        let stream_height = caps_struct.get::<i32>("height")?;
        imp.stream_size.set((stream_width, stream_height)).unwrap();
        this.update_selection_ui();

        // Wait for user response
        result_rx.await.unwrap()?;

        Ok(SelectAreaData {
            selection: imp.view_port.selection().unwrap(),
            paintable_rect: imp.view_port.paintable_rect().unwrap(),
            stream_size: (stream_width, stream_height),
        })
    }

    fn restore_selection(&self) {
        let imp = self.imp();

        let app = Application::get();
        let settings = app.settings();

        let selection = settings.selection();
        if selection != settings.selection_default_value() {
            imp.view_port.set_selection(Some(selection));
        }
    }

    fn save_selection(&self) {
        let imp = self.imp();

        let app = Application::get();
        let settings = app.settings();

        if let Some(selection) = imp.view_port.selection() {
            settings.set_selection(selection);
        } else {
            settings.reset_selection();
        }
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::ControlFlow {
        use gst::MessageView;

        let imp = self.imp();

        match message.view() {
            MessageView::AsyncDone(_) => {
                if let Some(async_done_tx) = imp.async_done_tx.take() {
                    let _ = async_done_tx.send(Ok(()));
                }

                glib::ControlFlow::Continue
            }
            MessageView::Eos(_) => {
                tracing::debug!("Eos signal received from record bus");

                glib::ControlFlow::Break
            }
            MessageView::StateChanged(sc) => {
                let new_state = sc.current();

                if message.src()
                    != imp
                        .pipeline
                        .get()
                        .map(|pipeline| pipeline.upcast_ref::<gst::Object>())
                {
                    tracing::trace!(
                        "`{}` changed state from `{:?}` -> `{:?}`",
                        message
                            .src()
                            .map_or_else(|| "<unknown source>".into(), |e| e.name()),
                        sc.old(),
                        new_state,
                    );
                    return glib::ControlFlow::Continue;
                }

                tracing::debug!(
                    "Pipeline changed state from `{:?}` -> `{:?}`",
                    sc.old(),
                    new_state,
                );

                glib::ControlFlow::Continue
            }
            MessageView::Error(e) => {
                tracing::error!("Received error message on bus: {:?}", e);
                glib::ControlFlow::Break
            }
            MessageView::Warning(w) => {
                tracing::warn!("Received warning message on bus: {:?}", w);
                glib::ControlFlow::Continue
            }
            MessageView::Info(i) => {
                tracing::debug!("Received info message on bus: {:?}", i);
                glib::ControlFlow::Continue
            }
            other => {
                tracing::trace!("Received other message on bus: {:?}", other);
                glib::ControlFlow::Continue
            }
        }
    }

    fn update_selection_ui(&self) {
        let imp = self.imp();
        let view_port = imp.view_port.get();

        let selection = view_port.selection();

        self.action_set_enabled("area-selector.reset", selection.is_some());
        self.action_set_enabled("area-selector.done", selection.is_some());

        if selection.is_some() {
            imp.done_button.grab_focus();
        }

        let (Some(stream_size), Some(selection)) = (imp.stream_size.get(), selection) else {
            imp.window_title.set_subtitle("");
            return;
        };

        let Some(paintable_rect) = view_port.paintable_rect() else {
            imp.window_title.set_subtitle("");
            return;
        };

        let (stream_width, stream_height) = stream_size;
        let scale_factor_h = *stream_width as f32 / paintable_rect.width();
        let scale_factor_v = *stream_height as f32 / paintable_rect.height();

        let selection_rect_scaled = selection.rect().scale(scale_factor_h, scale_factor_v);
        imp.window_title.set_subtitle(&format!(
            "{} {}×{}",
            gettext("approx."),
            selection_rect_scaled.width().round() as i32,
            selection_rect_scaled.height().round() as i32,
        ));
    }
}
