mod view_port;

use adw::{prelude::*, subclass::prelude::*};
use anyhow::{Context, Result};
use futures_channel::oneshot::{self, Sender};
use gst::prelude::*;
use gtk::{
    gdk,
    glib::{self, clone, WeakRef},
    graphene::Rect,
};
use once_cell::unsync::OnceCell;

use std::{cell::RefCell, os::unix::prelude::RawFd};

use self::view_port::{Selection, ViewPort};
use crate::{cancelled::Cancelled, pipeline, screencast_session::Stream, utils};

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
        pub(super) view_port: TemplateChild<ViewPort>,
        #[template_child]
        pub(super) window_title: TemplateChild<adw::WindowTitle>,

        pub(super) stream_size: OnceCell<(i32, i32)>,
        pub(super) sender: RefCell<Option<Sender<Result<(), Cancelled>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AreaSelector {
        const NAME: &'static str = "KoohaAreaSelector";
        type Type = super::AreaSelector;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("area-selector.cancel", None, move |obj, _, _| {
                if let Some(sender) = obj.imp().sender.take() {
                    let _ = sender.send(Err(Cancelled::new("area select")));
                    obj.close();
                } else {
                    tracing::error!("Sent response twice");
                }
            });

            klass.install_action("area-selector.done", None, move |obj, _, _| {
                if let Some(sender) = obj.imp().sender.take() {
                    let _ = sender.send(Ok(()));
                    obj.close();
                } else {
                    tracing::error!("Sent response twice");
                }
            });

            klass.install_action("area-selector.reset", None, move |obj, _, _| {
                obj.imp().view_port.reset_selection();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AreaSelector {}
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
        struct Finally {
            weak: WeakRef<gst::Pipeline>,
        }

        impl Drop for Finally {
            fn drop(&mut self) {
                if let Some(pipeline) = self.weak.upgrade() {
                    if let Err(err) = pipeline.set_state(gst::State::Null) {
                        tracing::warn!("Failed to set pipeline to null: {}", err);
                    }
                    let _ = pipeline.bus().unwrap().remove_watch();
                }
            }
        }

        let pipeline = gst::Pipeline::new(None);

        let videosrc_bin = pipeline::pipewiresrc_bin(fd, streams, PREVIEW_FRAMERATE, None)?;
        let sink = utils::make_element("gtk4paintablesink")?;

        pipeline.add_many(&[videosrc_bin.upcast_ref(), &sink])?;
        gst::Element::link_many(&[videosrc_bin.upcast_ref(), &sink])?;

        let this: Self = glib::Object::new(&[]).expect("Failed to create KoohaAreaSelector.");
        let imp = this.imp();

        imp.view_port
            .connect_selection_notify(clone!(@weak this => move |_| {
                this.update_selection_ui();
            }));

        if let Some(transient_for) = transient_for {
            let transient_for = transient_for.as_ref();

            this.set_transient_for(Some(transient_for));
            this.set_modal(true);

            let monitor_geometry = transient_for
                .display()
                .monitor_at_surface(&transient_for.surface())
                .geometry();
            this.set_default_width(
                (monitor_geometry.width() as f64 * 0.4 - ASSUMED_HEADER_BAR_HEIGHT * 2.0) as i32,
            );
            this.set_default_height((monitor_geometry.height() as f64 * 0.4) as i32);
        }

        let (result_tx, result_rx) = oneshot::channel();
        imp.sender.replace(Some(result_tx));

        let paintable = sink.property::<gdk::Paintable>("paintable");
        imp.view_port.set_paintable(Some(&paintable));

        pipeline.set_state(gst::State::Playing)?;

        let (async_done_tx, async_done_rx) = oneshot::channel();
        let mut async_done_tx = Some(async_done_tx);
        pipeline
            .bus()
            .unwrap()
            .add_watch_local(move |_, message| {
                match message.view() {
                    // gst::MessageView::Eos(_) => {}
                    // gst::MessageView::Error(_) => {}
                    // gst::MessageView::Warning(_) => {}
                    // gst::MessageView::Info(_) => {}
                    gst::MessageView::AsyncDone(_) => {
                        if let Some(tx) = async_done_tx.take() {
                            let _ = tx.send(());
                        }
                    }
                    _ => {
                        tracing::debug!("Unhandled message: {:?}", message.view());
                    }
                }

                glib::Continue(true)
            })
            .unwrap();

        // Properly dispose pipeline on return
        let _finally = Finally {
            weak: pipeline.downgrade(),
        };

        this.present();

        // Wait for pipeline to be on playing state before getting
        // stream size
        async_done_rx.await.unwrap();

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

        result_rx.await.unwrap()?;

        Ok(Data {
            selection: imp.view_port.selection().unwrap(),
            paintable_rect: imp.view_port.paintable_rect().unwrap(),
            stream_size: (stream_width, stream_height),
        })
    }

    fn update_selection_ui(&self) {
        let imp = self.imp();
        let view_port = imp.view_port.get();

        let selection = view_port.selection();

        self.action_set_enabled("area-selector.done", selection.is_some());

        if let (Some(stream_size), Some(selection)) = (imp.stream_size.get(), selection) {
            let paintable_rect = view_port.paintable_rect().unwrap();

            let (stream_width, stream_height) = stream_size;
            let scale_factor_h = *stream_width as f32 / paintable_rect.width();
            let scale_factor_v = *stream_height as f32 / paintable_rect.height();

            let selection_rect_scaled = selection.rect().scale(scale_factor_h, scale_factor_v);
            imp.window_title.set_subtitle(&format!(
                "approx. {}x{}",
                selection_rect_scaled.width().round() as i32,
                selection_rect_scaled.height().round() as i32,
            ));
        } else {
            imp.window_title.set_subtitle("");
        }
    }
}
