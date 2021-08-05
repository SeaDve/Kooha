use gst::prelude::*;
use gtk::{
    glib::{self, clone, subclass::Signal, GEnum, SignalHandlerId},
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::cell::Cell;

use crate::{
    backend::{Recorder, RecorderResponse, RecorderState, Timer, TimerState},
    widgets::MainWindow,
};

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderControllerState")]
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

    #[derive(Debug)]
    pub struct RecorderController {
        pub recorder: Recorder,
        pub timer: Timer,
        pub state: Cell<RecorderControllerState>,
        pub time: Cell<u32>,
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
                record_delay: Cell::new(0),
            }
        }
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
                    glib::ParamSpec::new_enum(
                        "state",
                        "state",
                        "Current state of Self",
                        RecorderControllerState::static_type(),
                        RecorderControllerState::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_uint(
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
        glib::Object::new::<Self>(&[]).expect("Failed to create RecorderController")
    }

    fn private(&self) -> &imp::RecorderController {
        imp::RecorderController::from_instance(self)
    }

    fn setup_signals(&self) {
        let imp = self.private();

        imp.timer.bind_property("time", self, "time").build();

        imp.timer
            .connect_state_notify(clone!(@weak self as obj => move |timer, _| {
                let new_state = match timer.state() {
                    TimerState::Stopped => RecorderControllerState::Null,
                    TimerState::Delayed => RecorderControllerState::Delayed,
                    TimerState::Paused => RecorderControllerState::Paused,
                    TimerState::Running => RecorderControllerState::Recording,
                };
                obj.set_state(new_state);
            }));

        imp.recorder
            .connect_state_notify(clone!(@weak self as obj => move |recorder, _| {
                let imp = obj.private();

                match recorder.state() {
                    RecorderState::Null => imp.timer.stop(),
                    RecorderState::Paused => imp.timer.pause(),
                    RecorderState::Playing => imp.timer.resume(),
                    RecorderState::Flushing => obj.set_state(RecorderControllerState::Flushing),
                };
            }));

        imp.timer
            .connect_delay_done(clone!(@weak self as obj => @default-return None, move |_| {
                let imp = obj.private();
                imp.recorder.start();
                None
            }));

        imp.recorder
            .connect_ready(clone!(@weak self as obj => @default-return None, move |_| {
                let imp = obj.private();
                let record_delay = imp.record_delay.get();
                imp.timer.start(record_delay);
                None
            }));

        imp.recorder.connect_response(
            clone!(@weak self as obj => @default-return None, move |args| {
                let response = args[1].get().unwrap();
                obj.emit_response(response);
                None
            }),
        );
    }

    fn emit_response(&self, response: &RecorderResponse) {
        self.emit_by_name("response", &[response]).unwrap();
    }

    fn set_state(&self, state: RecorderControllerState) {
        self.set_property("state", state).unwrap();
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

    pub fn set_window(&self, window: &MainWindow) {
        let imp = self.private();
        imp.recorder.set_window(window);
    }

    pub fn connect_state_notify<F: Fn(&Self, &glib::ParamSpec) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_notify_local(Some("state"), f)
    }

    pub fn connect_time_notify<F: Fn(&Self, &glib::ParamSpec) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_notify_local(Some("time"), f)
    }

    pub fn connect_response<F: Fn(&[glib::Value]) -> Option<glib::Value> + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("response", false, f).unwrap()
    }

    pub fn start(&self, record_delay: u32) {
        let imp = self.private();
        imp.record_delay.set(record_delay);

        imp.recorder.ready();
    }

    pub fn cancel_delay(&self) {
        let imp = self.private();
        imp.recorder.cancel();
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
