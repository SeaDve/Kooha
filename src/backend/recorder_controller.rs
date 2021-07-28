use gst::prelude::*;
use gtk::{
    glib::{self, clone, subclass::Signal, GEnum},
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::cell::Cell;

use crate::backend::{Recorder, RecorderState, Timer, TimerState};

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderControllerState")]
pub enum RecorderControllerState {
    Null,
    Delayed,
    Paused,
    Recording,
}

impl Default for RecorderControllerState {
    fn default() -> Self {
        RecorderControllerState::Null
    }
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct RecorderController {
        pub recorder: Recorder,
        pub timer: Timer,
        pub state: Cell<RecorderControllerState>,
        pub time: Cell<u32>,
        pub is_readying: Cell<bool>,
        pub record_delay: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecorderController {
        const NAME: &'static str = "RecorderController";
        type Type = super::RecorderController;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                recorder: Recorder::new(),
                timer: Timer::new(),
                state: Cell::new(RecorderControllerState::default()),
                time: Cell::new(0),
                is_readying: Cell::new(false),
                record_delay: Cell::new(0),
            }
        }
    }

    impl ObjectImpl for RecorderController {
        fn constructed(&self, obj: &Self::Type) {
            let imp = obj.private();
            imp.timer.bind_property("time", obj, "time").build();
            imp.recorder
                .bind_property("is-readying", obj, "is-readying")
                .build();

            self.timer.connect_notify_local(
                Some("state"),
                clone!(@weak obj => move |timer, _| {
                    let new_state = match timer.state() {
                        TimerState::Stopped => RecorderControllerState::Null,
                        TimerState::Delayed => RecorderControllerState::Delayed,
                        TimerState::Paused => RecorderControllerState::Paused,
                        TimerState::Running => RecorderControllerState::Recording,
                    };
                    obj.set_property("state", new_state).unwrap();
                }),
            );
            self.recorder.connect_notify_local(
                Some("state"),
                clone!(@weak obj => move |recorder, _| {
                    let imp = obj.private();

                    match recorder.state() {
                        RecorderState::Null => imp.timer.stop(),
                        RecorderState::Playing => imp.timer.resume(),
                        RecorderState::Paused => imp.timer.pause(),
                    };
                }),
            );

            self.timer
                .connect_local(
                    "delay-done",
                    false,
                    clone!(@weak obj => @default-return None, move |_| {
                        println!("timer delay-done");
                        None
                    }),
                )
                .unwrap();
            self.recorder
                .connect_local(
                    "ready",
                    false,
                    clone!(@weak obj => @default-return None, move |_| {
                        println!("recorder ready");
                        None
                    }),
                )
                .unwrap();
            self.recorder
                .connect_local(
                    "record-success",
                    false,
                    clone!(@weak obj => @default-return None, move |_| {
                        println!("recorder record-success");
                        None
                    }),
                )
                .unwrap();
            self.recorder
                .connect_local(
                    "record-failed",
                    false,
                    clone!(@weak obj => @default-return None, move |_| {
                        println!("recorder record-failed");
                        None
                    }),
                )
                .unwrap();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder(
                        "record-success",
                        &[String::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                    Signal::builder(
                        "record-failed",
                        &[String::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                ]
            });
            SIGNALS.as_ref()
        }

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
    pub struct RecorderController(ObjectSubclass<imp::RecorderController>);
}

impl RecorderController {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create RecorderController")
    }

    fn private(&self) -> &imp::RecorderController {
        &imp::RecorderController::from_instance(self)
    }

    pub fn state(&self) -> RecorderControllerState {
        self.property("state")
            .unwrap()
            .get::<RecorderControllerState>()
            .unwrap()
    }

    pub fn time(&self) -> u32 {
        self.property("time").unwrap().get::<u32>().unwrap()
    }

    pub fn start(&self, record_delay: u32) {
        let imp = self.private();
        imp.record_delay.set(record_delay);

        imp.timer.start(record_delay);

        imp.recorder.ready();
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
