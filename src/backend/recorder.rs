use gst::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

use once_cell::sync::Lazy;
use std::{cell::Cell, cell::RefCell, rc::Rc};

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct KhaRecorder {
        pub state: Rc<RefCell<gst::State>>,
        pub pipeline: RefCell<Option<gst::Pipeline>>,
        pub is_readying: Cell<bool>,
        pub video_format: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaRecorder {
        const NAME: &'static str = "KhaRecorder";
        type Type = super::KhaRecorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Rc::new(RefCell::new(gst::State::Null)),
                pipeline: RefCell::new(None),
                is_readying: Cell::new(false),
                video_format: RefCell::new("".to_string()),
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
                    glib::ParamSpec::new_string(
                        "video-format",
                        "video-format",
                        "Video format",
                        None,
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
                    let is_readying = value.get().unwrap();
                    self.is_readying.set(is_readying);
                }
                "video-format" => {
                    let video_format = value.get().unwrap();
                    self.video_format.replace(video_format);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "is-readying" => self.is_readying.get().to_value(),
                "video-format" => self.video_format.borrow().to_value(),
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
        obj
    }

    fn get_private(&self) -> &imp::KhaRecorder {
        &imp::KhaRecorder::from_instance(self)
    }

    fn set_state(&self, new_state: gst::State) {
        let self_ = self.get_private();

        let mut state = self_.state.borrow_mut();
        *state = new_state;
        let pipeline = self_.pipeline.borrow_mut().take().unwrap();
        pipeline
            .set_state(new_state)
            .expect("Failed to set pipeline state");
    }

    pub fn start(&self) {
        let self_ = self.get_private();

        gstgif::plugin_register_static().expect("Failed to register gif plugin");

        let pipeline_string = format!("videotestsrc num-buffers=100 ! videoconvert ! gifenc speed=30 ! filesink location=/home/dave/test.gif");
        let gst_pipeline = gst::parse_launch(&pipeline_string).expect("Failed to parse pipeline");
        let gst_pipeline = gst_pipeline
            .downcast::<gst::Pipeline>()
            .expect("Couldn't downcast pipeline");
        self_.pipeline.replace(Some(gst_pipeline));

        self.set_state(gst::State::Playing);
    }

    pub fn stop(&self) {
        self.set_state(gst::State::Null);
    }
}
