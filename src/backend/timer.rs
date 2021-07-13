use gtk::{
    glib::{self, clone, Continue, GEnum},
    prelude::*,
    subclass::prelude::*,
};

use std::{cell::Cell, cell::RefCell, rc::Rc};

#[repr(u32)]
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Copy, GEnum)]
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

    pub struct KhaTimer {
        pub state: Rc<RefCell<TimerState>>,
        pub time: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaTimer {
        const NAME: &'static str = "KhaTimer";
        type Type = super::KhaTimer;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                state: Rc::new(RefCell::new(TimerState::default())),
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
                    self.state.replace(state);
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
                "state" => self.state.borrow().to_value(),
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

    pub fn start(&self, delay: u32) {
        self.set_property("time", delay).unwrap();

        glib::timeout_add_seconds_local(
            1,
            clone!(@weak self as timer => @default-return Continue(false), move || {
                let current_state = timer.property("state").unwrap().get::<TimerState>().unwrap();
                let current_time = timer.property("time").unwrap().get::<u32>().unwrap();

                if current_state == TimerState::Stopped {
                    return Continue(false);
                }

                if current_state != TimerState::Paused {
                    let new_time = match current_state {
                        TimerState::Delayed => current_time - 1,
                        _ => current_time + 1,
                    };
                    timer.set_property("time", new_time).unwrap();

                    if new_time == 0 && current_state == TimerState::Delayed {
                        timer.set_property("state", TimerState::Running).unwrap();
                        timer.emit_by_name("delay-done", &[]).unwrap();
                    }
                }

                Continue(true)
            }),
        );

        if delay == 0 {
            self.set_property("state", TimerState::Running).unwrap();
            self.emit_by_name("delay-done", &[]).unwrap();
        } else {
            self.set_property("state", TimerState::Delayed).unwrap();
        }
    }

    pub fn pause(&self) {
        self.set_property("state", TimerState::Paused).unwrap();
    }

    pub fn resume(&self) {
        self.set_property("state", TimerState::Running).unwrap();
    }

    pub fn stop(&self) {
        self.set_property("state", TimerState::Stopped).unwrap();
    }
}
