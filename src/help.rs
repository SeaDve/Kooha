use error_stack::Result;

use std::fmt;

pub struct Help(String);

impl Help {
    fn new(msg: &str) -> Self {
        Help(msg.to_string())
    }
}

impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub trait ResultExt {
    fn attach_help(self, msg: &str) -> Self;
    fn attach_help_lazy(self, msg_func: impl FnOnce() -> String) -> Self;
}

impl<T, C> ResultExt for Result<T, C> {
    #[track_caller]
    fn attach_help(self, msg: &str) -> Self {
        match self {
            Ok(ok) => Ok(ok),
            Err(report) => Err(report.attach(Help::new(msg))),
        }
    }

    #[track_caller]
    fn attach_help_lazy(self, msg_func: impl FnOnce() -> String) -> Self {
        match self {
            Ok(ok) => Ok(ok),
            Err(report) => Err(report.attach(msg_func())),
        }
    }
}
