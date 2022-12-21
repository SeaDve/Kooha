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
use once_cell::unsync::OnceCell;

use std::{cell::RefCell, os::unix::prelude::RawFd};

use self::view_port::{Selection, ViewPort};
use crate::{cancelled::Cancelled, pipeline, screencast_session::Stream};

const PREVIEW_FRAMERATE: u32 = 60;
const ASSUMED_HEADER_BAR_HEIGHT: f64 = 47.0;

#[derive(Debug)]
pub struct Data {
    /// Selection relative to paintable_rect
    pub selection: Selection,
    /// The geometry of paintable where the stream is displayed
    pub paintable_rect: Rect,
    /// Actual stream size
    pub stream_size: (i32, i32),
}

mod imp {
    use super::*;
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
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "KoohaAreaSelector";
        type Type = super::AreaSelector;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("area-selector.cancel", None, move |obj, _, _| {
                if let Some(sender) = obj.imp().async_done_tx.take() {
                    let _ = sender.send(Err(Cancelled::new("area select loading")));
                }

                if let Some(sender) = obj.imp().result_tx.take() {
                    let _ = sender.send(Err(Cancelled::new("area select")));
                    obj.close();
                } else {
                    tracing::error!("Sent result twice");
                }
            });

            klass.install_action("area-selector.done", None, move |obj, _, _| {
                if let Some(sender) = obj.imp().result_tx.take() {
                    let _ = sender.send(Ok(()));
                    obj.close();
                } else {
                    tracing::error!("Sent response twice");
                }
            });

            klass.install_action("area-selector.reset", None, move |obj, _, _| {
                obj.imp().view_port.reset_selection();
            });

            klass.add_binding_action(
                gdk::Key::Escape,
                gdk::ModifierType::empty(),
                "area-selector.cancel",
                None,
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

            let done_button = self.done_button.get();
            obj.set_default_widget(Some(&done_button));
            obj.set_focus_widget(Some(&done_button));

            obj.update_selection_ui();
        }

        fn dispose(&self) {
            if let Some(pipeline) = self.pipeline.get() {
                if let Err(err) = pipeline.set_state(gst::State::Null) {
                    tracing::warn!("Failed to set pipeline to Null: {}", err);
                }

                let _ = pipeline.bus().unwrap().remove_watch();
            }
        }
    }

    impl WidgetImpl for AreaSelector {}
    impl WindowImpl for AreaSelector {}
    impl AdwWindowImpl for AreaSelector {}
}

glib::wrapper! {
    pub struct AreaSelector(ObjectSubclass<imp::AreaSelector>)
        @extends gtk::Widget, gtk::Window, adw::Window,
        @implements gtk::Native;
}

impl AreaSelector {
    pub async fn present(
        transient_for: Option<&impl IsA<gtk::Window>>,
        fd: RawFd,
        streams: &[Stream],
    ) -> Result<Data> {
        let this: Self = glib::Object::builder().build();
        let imp = this.imp();

        // Setup window size and transient for
        if let Some(transient_for) = transient_for {
            let transient_for = transient_for.as_ref();

            this.set_transient_for(Some(transient_for));
            this.set_modal(true);

            let monitor_geometry = RootExt::display(transient_for)
                .monitor_at_surface(&transient_for.surface())
                .geometry();
            this.set_default_width(
                (monitor_geometry.width() as f64 * 0.4 - ASSUMED_HEADER_BAR_HEIGHT * 2.0) as i32,
            );
            this.set_default_height((monitor_geometry.height() as f64 * 0.4) as i32);
        }

        imp.stack.set_visible_child(&imp.loading.get());

        let (result_tx, result_rx) = oneshot::channel();
        imp.result_tx.replace(Some(result_tx));

        // Setup pipeline
        let pipeline = gst::Pipeline::new(None);
        let videosrc_bin = pipeline::pipewiresrc_bin(fd, streams, PREVIEW_FRAMERATE, None)?;
        let sink = gst::ElementFactory::make("gtk4paintablesink").build()?;
        pipeline.add_many(&[videosrc_bin.upcast_ref(), &sink])?;
        videosrc_bin.link(&sink)?;
        imp.pipeline.set(pipeline.clone()).unwrap();

        // Setup paintable
        let paintable = sink.property::<gdk::Paintable>("paintable");
        imp.view_port.set_paintable(Some(&paintable));

        pipeline.set_state(gst::State::Playing)?;

        let (async_done_tx, async_done_rx) = oneshot::channel();
        imp.async_done_tx.replace(Some(async_done_tx));

        // Setup bus to receive async done message
        pipeline
            .bus()
            .unwrap()
            .add_watch_local(
                clone!(@weak this as obj => @default-return Continue(false), move |_, message| {
                    obj.handle_bus_message(message)
                }),
            )
            .unwrap();

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

        Ok(Data {
            selection: imp.view_port.selection().unwrap(),
            paintable_rect: imp.view_port.paintable_rect().unwrap(),
            stream_size: (stream_width, stream_height),
        })
    }

    fn handle_bus_message(&self, message: &gst::Message) -> glib::Continue {
        use gst::MessageView;

        let imp = self.imp();

        match message.view() {
            MessageView::AsyncDone(_) => {
                if let Some(async_done_tx) = imp.async_done_tx.take() {
                    let _ = async_done_tx.send(Ok(()));
                }

                Continue(true)
            }
            MessageView::Eos(_) => {
                tracing::debug!("Eos signal received from record bus");

                Continue(false)
            }
            MessageView::StateChanged(sc) => {
                let new_state = sc.current();

                if message.src().as_ref()
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
                    return Continue(true);
                }

                tracing::debug!(
                    "Pipeline changed state from `{:?}` -> `{:?}`",
                    sc.old(),
                    new_state,
                );

                Continue(true)
            }
            MessageView::Error(e) => {
                tracing::error!("Received error message on bus: {:?}", e);
                Continue(false)
            }
            MessageView::Warning(w) => {
                tracing::warn!("Received warning message on bus: {:?}", w);
                Continue(true)
            }
            MessageView::Info(i) => {
                tracing::debug!("Received info message on bus: {:?}", i);
                Continue(true)
            }
            other => {
                tracing::trace!("Received other message on bus: {:?}", other);
                Continue(true)
            }
        }
    }

    fn update_selection_ui(&self) {
        let imp = self.imp();
        let view_port = imp.view_port.get();

        let selection = view_port.selection();

        self.action_set_enabled("area-selector.reset", selection.is_some());
        self.action_set_enabled("area-selector.done", selection.is_some());

        if let (Some(stream_size), Some(selection)) = (imp.stream_size.get(), selection) {
            let paintable_rect = view_port.paintable_rect().unwrap();

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
        } else {
            imp.window_title.set_subtitle("");
        }
    }
}
