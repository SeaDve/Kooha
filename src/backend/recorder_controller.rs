use gst::prelude::*;
use gtk::{
    glib::{self, clone},
    subclass::prelude::*,
};

use std::cell::Cell;

use crate::backend::{Recorder, RecorderResponse, RecorderState, Timer, TimerState};

#[derive(Debug, PartialEq, Clone, Copy, glib::Enum)]
#[enum_type(name = "KoohaRecorderControllerState")]
pub enum RecorderControllerState {
    Null,
    Delayed,
    Paused,
    Recording,
    Flushing,
}

impl Default for RecorderControllerState {
    fn default() -> Self {
        RecorderControllerState::Null
    }
}

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    #[derive(Debug, Default)]
    pub struct RecorderController {
        pub state: Cell<RecorderControllerState>,
        pub time: Cell<u32>,

        pub record_delay: Cell<u32>,
        pub recorder: Recorder,
        pub timer: Timer,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecorderController {
        const NAME: &'static str = "KoohaRecorderController";
        type Type = super::RecorderController;
    }

    impl ObjectImpl for RecorderController {
        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);

            obj.setup_signals();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder(
                    "response",
                    &[RecorderResponse::static_type().into()],
                    <()>::static_type().into(),
                )
                .build()]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecEnum::new(
                        "state",
                        "state",
                        "Current state of Self",
                        RecorderControllerState::static_type(),
                        RecorderControllerState::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpecUInt::new(
                        "time",
                        "time",
                        "Current time",
                        0,
                        std::u32::MAX as u32,
                        0,
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
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "state" => self.state.get().to_value(),
                "time" => self.time.get().to_value(),
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
        glib::Object::new::<Self>(&[]).expect("Failed to create RecorderController.")
    }

    fn private(&self) -> &imp::RecorderController {
        imp::RecorderController::from_instance(self)
    }

    fn setup_signals(&self) {
        let imp = self.private();

        imp.timer.bind_property("time", self, "time").build();

        imp.timer
            .connect_state_notify(clone!(@weak self as obj => move |timer| {
                let new_state = match timer.state() {
                    TimerState::Stopped => RecorderControllerState::Null,
                    TimerState::Delayed => RecorderControllerState::Delayed,
                    TimerState::Paused => RecorderControllerState::Paused,
                    TimerState::Running => RecorderControllerState::Recording,
                };
                obj.set_state(new_state);
            }));

        imp.recorder
            .connect_state_notify(clone!(@weak self as obj => move |recorder| {
                let imp = obj.private();

                match recorder.state() {
                    RecorderState::Null => imp.timer.stop(),
                    RecorderState::Paused => imp.timer.pause(),
                    RecorderState::Playing => imp.timer.resume(),
                    RecorderState::Flushing => obj.set_state(RecorderControllerState::Flushing),
                };
            }));

        imp.recorder
            .connect_response(clone!(@weak self as obj => move |_, response| {
                obj.emit_response(response);
            }));

        imp.timer
            .connect_delay_done(clone!(@weak self as obj => move |_| {
                let imp = obj.private();
                imp.recorder.start();
            }));

        imp.recorder
            .connect_prepared(clone!(@weak self as obj => move |_| {
                let imp = obj.private();
                let record_delay = imp.record_delay.take();
                imp.timer.start(record_delay);
            }));
    }

    fn emit_response(&self, response: &RecorderResponse) {
        self.emit_by_name::<()>("response", &[response]);
    }

    fn set_state(&self, state: RecorderControllerState) {
        self.set_property("state", state);
    }

    pub fn state(&self) -> RecorderControllerState {
        self.property("state")
    }

    pub fn time(&self) -> u32 {
        self.property("time")
    }

    pub fn connect_state_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("state"), move |obj, _| f(obj))
    }

    pub fn connect_time_notify<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_notify_local(Some("time"), move |obj, _| f(obj))
    }

    pub fn connect_response<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, &RecorderResponse) + 'static,
    {
        self.connect_local("response", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let response = values[1].get::<RecorderResponse>().unwrap();
            f(&obj, &response);
            None
        })
    }

    pub fn start(&self, record_delay: u32) {
        let imp = self.private();
        imp.record_delay.set(record_delay);
        imp.recorder.prepare();
    }

    pub fn cancel_delay(&self) {
        let imp = self.private();
        imp.recorder.cancel_prepare();
        imp.timer.stop();
    }

    pub fn stop(&self) {
        let imp = self.private();
        imp.recorder.stop();
    }

    pub fn pause(&self) {
        let imp = self.private();
        imp.recorder.pause();
    }

    pub fn resume(&self) {
        let imp = self.private();
        imp.recorder.resume();
    }
}

impl Default for RecorderController {
    fn default() -> Self {
        Self::new()
    }
}
