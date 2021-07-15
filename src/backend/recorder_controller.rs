use gst::prelude::*;
use gtk::{
    glib::{self, clone, GEnum},
    subclass::prelude::*,
};

use crate::backend::TimerState;

#[derive(Debug, Eq, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderControllerState")]
pub enum RecorderControllerState {
    Null,
    Delayed,
    Paused,
    Playing,
}

impl Default for RecorderControllerState {
    fn default() -> Self {
        RecorderControllerState::Null
    }
}

mod imp {
    use super::*;

    use once_cell::sync::Lazy;

    use std::cell::Cell;

    use crate::backend::{KhaRecorder, KhaTimer};

    #[derive(Debug)]
    pub struct KhaRecorderController {
        pub recorder: KhaRecorder,
        pub timer: KhaTimer,
        pub state: Cell<RecorderControllerState>,
        pub time: Cell<u32>,
        pub is_readying: Cell<bool>,
        pub record_delay: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaRecorderController {
        const NAME: &'static str = "KhaRecorderController";
        type Type = super::KhaRecorderController;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                recorder: KhaRecorder::new(),
                timer: KhaTimer::new(),
                state: Cell::new(RecorderControllerState::default()),
                time: Cell::new(0),
                is_readying: Cell::new(false),
                record_delay: Cell::new(0),
            }
        }
    }

    impl ObjectImpl for KhaRecorderController {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_enum(
                        "state",
                        "state",
                        "State",
                        RecorderControllerState::static_type(),
                        RecorderControllerState::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_uint(
                        "time",
                        "time",
                        "Time",
                        0,
                        std::u32::MAX as u32,
                        0,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_boolean(
                        "is-readying",
                        "is-readying",
                        "Is readying",
                        false,
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
                "state" => {
                    let state = value.get().unwrap();
                    self.state.set(state);
                }
                "time" => {
                    let time = value.get().unwrap();
                    self.time.set(time);
                }
                "is-readying" => {
                    let is_readying = value.get().unwrap();
                    self.is_readying.set(is_readying);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => self.state.get().to_value(),
                "time" => self.time.get().to_value(),
                "is-readying" => self.is_readying.get().to_value(),
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
            glib::Object::new::<Self>(&[]).expect("Failed to create KhaRecorderController");
        obj.setup_signals();
        obj.setup_bindings();
        obj
    }

    fn private(&self) -> &imp::KhaRecorderController {
        &imp::KhaRecorderController::from_instance(self)
    }

    fn setup_bindings(&self) {
        let imp = self.private();
        imp.timer.bind_property("time", self, "time").build();
    }

    fn setup_signals(&self) {
        let imp = self.private();

        imp.timer.connect_notify_local(Some("state"), clone!(@weak self as reccon => move |timer, _| {
            let new_state = match timer.property("state").unwrap().get::<TimerState>().unwrap() {
                TimerState::Stopped => RecorderControllerState::Null,
                TimerState::Delayed => RecorderControllerState::Delayed,
                TimerState::Paused => RecorderControllerState::Paused,
                TimerState::Running => RecorderControllerState::Playing,
            };
            reccon.set_property("state", new_state).unwrap();
        }));
    }

    pub fn is_stopped(&self) -> bool {
        let current_state = self
            .property("state")
            .unwrap()
            .get::<RecorderControllerState>()
            .unwrap();
        current_state == RecorderControllerState::Null
    }

    pub fn is_paused(&self) -> bool {
        let current_state = self
            .property("state")
            .unwrap()
            .get::<RecorderControllerState>()
            .unwrap();
        current_state == RecorderControllerState::Paused
    }

    pub fn start(&self, record_delay: u32) {
        let imp = self.private();
        imp.record_delay.set(record_delay);

        imp.timer.start(record_delay);
    }

    pub fn cancel_delay(&self) {
        let imp = self.private();
        // imp.recorder.portal().close();

        imp.timer.stop();
    }

    pub fn stop(&self) {
        let imp = self.private();
        // imp.recorder.stop();

        imp.timer.stop();
    }

    pub fn pause(&self) {
        let imp = self.private();
        // imp.recorder.pause();

        imp.timer.pause();
    }

    pub fn resume(&self) {
        let imp = self.private();
        // imp.recorder.resume();

        imp.timer.resume();
    }
}
