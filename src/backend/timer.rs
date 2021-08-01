use gtk::{
    glib::{self, clone, subclass::Signal, Continue, GEnum},
    prelude::*,
    subclass::prelude::*,
};
use once_cell::sync::Lazy;

use std::cell::Cell;

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "TimerState")]
pub enum TimerState {
    Stopped,
    Delayed,
    Paused,
    Running,
}

impl Default for TimerState {
    fn default() -> Self {
        TimerState::Stopped
    }
}

mod imp {
    use super::*;

    #[derive(Debug)]
    pub struct Timer {
        pub state: Cell<TimerState>,
        pub time: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Timer {
        const NAME: &'static str = "Timer";
        type Type = super::Timer;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Cell::new(TimerState::default()),
                time: Cell::new(0),
            }
        }
    }

    impl ObjectImpl for Timer {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("delay-done", &[], <()>::static_type().into()).build()]
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
                        TimerState::static_type(),
                        TimerState::default() as i32,
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
    pub struct Timer(ObjectSubclass<imp::Timer>);
}

impl Timer {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create Timer")
    }

    fn set_state(&self, new_state: TimerState) {
        self.set_property("state", new_state)
            .expect("Failed to set timer state");
    }

    pub fn state(&self) -> TimerState {
        self.property("state")
            .unwrap()
            .get::<TimerState>()
            .expect("Failed to get timer state")
    }

    fn set_time(&self, new_time: u32) {
        self.set_property("time", new_time)
            .expect("Failed to get timer time");
    }

    fn time(&self) -> u32 {
        self.property("time")
            .unwrap()
            .get::<u32>()
            .expect("Failed to get timer time")
    }

    fn update_time(&self) {
        let current_time = self.time();

        let new_time = if self.state() == TimerState::Delayed {
            current_time - 1
        } else {
            current_time + 1
        };

        self.set_time(new_time);
    }

    pub fn start(&self, delay: u32) {
        self.set_time(delay);

        glib::timeout_add_seconds_local(
            1,
            clone!(@weak self as obj => @default-return Continue(true), move || {
                let current_state = obj.state();

                if current_state == TimerState::Stopped {
                    return Continue(false);
                }

                if current_state != TimerState::Paused {
                    obj.update_time();
                }

                if obj.time() == 0 && current_state == TimerState::Delayed {
                    obj.set_state(TimerState::Running);
                    obj.emit_by_name("delay-done", &[]).unwrap();
                }

                Continue(true)
            }),
        );

        if delay == 0 {
            self.set_state(TimerState::Running);
            self.emit_by_name("delay-done", &[]).unwrap();
        } else {
            self.set_state(TimerState::Delayed);
        }
    }

    pub fn pause(&self) {
        self.set_state(TimerState::Paused);
    }

    pub fn resume(&self) {
        self.set_state(TimerState::Running);
    }

    pub fn stop(&self) {
        self.set_state(TimerState::Stopped);
    }
}
