use gst::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;
use glib::GEnum;
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
    pub struct KhaRecorderController {
        pub state: Rc<RefCell<RecorderState>>,
        pub pipeline: RefCell<Option<gst::Pipeline>>,
        pub is_readying: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaRecorderController {
        const NAME: &'static str = "KhaRecorderController";
        type Type = super::KhaRecorderController;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Rc::new(RefCell::new(RecorderState::default())),
                pipeline: RefCell::new(None),
                is_readying: Cell::new(false),
            }
        }
    }

    impl ObjectImpl for KhaRecorderController {
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
                    let is_readying = value.get().unwrap();
                    self.is_readying.set(is_readying);
                }
                "state" => {
                    let state = value.get().unwrap();
                    self.state.replace(state);

                    let pipeline = self.pipeline.borrow_mut().take().unwrap();
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
    pub struct KhaRecorderController(ObjectSubclass<imp::KhaRecorderController>);
}

impl KhaRecorderController {
    pub fn new() -> Self {
        let obj: Self =
            glib::Object::new::<Self>(&[]).expect("Failed to initialize Recorder object");
        obj
    }

    fn get_private(&self) -> &imp::KhaRecorderController {
        &imp::KhaRecorderController::from_instance(self)
    }

}
