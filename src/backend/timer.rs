use gtk::{
    glib::{self, clone, Continue, GEnum},
    prelude::*,
    subclass::prelude::*,
};

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

    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    use std::cell::Cell;

    #[derive(Debug)]
    pub struct KhaTimer {
        pub state: Cell<TimerState>,
        pub time: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaTimer {
        const NAME: &'static str = "KhaTimer";
        type Type = super::KhaTimer;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Cell::new(TimerState::default()),
                time: Cell::new(0),
            }
        }
    }

    impl ObjectImpl for KhaTimer {
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
                        "State",
                        TimerState::static_type(),
                        TimerState::default() as i32,
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
    pub struct KhaTimer(ObjectSubclass<imp::KhaTimer>);
}

impl KhaTimer {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create KhaTimer")
    }

    fn set_state(&self, new_state: TimerState) {
        self.set_property("state", new_state)
            .expect("KhaTimer failed to set state");
    }

    pub fn state(&self) -> TimerState {
        self.property("state")
            .unwrap()
            .get::<TimerState>()
            .expect("KhaTimer failed to get state")
    }

    fn set_time(&self, new_time: u32) {
        self.set_property("time", new_time)
            .expect("KhaTimer failed to set time");
    }

    fn time(&self) -> u32 {
        self.property("time")
            .unwrap()
            .get::<u32>()
            .expect("KhaTimer failed to get time")
    }

    pub fn start(&self, delay: u32) {
        self.set_time(delay);

        glib::timeout_add_seconds_local(
            1,
            clone!(@weak self as obj => @default-return Continue(true), move || {
                let current_state = obj.state();
                let current_time = obj.time();

                if current_state == TimerState::Stopped {
                    return Continue(false);
                }

                if current_state != TimerState::Paused {
                    let new_time = match current_state {
                        TimerState::Delayed => current_time - 1,
                        _ => current_time + 1,
                    };

                    obj.set_time(new_time);

                    if new_time == 0 && current_state == TimerState::Delayed {
                        obj.set_state(TimerState::Running);
                        obj.emit_by_name("delay-done", &[]).unwrap();
                    }
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
