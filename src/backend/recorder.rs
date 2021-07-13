use crate::backend::KhaScreencastPortal;
use crate::backend::Stream;

use gst::prelude::*;
use gtk::{
    glib::{self, clone, GEnum},
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::{cell::Cell, cell::RefCell, rc::Rc};

#[repr(u32)]
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderState")]
pub enum RecorderState {
    Null,
    Paused,
    Playing,
}

impl Default for RecorderState {
    fn default() -> Self {
        RecorderState::Null
    }
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct KhaRecorder {
        pub pipeline: Rc<RefCell<Option<gst::Pipeline>>>,
        pub portal: KhaScreencastPortal,
        pub is_readying: Cell<bool>,
        pub state: Rc<RefCell<RecorderState>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaRecorder {
        const NAME: &'static str = "KhaRecorder";
        type Type = super::KhaRecorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Rc::new(RefCell::new(RecorderState::default())),
                portal: KhaScreencastPortal::new(),
                is_readying: Cell::new(false),
                pipeline: Rc::new(RefCell::new(None)),
            }
        }
    }

    impl ObjectImpl for KhaRecorder {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_boolean(
                        "is-readying",
                        "is-readying",
                        "Is readying",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_enum(
                        "state",
                        "state",
                        "State",
                        RecorderState::static_type(),
                        RecorderState::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "is-readying" => {
                    self.is_readying.set(value.get().unwrap());
                }
                "state" => {
                    let state = value.get().unwrap();
                    self.state.replace(state);

                    let pipeline = self.pipeline.borrow();
                    let pipeline = pipeline.as_ref().expect("Failed to get pipeline");
                    let pipeline_state = match state {
                        RecorderState::Null => gst::State::Null,
                        RecorderState::Paused => gst::State::Paused,
                        RecorderState::Playing => gst::State::Playing,
                    };
                    pipeline
                        .set_state(pipeline_state)
                        .expect("Failed to set pipeline state");
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "is-readying" => self.is_readying.get().to_value(),
                "state" => self.state.borrow().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct KhaRecorder(ObjectSubclass<imp::KhaRecorder>);
}

impl KhaRecorder {
    pub fn new() -> Self {
        let obj: Self =
            glib::Object::new::<Self>(&[]).expect("Failed to initialize Recorder object");

        obj.setup_signals();

        obj
    }

    fn private(&self) -> &imp::KhaRecorder {
        &imp::KhaRecorder::from_instance(self)
    }

    fn setup_signals(&self) {
        let imp = self.private();

        imp.portal
            .connect_local(
                "ready",
                false,
                clone!(@weak self as rec => @default-return None, move | args | {
                    let stream = args[1].get().unwrap();

                    rec.build_pipeline(stream);

                    None
                }),
            )
            .expect("Could not connect to ready signal.");
    }

    fn build_pipeline(&self, stream: Stream) {
        let imp = self.private();

        let fd = stream.fd;
        let node_id = stream.node_id;

        println!("{}", fd);
        println!("{}", node_id);
        println!("{}", stream.screen.width);
        println!("{}", stream.screen.height);

        let pipeline_string = format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! videoconvert ! queue ! vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=3 ! queue ! webmmux ! filesink location=/home/dave/test.webm", fd, node_id);
        let gst_pipeline = gst::parse_launch(&pipeline_string).expect("Failed to parse pipeline");
        let gst_pipeline = gst_pipeline
            .downcast::<gst::Pipeline>()
            .expect("Couldn't downcast pipeline");
        imp.pipeline.replace(Some(gst_pipeline));

        self.set_property("state", RecorderState::Playing).unwrap();
    }

    pub fn start(&self) {
        let imp = self.private();
        imp.portal.open();
    }

    pub fn stop(&self) {
        let imp = self.private();
        imp.portal.close();

        // self.set_property("state", RecorderState::Null).unwrap();
    }

    pub fn portal(&self) -> &KhaScreencastPortal {
        let imp = self.private();
        &imp.portal
    }
}
